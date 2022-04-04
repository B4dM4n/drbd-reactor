# AUTOMATCALLY GENERATED by `shtab`

_shtab_drctl_subparsers=('disable' 'enable' 'status' 'restart' 'edit' 'rm' 'evict' 'cat' 'ls')

_shtab_drctl_option_strings=('-h' '--help' '-c' '--config' '--color')
_shtab_drctl_disable_option_strings=('-h' '--help' '--now')
_shtab_drctl_enable_option_strings=('-h' '--help')
_shtab_drctl_status_option_strings=('-h' '--help' '-v' '--verbose')
_shtab_drctl_restart_option_strings=('-h' '--help' '--with-targets')
_shtab_drctl_edit_option_strings=('-h' '--help' '-t' '--type')
_shtab_drctl_rm_option_strings=('-h' '--help' '-f' '--force' '--disabled')
_shtab_drctl_evict_option_strings=('-h' '--help' '-d' '--delay' '-f' '--force' '-k' '--keep-masked' '-u' '--unmask')
_shtab_drctl_cat_option_strings=('-h' '--help')
_shtab_drctl_ls_option_strings=('-h' '--help' '--disabled')



_shtab_drctl_pos_0_choices='disable enable status restart edit rm evict cat ls'
_shtab_drctl___color_choices='auto always never'
_shtab_drctl_edit__t_choices='promoter prometheus umh debugger'
_shtab_drctl_edit___type_choices='promoter prometheus umh debugger'

_shtab_drctl_pos_0_nargs=A...
_shtab_drctl__h_nargs=0
_shtab_drctl___help_nargs=0
_shtab_drctl_disable_pos_0_nargs=*
_shtab_drctl_disable__h_nargs=0
_shtab_drctl_disable___help_nargs=0
_shtab_drctl_disable___now_nargs=0
_shtab_drctl_enable_pos_0_nargs=*
_shtab_drctl_enable__h_nargs=0
_shtab_drctl_enable___help_nargs=0
_shtab_drctl_status_pos_0_nargs=*
_shtab_drctl_status__h_nargs=0
_shtab_drctl_status___help_nargs=0
_shtab_drctl_status__v_nargs=0
_shtab_drctl_status___verbose_nargs=0
_shtab_drctl_restart_pos_0_nargs=*
_shtab_drctl_restart__h_nargs=0
_shtab_drctl_restart___help_nargs=0
_shtab_drctl_restart___with_targets_nargs=0
_shtab_drctl_edit_pos_0_nargs=1
_shtab_drctl_edit__h_nargs=0
_shtab_drctl_edit___help_nargs=0
_shtab_drctl_rm_pos_0_nargs=+
_shtab_drctl_rm__h_nargs=0
_shtab_drctl_rm___help_nargs=0
_shtab_drctl_rm__f_nargs=0
_shtab_drctl_rm___force_nargs=0
_shtab_drctl_rm___disabled_nargs=0
_shtab_drctl_evict_pos_0_nargs=*
_shtab_drctl_evict__h_nargs=0
_shtab_drctl_evict___help_nargs=0
_shtab_drctl_evict__f_nargs=0
_shtab_drctl_evict___force_nargs=0
_shtab_drctl_evict__k_nargs=0
_shtab_drctl_evict___keep_masked_nargs=0
_shtab_drctl_evict__u_nargs=0
_shtab_drctl_evict___unmask_nargs=0
_shtab_drctl_cat_pos_0_nargs=*
_shtab_drctl_cat__h_nargs=0
_shtab_drctl_cat___help_nargs=0
_shtab_drctl_ls_pos_0_nargs=*
_shtab_drctl_ls__h_nargs=0
_shtab_drctl_ls___help_nargs=0
_shtab_drctl_ls___disabled_nargs=0


# $1=COMP_WORDS[1]
_shtab_compgen_files() {
  compgen -f -- $1  # files
}

# $1=COMP_WORDS[1]
_shtab_compgen_dirs() {
  compgen -d -- $1  # recurse into subdirs
}

# $1=COMP_WORDS[1]
_shtab_replace_nonword() {
  echo "${1//[^[:word:]]/_}"
}

# set default values (called for the initial parser & any subparsers)
_set_parser_defaults() {
  local subparsers_var="${prefix}_subparsers[@]"
  sub_parsers=${!subparsers_var}

  local current_option_strings_var="${prefix}_option_strings[@]"
  current_option_strings=${!current_option_strings_var}

  completed_positional_actions=0

  _set_new_action "pos_${completed_positional_actions}" true
}

# $1=action identifier
# $2=positional action (bool)
# set all identifiers for an action's parameters
_set_new_action() {
  current_action="${prefix}_$(_shtab_replace_nonword $1)"

  local current_action_compgen_var=${current_action}_COMPGEN
  current_action_compgen="${!current_action_compgen_var}"

  local current_action_choices_var="${current_action}_choices"
  current_action_choices="${!current_action_choices_var}"

  local current_action_nargs_var="${current_action}_nargs"
  if [ -n "${!current_action_nargs_var}" ]; then
    current_action_nargs="${!current_action_nargs_var}"
  else
    current_action_nargs=1
  fi

  current_action_args_start_index=$(( $word_index + 1 ))

  current_action_is_positional=$2
}

# Notes:
# `COMPREPLY`: what will be rendered after completion is triggered
# `completing_word`: currently typed word to generate completions for
# `${!var}`: evaluates the content of `var` and expand its content as a variable
#     hello="world"
#     x="hello"
#     ${!x} -> ${hello} -> "world"
_shtab_drctl() {
  local completing_word="${COMP_WORDS[COMP_CWORD]}"
  COMPREPLY=()

  prefix=_shtab_drctl
  word_index=0
  _set_parser_defaults
  word_index=1

  # determine what arguments are appropriate for the current state
  # of the arg parser
  while [ $word_index -ne $COMP_CWORD ]; do
    local this_word="${COMP_WORDS[$word_index]}"

    if [[ -n $sub_parsers && " ${sub_parsers[@]} " =~ " ${this_word} " ]]; then
      # valid subcommand: add it to the prefix & reset the current action
      prefix="${prefix}_$(_shtab_replace_nonword $this_word)"
      _set_parser_defaults
    fi

    if [[ " ${current_option_strings[@]} " =~ " ${this_word} " ]]; then
      # a new action should be acquired (due to recognised option string or
      # no more input expected from current action);
      # the next positional action can fill in here
      _set_new_action $this_word false
    fi

    if [[ "$current_action_nargs" != "*" ]] && \
       [[ "$current_action_nargs" != "+" ]] && \
       [[ "$current_action_nargs" != *"..." ]] && \
       (( $word_index + 1 - $current_action_args_start_index >= \
          $current_action_nargs )); then
      $current_action_is_positional && let "completed_positional_actions += 1"
      _set_new_action "pos_${completed_positional_actions}" true
    fi

    let "word_index+=1"
  done

  # Generate the completions

  if [[ "${completing_word}" == -* ]]; then
    # optional argument started: use option strings
    COMPREPLY=( $(compgen -W "${current_option_strings[*]}" -- "${completing_word}") )
  else
    # use choices & compgen
    COMPREPLY=( $(compgen -W "${current_action_choices}" -- "${completing_word}"; \
                  [ -n "${current_action_compgen}" ] \
                  && "${current_action_compgen}" "${completing_word}") )
  fi

  return 0
}

complete -o filenames -F _shtab_drctl drbd-reactorctl