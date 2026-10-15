//! Shell completion script generation.

use crate::{Error, io_error};
use std::fmt::Write as FmtWrite;
use std::io::Write;

const TOP_LEVEL: &[&str] = &[
    "init",
    "setup",
    "run",
    "provider",
    "tool",
    "workload",
    "completion",
];
const INIT_OPTIONS: &[&str] = &["--reset"];
const SETUP_OPTIONS: &[&str] = &[
    "--agent",
    "--workload",
    "--provider",
    "--model",
    "--workspace",
    "--output",
];
const RUN_OPTIONS: &[&str] = &["--json"];
const PROVIDER_SUBCOMMANDS: &[&str] = &["models"];
const PROVIDER_MODELS_OPTIONS: &[&str] = &["--json"];
const TOOL_SUBCOMMANDS: &[&str] = &["list", "install"];
const TOOL_INSTALL_VALUES: &[&str] = &[
    "codex",
    "opencode",
    "codeagent",
    "workbuddy",
    "gemini",
    "qwen-code",
    "aider",
    "mini-swe",
    "openhands",
    "openjiuwen",
    "cli2key",
    "--source",
    "--dir",
    "--dry-run",
];
const WORKLOAD_SUBCOMMANDS: &[&str] = &["list", "install"];
const WORKLOAD_LIST_OPTIONS: &[&str] = &["--dir"];
const WORKLOAD_INSTALL_OPTIONS: &[&str] = &["--name", "--dir"];
const COMPLETION_VALUES: &[&str] = &["bash"];

/// Write a Bash completion script assembled from the CLI command catalog.
pub(crate) fn bash(output: &mut dyn Write) -> Result<(), Error> {
    output
        .write_all(generate_bash().as_bytes())
        .map_err(io_error)
}

fn generate_bash() -> String {
    let mut script = String::from("# bash completion for asb\n_asb() {\n");
    script.push_str("    local cur command subcommand\n    COMPREPLY=()\n");
    script.push_str("    cur=\"${COMP_WORDS[COMP_CWORD]}\"\n");
    script.push_str("    command=\"${COMP_WORDS[1]}\"\n");
    script.push_str("    subcommand=\"${COMP_WORDS[2]}\"\n\n");
    let _ = writeln!(
        script,
        "    if [[ ${{COMP_CWORD}} -eq 1 ]]; then\n        COMPREPLY=( $(compgen -W '{}' -- \"$cur\") )\n        return\n    fi\n",
        words(TOP_LEVEL)
    );
    script.push_str("    case \"$command\" in\n");
    simple_case(&mut script, "init", INIT_OPTIONS);
    simple_case(&mut script, "setup", SETUP_OPTIONS);
    simple_case(&mut script, "run", RUN_OPTIONS);
    nested_case(
        &mut script,
        "provider",
        PROVIDER_SUBCOMMANDS,
        &[("models", PROVIDER_MODELS_OPTIONS)],
    );
    nested_case(
        &mut script,
        "tool",
        TOOL_SUBCOMMANDS,
        &[("install", TOOL_INSTALL_VALUES)],
    );
    nested_case(
        &mut script,
        "workload",
        WORKLOAD_SUBCOMMANDS,
        &[
            ("list", WORKLOAD_LIST_OPTIONS),
            ("install", WORKLOAD_INSTALL_OPTIONS),
        ],
    );
    simple_case(&mut script, "completion", COMPLETION_VALUES);
    script.push_str("    esac\n}\ncomplete -F _asb asb\n");
    script
}

fn simple_case(script: &mut String, command: &str, values: &[&str]) {
    let _ = writeln!(
        script,
        "        {command}) COMPREPLY=( $(compgen -W '{}' -- \"$cur\") ) ;;",
        words(values)
    );
}

fn nested_case(
    script: &mut String,
    command: &str,
    subcommands: &[&str],
    nested_values: &[(&str, &[&str])],
) {
    let _ = writeln!(script, "        {command})");
    let _ = writeln!(
        script,
        "            if [[ ${{COMP_CWORD}} -eq 2 ]]; then\n                COMPREPLY=( $(compgen -W '{}' -- \"$cur\") )",
        words(subcommands)
    );
    for (subcommand, values) in nested_values {
        let _ = writeln!(
            script,
            "            elif [[ \"$subcommand\" == {subcommand} ]]; then\n                COMPREPLY=( $(compgen -W '{}' -- \"$cur\") )",
            words(values)
        );
    }
    script.push_str("            fi ;;\n");
}

fn words(values: &[&str]) -> String {
    values.join(" ")
}
