//! Shell completion script generation.

use crate::{Error, io_error};
use std::io::Write;

/// Write the generated Bash completion script.
pub(crate) fn bash(output: &mut dyn Write) -> Result<(), Error> {
    output
        .write_all(BASH_COMPLETION.as_bytes())
        .and_then(|_| output.write_all(b"\n"))
        .map_err(io_error)
}

const BASH_COMPLETION: &str = r#"# bash completion for asb
_asb() {
    local cur command subcommand
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    command="${COMP_WORDS[1]}"
    subcommand="${COMP_WORDS[2]}"

    if [[ ${COMP_CWORD} -eq 1 ]]; then
        COMPREPLY=( $(compgen -W 'init setup run provider tool workload completion' -- "$cur") )
        return
    fi

    case "$command" in
        init) COMPREPLY=( $(compgen -W '--reset' -- "$cur") ) ;;
        setup) COMPREPLY=( $(compgen -W '--agent --workload --provider --model --workspace --output' -- "$cur") ) ;;
        run) COMPREPLY=( $(compgen -W '--json' -- "$cur") ) ;;
        provider)
            if [[ ${COMP_CWORD} -eq 2 ]]; then
                COMPREPLY=( $(compgen -W 'models' -- "$cur") )
            elif [[ "$subcommand" == models ]]; then
                COMPREPLY=( $(compgen -W '--json' -- "$cur") )
            fi ;;
        tool)
            if [[ ${COMP_CWORD} -eq 2 ]]; then
                COMPREPLY=( $(compgen -W 'list install' -- "$cur") )
            elif [[ "$subcommand" == install ]]; then
                COMPREPLY=( $(compgen -W 'codex opencode codeagent workbuddy gemini qwen-code aider mini-swe openhands openjiuwen cli2key --source --dir --dry-run' -- "$cur") )
            fi ;;
        workload)
            if [[ ${COMP_CWORD} -eq 2 ]]; then
                COMPREPLY=( $(compgen -W 'list install' -- "$cur") )
            elif [[ "$subcommand" == list ]]; then
                COMPREPLY=( $(compgen -W '--dir' -- "$cur") )
            elif [[ "$subcommand" == install ]]; then
                COMPREPLY=( $(compgen -W '--name --dir' -- "$cur") )
            fi ;;
        completion) COMPREPLY=( $(compgen -W 'bash' -- "$cur") ) ;;
    esac
}
complete -F _asb asb
"#;
