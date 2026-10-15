//! Concise command-specific help text.

use crate::{Error, io_error};
use std::io::Write;

/// Write the top-level command summary.
pub fn root(output: &mut dyn Write) -> Result<(), Error> {
    write_text(
        output,
        "ASB\n\nUsage:\n  asb init [PROJECT_DIR] [--reset]\n  asb setup --agent AGENT --workload WORKLOAD --provider PROVIDER --model MODEL [--workspace DIR] [--output FILE]\n  asb run CONFIG [--json] [--measurements DIR]\n  asb record CONFIG [--cassette FILE] [--json] [--measurements DIR]\n  asb replay CONFIG [--cassette FILE] [--json] [--measurements DIR]\n  asb provider models [PROVIDER] [--json]\n  asb tool list\n  asb tool install AGENT [--source URL|PATH] [--dir DIR] [--dry-run]\n  asb workload list [--dir DIR]\n  asb workload install SOURCE.txt|URL [--name NAME] [--dir DIR]\n\nUse `asb COMMAND --help` for concise help for one command.",
    )
}

/// Write help for a command or nested subcommand.
pub fn command(name: &str, output: &mut dyn Write) -> Result<(), Error> {
    let text = match name {
        "init" => {
            "Usage: asb init [PROJECT_DIR] [--reset]\n\nCreate project state and bundled workloads. Existing projects are preserved unless --reset is supplied."
        }
        "setup" => {
            "Usage: asb setup --agent AGENT --workload WORKLOAD --provider PROVIDER --model MODEL [--workspace DIR] [--output FILE]\n\nWrite a benchmark TOML configuration. Generated configurations enable every measurement module; credentials remain in the environment."
        }
        "record" => {
            "Usage: asb record CONFIG [--cassette FILE] [--json] [--measurements DIR]\n\nRun the configured workload through the local llmtape proxy and save LLM requests and responses to a JSONL cassette. Install it project-locally with `asb tool install llmtape`, or set ASB_LLM_TAPE to another executable. The cassette is local output and is never committed automatically."
        }
        "replay" => {
            "Usage: asb replay CONFIG [--cassette FILE] [--json] [--measurements DIR]\n\nRun the configured workload against a local llmtape cassette. Replay refuses a missing cassette, does not start cli2key or any live provider, and redirects provider endpoints to the local replay server."
        }
        "run" => {
            "Usage: asb run CONFIG [--json] [--measurements DIR]\n\nRun the configured workload with the selected local agent and provider. Human-readable output is default; --json emits machine-readable output. --measurements writes bounded machine-readable system, runtime, collector, token, and success-rate data. Optional collectors are enabled with ASB_MEASURE_MPSTAT, ASB_MEASURE_IOSTAT, ASB_MEASURE_PERF, and ASB_MEASURE_BPFTRACE."
        }
        "provider" => {
            "Usage: asb provider models [PROVIDER] [--json]\n\nAccess provider-related commands. `models` lists live provider catalogs."
        }
        "provider models" => {
            "Usage: asb provider models [PROVIDER] [--json]\n\nList live models for one provider, or all supported providers when omitted."
        }
        "tool" => {
            "Usage: asb tool COMMAND\n\nCommands:\n  list                 List built-in agent installers.\n  install AGENT ...    Install one agent into the project."
        }
        "tool list" => {
            "Usage: asb tool list\n\nList every built-in agent and support-tool installer and its local executable."
        }
        "tool install" => {
            "Usage: asb tool install NAME [--source URL|PATH] [--dir DIR] [--dry-run]\n\nInstall a pinned built-in agent or support tool, or copy/download an executable with --source. `cli2key` installs the local provider bridge; `llmtape` installs the local record/replay proxy."
        }
        "workload" => {
            "Usage: asb workload COMMAND\n\nCommands:\n  list                 List workload files in the project.\n  install SOURCE ...   Copy or download a workload prompt."
        }
        "workload list" => {
            "Usage: asb workload list [--dir DIR]\n\nList installable .txt workloads in the configured directory."
        }
        "workload install" => {
            "Usage: asb workload install SOURCE.txt|URL [--name NAME] [--dir DIR]\n\nCopy or download one prompt into the project workload directory."
        }
        _ => return root(output),
    };
    write_text(output, text)
}

fn write_text(output: &mut dyn Write, text: &str) -> Result<(), Error> {
    writeln!(output, "{text}").map_err(io_error)
}
