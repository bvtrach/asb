//! Small command line front end for configuring and running ASB benchmarks.

mod benchmark;
mod config;
mod runner;

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
        [command, rest @ ..] if command == "setup" => config::setup(rest, stdout),
        [command, path] if command == "run" => runner::run(path, stdout),
        [command, help] if (command == "setup" || command == "run") && help == "--help" => {
            write_help(stdout)
        }
        _ => Err(CliError::Usage("use `asb setup` or `asb run CONFIG`")),
    };
    match result {
        Ok(()) => 0,
        Err(error) => fail(stderr, &error.to_string()),
    }
}

fn write_help(output: &mut dyn Write) -> Result<(), CliError> {
    writeln!(output, "ASB\n\nUsage:\n  asb setup --agent AGENT --workload WORKLOAD --provider PROVIDER --model MODEL [--workspace DIR] [--output FILE]\n  asb run CONFIG\n\nsetup writes a benchmark configuration. run executes it live.")
        .map_err(io_error)
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
