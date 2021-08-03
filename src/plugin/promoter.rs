use regex::Regex;
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use log::{info, trace, warn};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use shell_words;
use tinytemplate::TinyTemplate;

use crate::drbd::{DiskState, EventType, PluginUpdate, Resource};
use crate::plugin;

pub struct Promoter {
    cfg: PromoterConfig,
}

impl Promoter {
    pub fn new(cfg: PromoterConfig) -> Result<Self> {
        let names = cfg.resources.keys().cloned().collect::<Vec<String>>();
        adjust_resources(&names)?;

        for (name, res) in &cfg.resources {
            if res.runner == Runner::Systemd {
                let dependencies = SystemdDependencies {
                    dependencies_as: res.dependencies_as.clone(),
                    target_as: res.target_as.clone(),
                };
                generate_systemd_templates(name, &res.start, &dependencies)?;
                systemd_daemon_reload()?;
            }
        }

        Ok(Self { cfg })
    }
}

impl super::Plugin for Promoter {
    fn run(&self, rx: super::PluginReceiver) -> Result<()> {
        trace!("run: start");

        let type_exists = plugin::typefilter(&EventType::Exists);
        let type_change = plugin::typefilter(&EventType::Change);
        let names = self.cfg.resources.keys().cloned().collect::<Vec<String>>();
        let names = plugin::namefilter(&names);

        // set default stop actions (i.e., reversed start, and default on-stop-failure (i.e., true)
        let cfg = {
            let mut cfg = self.cfg.clone();
            for res in cfg.resources.values_mut() {
                if res.stop.is_empty() {
                    res.stop = res.start.clone();
                    res.stop.reverse();
                }
                if res.on_stop_failure == "" {
                    res.on_stop_failure = "true".to_string();
                }
            }
            cfg
        };

        for r in rx
            .into_iter()
            .filter(names)
            .filter(|x| type_exists(x) || type_change(x))
        {
            let name = r.get_name();
            let res = cfg
                .resources
                .get(&name)
                .expect("Can not happen, name filter is built from the cfg");

            match r.as_ref() {
                PluginUpdate::Resource(u) => {
                    if !u.old.may_promote && u.new.may_promote {
                        let sleep_millis = get_sleep_before_promote_ms(
                            &u.resource,
                            res.sleep_before_promote_factor,
                        );
                        info!(
                            "run: resource '{}' may promote after {}ms",
                            name, sleep_millis
                        );
                        if sleep_millis > 0 {
                            thread::sleep(Duration::from_millis(sleep_millis));
                        }
                        if start_actions(&name, &res.start, &res.runner).is_err() {
                            stop_and_on_failure(&name, res, true); // loops util success
                        }
                    }
                }
                PluginUpdate::Device(u) => {
                    if u.old.quorum && !u.new.quorum {
                        info!("run: resource '{}' lost quorum", name);
                        stop_and_on_failure(&name, res, true); // loops util success
                    }
                }
                _ => (),
            }
        }

        // stop services if configured
        for (name, res) in cfg.resources {
            if res.stop_services_on_exit {
                stop_and_on_failure(&name, &res, false); // loops util success
            }
        }

        trace!("run: exit");
        Ok(())
    }

    fn get_id(&self) -> Option<String> {
        self.cfg.id.clone()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct PromoterConfig {
    #[serde(default)]
    pub resources: HashMap<String, PromoterOptResource>,
    pub id: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct PromoterOptResource {
    #[serde(default)]
    pub start: Vec<String>,
    #[serde(default)]
    pub stop: Vec<String>,
    #[serde(default)]
    pub on_stop_failure: String,
    #[serde(default)]
    pub stop_services_on_exit: bool,
    #[serde(default)]
    pub runner: Runner,
    #[serde(default)]
    pub dependencies_as: SystemdDependency,
    #[serde(default)]
    pub target_as: SystemdDependency,
    #[serde(default = "default_promote_sleep")]
    pub sleep_before_promote_factor: f32,
}

fn default_promote_sleep() -> f32 {
    1.0
}

fn systemd_stop(unit: &str) -> Result<()> {
    info!("systemd_stop: systemctl stop {}", unit);
    plugin::map_status(Command::new("systemctl").arg("stop").arg(unit).status())
}

fn systemd_start(unit: &str) -> Result<()> {
    for action in &["reset-failed", "stop"] {
        // we really don't care
        let _ = Command::new("systemctl")
            .arg(action)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .arg(unit)
            .status();
    }

    info!("systemd_start: systemctl start {}", unit);
    plugin::map_status(Command::new("systemctl").arg("start").arg(unit).status())
}

fn action(what: &str, to: State, how: &Runner) -> Result<()> {
    match how {
        Runner::Shell => plugin::system(what),
        Runner::Systemd => match to {
            State::Start => systemd_start(what),
            State::Stop => systemd_stop(what),
        },
    }
}

fn start_actions(name: &str, actions: &[String], how: &Runner) -> Result<()> {
    match how {
        Runner::Shell => {
            for a in actions {
                action(a, State::Start, how)?;
            }
            Ok(())
        }
        Runner::Systemd => action(&format!("drbd-services@{}.target", name), State::Start, how),
    }
}

fn stop_actions(name: &str, actions: &[String], how: &Runner) -> Result<()> {
    match how {
        Runner::Shell => {
            for a in actions {
                action(a, State::Stop, how)?;
            }
            Ok(())
        }
        Runner::Systemd => action(&format!("drbd-services@{}.target", name), State::Stop, how),
    }
}

pub fn on_failure(action: &str) {
    info!("on_failure: starting on-failure action in a loop");
    loop {
        if plugin::system(action).is_ok() {
            return;
        }
        thread::sleep(Duration::from_secs(2));
    }
}

fn stop_and_on_failure(name: &str, res: &PromoterOptResource, wait_on_stop: bool) {
    match stop_actions(name, &res.stop, &res.runner) {
        Ok(_) => {
            if wait_on_stop {
                thread::sleep(Duration::from_secs(2)); // give some peer a little time to promote
            }
        }
        Err(e) => {
            warn!("stop_and_on_failure: {}", e);
            on_failure(&res.on_stop_failure); // loops until success
        }
    }
}

fn get_backing_devices(resname: &str) -> Result<Vec<String>> {
    let shlldev = Command::new("drbdadm")
        .arg("sh-ll-dev")
        .arg(resname)
        .output()?;
    if !shlldev.status.success() {
        return Err(anyhow::anyhow!(
            "'drbdadm sh-ll-dev {}' not executed successfully, stdout: '{}', stderr: '{}'",
            resname,
            String::from_utf8(shlldev.stdout).unwrap_or("<Could not convert stdout>".to_string()),
            String::from_utf8(shlldev.stderr).unwrap_or("<Could not convert stderr>".to_string())
        ));
    }

    let shlldev = String::from_utf8(shlldev.stdout)?;
    let devices: Vec<String> = shlldev.lines().map(|s| s.to_string()).collect();
    Ok(devices)
}

fn adjust_resources(to_start: &[String]) -> Result<()> {
    for res in to_start {
        for dev in get_backing_devices(res)? {
            info!(
                "adjust_resources: waiting for backing device '{}' to become ready",
                dev
            );
            while !drbd_backing_device_ready(&dev) {
                thread::sleep(Duration::from_secs(2));
            }
            info!("adjust_resources: backing device '{}' now ready", dev);
        }

        let status = Command::new("drbdadm").arg("adjust").arg(res).status()?;
        if !status.success() {
            // for now let's keep it a warning, I don't think we should fail hard here.
            warn!(
                "adjust_resources: 'drbdadm adjust {}' did not return successfully",
                res
            );
        }
    }
    Ok(())
}

fn drbd_backing_device_ready(dev: &str) -> bool {
    dev == "none"
        || match fs::metadata(dev) {
            Err(_) => false,
            Ok(meta) => meta.file_type().is_block_device(),
        }
}

const SYSTEMD_PREFIX: &str = "/run/systemd/system";
const SYSTEMD_CONF: &str = "reactor.conf";

fn generate_systemd_templates(
    name: &str,
    actions: &[String],
    strictness: &SystemdDependencies,
) -> Result<()> {
    let devices = get_backing_devices(name)?
        .into_iter()
        .filter(|d| d != "none")
        .collect();

    let prefix = Path::new(SYSTEMD_PREFIX).join(format!("drbd-promote@{}.service.d", name));
    systemd_write_unit(prefix, SYSTEMD_CONF, systemd_devices(devices, strictness)?)?;

    let mut target_requires: Vec<String> = Vec::new();

    let ocf_pattern = Regex::new(r"^ocf:(\S+):(\S+) (.*)$")?;

    for action in actions {
        let deps = match target_requires.last() {
            Some(prev) => vec![format!("drbd-promote@{}.service", name), prev.to_string()],
            None => vec![format!("drbd-promote@{}.service", name)],
        };

        let (service_name, env) = match ocf_pattern.captures(action) {
            Some(ocf) => {
                let (vendor, agent, args) = (&ocf[1], &ocf[2], &ocf[3]);
                systemd_ocf_parse_to_env(name, vendor, agent, args)?
            }
            _ => (action.to_string(), Vec::new()),
        };

        let prefix = Path::new(SYSTEMD_PREFIX).join(format!("{}.d", service_name));
        systemd_write_unit(
            prefix,
            SYSTEMD_CONF,
            systemd_unit(name, &deps, strictness, &env)?,
        )?;

        // we would not need to keep the order here, as it does not matter
        // what matters is After=, but IMO it would confuse unexperienced users
        // just keep the order, so no HashSet, the Vecs are short, does not matter.
        if target_requires.contains(&service_name) {
            return Err(anyhow::anyhow!(
                "generate_systemd_templates: Service name '{}' already used",
                service_name
            ));
        }
        target_requires.push(service_name.clone());
    }

    let prefix = Path::new(SYSTEMD_PREFIX).join(format!("drbd-services@{}.target.d", name));
    systemd_write_unit(
        prefix,
        SYSTEMD_CONF,
        systemd_target_requires(&target_requires, strictness)?,
    )
}

fn systemd_ocf_parse_to_env(
    name: &str,
    vendor: &str,
    agent: &str,
    args: &str,
) -> Result<(String, Vec<String>)> {
    let args = shell_words::split(args)?;

    if args.len() < 1 {
        anyhow::bail!("promoter::systemd_ocf: agent needs at least one argument (its name)")
    }

    let ra_name = &args[0];
    let ra_name = format!("{}_{}", ra_name, name);
    let service_name = format!("ocf.ra@{}.service", ra_name);
    let mut env = Vec::with_capacity(args.len() - 1);
    for item in &args[1..] {
        let mut split = item.splitn(2, "=");
        let add = match (split.next(), split.next()) {
            (Some(k), Some(v)) => format!("OCF_RESKEY_{}={}", k, shell_words::quote(v)),
            (Some(k), None) => format!("OCF_RESKEY_{}=", k),
            _ => continue, // skip empty items
        };
        env.push(add)
    }

    env.push(format!(
        "AGENT=/usr/lib/ocf/resource.d/{}/{}",
        vendor, agent
    ));

    Ok((service_name, env))
}

fn systemd_devices(devices: Vec<String>, strictness: &SystemdDependencies) -> Result<String> {
    const DEVICE_TEMPLATE: &str = r"[Service]
ExecStart=/lib/drbd/scripts/drbd-service-shim.sh primary %I
ExecCondition=
[Unit]
{{ for device in devices -}}
ConditionPathExists = {device | unescaped}
{strictness} = {device | systemd_path}.device
After = {device | systemd_path}.device
{{ endfor -}}";

    let mut tt = TinyTemplate::new();
    tt.add_template("devices", DEVICE_TEMPLATE)?;
    tt.add_formatter("systemd_path", |value, output| match value {
        Value::String(s) => {
            tinytemplate::format_unescaped(&Value::String(systemd_path(&s)), output)
        }
        _ => tinytemplate::format_unescaped(value, output),
    });

    #[derive(Serialize)]
    struct Context {
        devices: Vec<String>,
        strictness: String,
    }
    // filter diskless (== "none" devices)
    let result = tt.render(
        "devices",
        &Context {
            devices,
            strictness: strictness.dependencies_as.to_string(),
        },
    )?;
    Ok(result)
}

fn systemd_unit(
    name: &str,
    deps: &[String],
    strictness: &SystemdDependencies,
    env: &[String],
) -> Result<String> {
    const UNIT_TEMPLATE: &str = r"[Unit]
PartOf = drbd-services@{name}.target
{{ for dep in deps }}
{strictness} = {dep | unescaped}
After = {dep}
{{- endfor -}}

{{ for e in env }}
{{ if @first  }}
[Service]
{{ endif -}}
Environment= {e | unescaped}
{{- endfor -}}";

    let mut tt = TinyTemplate::new();
    tt.add_template("unit", UNIT_TEMPLATE)?;

    #[derive(Serialize)]
    struct Context<'a> {
        name: String,
        deps: &'a [String],
        env: &'a [String],
        strictness: String,
    }
    let result = tt.render(
        "unit",
        &Context {
            name: name.to_string(),
            deps,
            env,
            strictness: strictness.dependencies_as.to_string(),
        },
    )?;
    Ok(result)
}

fn systemd_target_requires(
    requires: &[String],
    strictness: &SystemdDependencies,
) -> Result<String> {
    const WANTS_TEMPLATE: &str = r"[Unit]
Before=drbd-reactor.service
{{- for require in requires }}
{strictness} = {require | unescaped}
{{- endfor -}}";

    let mut tt = TinyTemplate::new();
    tt.add_template("requires", WANTS_TEMPLATE)?;

    #[derive(Serialize)]
    struct Context<'a> {
        requires: &'a [String],
        strictness: String,
    }
    let result = tt.render(
        "requires",
        &Context {
            requires,
            strictness: strictness.target_as.to_string(),
        },
    )?;
    Ok(result)
}

fn systemd_write_unit(prefix: PathBuf, unit: &str, content: String) -> Result<()> {
    let path = prefix.join(unit);
    let tmp_path = prefix.join(format!("{}.tmp", unit));
    info!("systemd_write_unit: creating {:?}", path);

    fs::create_dir_all(&prefix)?;
    {
        let mut f = File::create(&tmp_path)?;
        f.write_all(content.as_bytes())?;
        f.write_all("\n".as_bytes())?;
    }
    fs::rename(tmp_path, path)?;

    Ok(())
}

fn systemd_daemon_reload() -> Result<()> {
    info!("systemd_daemon_reload: reloading daemon");
    plugin::system("systemctl daemon-reload")
}

enum State {
    Start,
    Stop,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub enum SystemdDependency {
    Wants,
    Requires,
    Requisite,
    BindsTo,
}
impl Default for SystemdDependency {
    fn default() -> Self {
        Self::Requires
    }
}
impl fmt::Display for SystemdDependency {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Wants => write!(f, "Wants"),
            Self::Requires => write!(f, "Requires"),
            Self::Requisite => write!(f, "Requisite"),
            Self::BindsTo => write!(f, "BindsTo"),
        }
    }
}

struct SystemdDependencies {
    dependencies_as: SystemdDependency,
    target_as: SystemdDependency,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub enum Runner {
    #[serde(rename = "systemd")]
    Systemd,
    #[serde(rename = "shell")]
    Shell,
}
impl Default for Runner {
    fn default() -> Self {
        Self::Systemd
    }
}

fn get_sleep_before_promote_ms(resource: &Resource, factor: f32) -> u64 {
    let max_sleep_s: i32 = resource
        .devices
        .iter()
        .map(|d| match d.disk_state {
            DiskState::Diskless => 6,
            DiskState::Attaching => 6,
            DiskState::Detaching => 6,
            DiskState::Failed => 6,
            DiskState::Negotiating => 6,
            DiskState::DUnknown => 6,
            DiskState::Inconsistent => 3,
            DiskState::Outdated => 2,
            DiskState::Consistent => 1,
            DiskState::UpToDate => 0,
        })
        .max() // if there are none, try the res file
        .unwrap_or_else(|| match get_backing_devices(&resource.name) {
            Ok(devices) if devices.contains(&"none".into()) => 6, // Diskless
            _ => 0,
        });

    // convert to ms and scale by factor
    (max_sleep_s as f32 * 1000.0 * factor).ceil() as u64
}

// inlined copy from https://crates.io/crates/libsystemd
// inlined because currently not packaged in Ubuntu Focal
pub fn systemd_path(name: &str) -> String {
    let trimmed = name.trim_matches('/');
    if trimmed.is_empty() {
        return "-".to_string();
    }

    let mut slash_seq = false;
    let parts: Vec<String> = trimmed
        .bytes()
        .filter(|b| {
            let is_slash = *b == b'/';
            let res = !(is_slash && slash_seq);
            slash_seq = is_slash;
            res
        })
        .enumerate()
        .map(|(n, b)| escape_byte(b, n))
        .collect();
    parts.join("")
}

// inlined copy from https://crates.io/crates/libsystemd
// inlined because currently not packaged in Ubuntu Focal
fn escape_byte(b: u8, index: usize) -> String {
    let c = char::from(b);
    match c {
        '/' => '-'.to_string(),
        ':' | '_' | '0'..='9' | 'a'..='z' | 'A'..='Z' => c.to_string(),
        '.' if index > 0 => c.to_string(),
        _ => format!(r#"\x{:02x}"#, b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drbd::Device;

    #[test]
    fn sleep_before_promote_ms() {
        // be careful to only use a Resource *with* devices filter out the unwarp_or_else?
        let r = Resource {
            name: "test".to_string(),
            devices: vec![
                Device {
                    disk_state: DiskState::Diskless,
                    ..Default::default()
                },
                Device {
                    disk_state: DiskState::Failed,
                    ..Default::default()
                },
                Device {
                    disk_state: DiskState::UpToDate,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert_eq!(get_sleep_before_promote_ms(&r, 1.0), 6000);
        assert_eq!(get_sleep_before_promote_ms(&r, 0.5), 3000);
    }

    #[test]
    fn test_systemd_ocf_parse_to_env() {
        let (name, env) = systemd_ocf_parse_to_env(
            "res1",
            "vendor1",
            "agent1",
            "name1 k1=v1 k2=\"with whitespace\" k3=with\\ different\\ whitespace foo empty=''",
        )
        .expect("should work");

        assert_eq!(name, "ocf.ra@name1_res1.service");
        assert_eq!(
            &env[..],
            &[
                "OCF_RESKEY_k1=v1",
                "OCF_RESKEY_k2='with whitespace'",
                "OCF_RESKEY_k3='with different whitespace'",
                "OCF_RESKEY_foo=",
                "OCF_RESKEY_empty=''",
                "AGENT=/usr/lib/ocf/resource.d/vendor1/agent1"
            ]
        );
    }

    #[test]
    fn test_systemd_devices() {
        let empty = systemd_devices(
            Vec::new(),
            &SystemdDependencies {
                target_as: SystemdDependency::Wants,
                dependencies_as: SystemdDependency::Wants,
            },
        )
        .expect("should work");

        assert_eq!(
            r"[Service]
ExecStart=/lib/drbd/scripts/drbd-service-shim.sh primary %I
ExecCondition=
[Unit]
",
            empty
        );

        let two_volumes = systemd_devices(
            vec![
                "/dev/vg0/backing0".to_string(),
                "/dev/disk/by-uuid/1123-456".to_string(),
            ],
            &SystemdDependencies {
                target_as: SystemdDependency::Wants,
                dependencies_as: SystemdDependency::Wants,
            },
        )
        .expect("should work");

        assert_eq!(
            r"[Service]
ExecStart=/lib/drbd/scripts/drbd-service-shim.sh primary %I
ExecCondition=
[Unit]
ConditionPathExists = /dev/vg0/backing0
Wants = dev-vg0-backing0.device
After = dev-vg0-backing0.device
ConditionPathExists = /dev/disk/by-uuid/1123-456
Wants = dev-disk-by\x2duuid-1123\x2d456.device
After = dev-disk-by\x2duuid-1123\x2d456.device
",
            two_volumes
        );
    }
}
