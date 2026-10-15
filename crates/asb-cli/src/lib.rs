//! Small command line front end for configuring and running ASB benchmarks.

mod benchmark;
mod cli2key;
mod completion;
mod config;
mod help;
mod install;
mod models;
mod prepare;
mod project;
mod providers;
mod runner;
mod workloads;

use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::process::ExitCode;

/// Run the process CLI.
#[must_use]
pub fn entry(args: Vec<OsString>) -> ExitCode {
    ExitCode::from(run(
        &args,
        &mut io::stdout().lock(),
        &mut io::stderr().lock(),
    ))
}

/// Run one CLI request with injectable streams for tests.
pub fn run(args: &[OsString], stdout: &mut dyn Write, stderr: &mut dyn Write) -> u8 {
    let words = match args
        .iter()
        .map(|value| value.to_str().map(str::to_owned))
        .collect::<Option<Vec<_>>>()
    {
        Some(words) => words,
        None => return fail(stderr, "arguments must be valid UTF-8"),
    };
    let result = match words.as_slice() {
        [] => write_help(stdout),
        [command] if command == "--help" || command == "-h" => write_help(stdout),
        [command] if command == "--version" || command == "-V" => {
            writeln!(stdout, "asb {}", env!("CARGO_PKG_VERSION")).map_err(io_error)
        }
        [command, help_arg] if help_arg == "--help" || help_arg == "-h" => {
            help::command(command, stdout)
        }
        [command] if command == "tool" || command == "workload" || command == "provider" => {
            help::command(command, stdout)
        }
        [command] if command == "completion" => help::command(command, stdout),
        [command, subcommand, help_arg]
            if (command == "provider" && subcommand == "models")
                && (help_arg == "--help" || help_arg == "-h") =>
        {
            help::command("provider models", stdout)
        }
        [command, subcommand, help_arg]
            if (command == "tool" || command == "workload")
                && (subcommand == "list" || subcommand == "install")
                && (help_arg == "--help" || help_arg == "-h") =>
        {
            help::command(&format!("{command} {subcommand}"), stdout)
        }
        [command, shell] if command == "completion" && shell == "bash" => completion::bash(stdout),
        [command, rest @ ..] if command == "setup" => config::setup(rest, stdout),
        [command, rest @ ..] if command == "init" => project::init(rest, stdout),
        [command, path, format] if command == "run" && format == "--json" => {
            runner::run(path, true, stdout)
        }
        [command, format, path] if command == "run" && format == "--json" => {
            runner::run(path, true, stdout)
        }
        [command, path] if command == "run" => runner::run(path, false, stdout),
        [command, subcommand] if command == "tool" && subcommand == "list" => {
            install::tool_list(stdout)
        }
        [command, subcommand, rest @ ..] if command == "tool" && subcommand == "install" => {
            install::tool(rest, stdout)
        }
        [command, subcommand] if command == "workload" && subcommand == "list" => {
            install::workload_list(&[], stdout)
        }
        [command, subcommand, rest @ ..] if command == "workload" && subcommand == "list" => {
            install::workload_list(rest, stdout)
        }
        [command, subcommand, rest @ ..] if command == "workload" && subcommand == "install" => {
            install::workload(rest, stdout)
        }
        [command, subcommand, rest @ ..] if command == "provider" && subcommand == "models" => {
            models::list(rest, stdout)
        }
        _ => Err(CliError::Usage("use `asb --help` for command usage")),
    };
    match result {
        Ok(()) => 0,
        Err(error) => fail(stderr, &error.to_string()),
    }
}

fn write_help(output: &mut dyn Write) -> Result<(), CliError> {
    help::root(output)
}

fn fail(stderr: &mut dyn Write, message: &str) -> u8 {
    let _ = writeln!(stderr, "asb: {message}");
    2
}

fn io_error(error: io::Error) -> CliError {
    CliError::Io(error.to_string())
}

#[derive(Debug)]
enum CliError {
    Usage(&'static str),
    Input(String),
    Io(String),
    Run(String),
}

impl std::fmt::Display for CliError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usage(message) => formatter.write_str(message),
            Self::Input(message) | Self::Io(message) | Self::Run(message) => {
                formatter.write_str(message)
            }
        }
    }
}

impl std::error::Error for CliError {}

pub(crate) fn read_config(path: &std::path::Path) -> Result<String, CliError> {
    const MAX_CONFIG_BYTES: u64 = 64 * 1024;
    let metadata = std::fs::metadata(path)
        .map_err(|error| CliError::Input(format!("cannot read {}: {error}", path.display())))?;
    if metadata.len() > MAX_CONFIG_BYTES {
        return Err(CliError::Input(
            "benchmark configuration is larger than 64 KiB".into(),
        ));
    }
    let mut file = std::fs::File::open(path)
        .map_err(|error| CliError::Input(format!("cannot open {}: {error}", path.display())))?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .map_err(|error| CliError::Input(format!("cannot read {}: {error}", path.display())))?;
    Ok(contents)
}

pub(crate) use CliError as Error;

#[cfg(test)]
mod tests {
    use super::run;
    use std::ffi::OsString;

    #[test]
    fn every_command_has_concise_help() {
        for words in [
            vec!["init", "--help"],
            vec!["setup", "--help"],
            vec!["run", "--help"],
            vec!["provider", "--help"],
            vec!["provider", "models", "--help"],
            vec!["tool", "--help"],
            vec!["tool", "list", "--help"],
            vec!["tool", "install", "--help"],
            vec!["workload", "--help"],
            vec!["workload", "list", "--help"],
            vec!["workload", "install", "--help"],
            vec!["completion", "--help"],
        ] {
            let is_tool_install_help = words == ["tool", "install", "--help"];
            let args = words.into_iter().map(OsString::from).collect::<Vec<_>>();
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            assert_eq!(run(&args, &mut stdout, &mut stderr), 0);
            assert!(String::from_utf8_lossy(&stdout).starts_with("Usage:"));
            if is_tool_install_help {
                assert!(String::from_utf8_lossy(&stdout).contains("provider bridge"));
            }
            assert!(stderr.is_empty());
        }
    }

    #[test]
    fn bash_completion_contains_public_commands() {
        let args = [OsString::from("completion"), OsString::from("bash")];
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        assert_eq!(run(&args, &mut stdout, &mut stderr), 0);
        let script = String::from_utf8(stdout).unwrap();
        assert!(script.starts_with("# bash completion for asb"));
        assert!(script.contains("completion"));
        assert!(script.contains("cli2key"));
        for flag in [
            "--reset",
            "--agent",
            "--workload",
            "--provider",
            "--model",
            "--workspace",
            "--output",
            "--json",
            "--source",
            "--dir",
            "--dry-run",
            "--name",
        ] {
            assert!(script.contains(flag), "missing completion for {flag}");
        }
        assert!(script.contains("complete -F _asb asb"));
        assert!(stderr.is_empty());
    }
}
