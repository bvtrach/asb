//! Live execution of one configured benchmark.

use super::{Error, io_error, read_config};
use crate::benchmark::BenchmarkConfig;
use crate::providers;
use crate::workloads;
use serde::Serialize;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

const MAX_OUTPUT: usize = 1024 * 1024;

#[derive(Serialize)]
struct RunResult<'a> {
    status: &'a str,
    agent: &'a str,
    workload: &'a str,
    provider: &'a str,
    model: &'a str,
    elapsed_ms: u128,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// Execute the selected agent with the selected workload prompt.
pub fn run(path: &str, output: &mut dyn Write) -> Result<(), Error> {
    let config_path = Path::new(path);
    let text = read_config(config_path)?;
    let config: BenchmarkConfig = toml::from_str(&text)
        .map_err(|error| Error::Input(format!("cannot parse {path}: {error}")))?;
    let workspace = if Path::new(&config.workspace).is_absolute() {
        Path::new(&config.workspace).to_path_buf()
    } else {
        config_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(&config.workspace)
    };
    std::fs::create_dir_all(&workspace).map_err(|error| {
        Error::Run(format!(
            "cannot create workspace {}: {error}",
            workspace.display()
        ))
    })?;
    let workload_directory = std::env::var_os("ASB_WORKLOAD_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let adjacent = config_path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join("workloads");
            if adjacent.is_dir() {
                adjacent
            } else {
                Path::new("workloads").to_path_buf()
            }
        });
    let prompt = workloads::load(&config.workload, &workload_directory).map_err(Error::Input)?;
    let (program, args) = command_for(&config);
    let started = Instant::now();
    let mut command = Command::new(&program);
    command
        .args(args)
        .arg(prompt)
        .current_dir(&workspace)
        .stdin(Stdio::null());
    providers::configure(&mut command, &config.provider, &config.model);
    let child = command
        .output()
        .map_err(|error| Error::Run(format!("could not start {program}: {error}")))?;
    let result = RunResult {
        status: if child.status.success() {
            "completed"
        } else {
            "failed"
        },
        agent: &config.agent,
        workload: &config.workload,
        provider: &config.provider,
        model: &config.model,
        elapsed_ms: started.elapsed().as_millis(),
        exit_code: child.status.code(),
        stdout: bounded(&child.stdout),
        stderr: bounded(&child.stderr),
    };
    serde_json::to_writer_pretty(&mut *output, &result)
        .map_err(|error| Error::Io(error.to_string()))?;
    writeln!(output).map_err(io_error)?;
    if child.status.success() {
        Ok(())
    } else {
        Err(Error::Run("agent exited unsuccessfully".into()))
    }
}

fn command_for(config: &BenchmarkConfig) -> (String, Vec<String>) {
    match config.agent.as_str() {
        "codex" => (
            "codex".into(),
            vec!["exec".into(), "--model".into(), config.model.clone()],
        ),
        "aider" => ("aider".into(), vec!["--message".into()]),
        "opencode" => (
            "opencode".into(),
            vec!["run".into(), "--model".into(), config.model.clone()],
        ),
        other => (other.into(), Vec::new()),
    }
}

fn bounded(bytes: &[u8]) -> String {
    String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_OUTPUT)]).into_owned()
}
