//! Live execution of one configured benchmark.

use super::{Error, io_error, read_config};
use crate::benchmark::BenchmarkConfig;
use crate::prepare;
use crate::project;
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
pub fn run(path: &str, json: bool, output: &mut dyn Write) -> Result<(), Error> {
    let config_path = Path::new(path);
    let text = read_config(config_path)?;
    let config: BenchmarkConfig = toml::from_str(&text)
        .map_err(|error| Error::Input(format!("cannot parse {path}: {error}")))?;
    let (project_root, project_config) = project::load(config_path)?;
    let workspace_name = if config.workspace == "asb-workspace" {
        &project_config.workspace_dir
    } else {
        &config.workspace
    };
    let workspace = project::resolve(&project_root, workspace_name);
    std::fs::create_dir_all(&workspace).map_err(|error| {
        Error::Run(format!(
            "cannot create workspace {}: {error}",
            workspace.display()
        ))
    })?;
    let workload_directory = std::env::var_os("ASB_WORKLOAD_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| project::resolve(&project_root, &project_config.workloads_dir));
    let prompt = workloads::load(&config.workload, &workload_directory).map_err(Error::Input)?;
    let codex_executable = local_agent("codex", &project_root, &project_config.tools_dir);
    let (mut program, args) = command_for(&config);
    let cli2key_agent = config.provider.eq_ignore_ascii_case("cli2key")
        && config.agent.eq_ignore_ascii_case("codex");
    if cli2key_agent {
        program = "cli2key".into();
        if let Some(local) = local_agent("cli2key", &project_root, &project_config.tools_dir) {
            program = local;
        }
    } else if let Some(local) = local_agent(&config.agent, &project_root, &project_config.tools_dir)
    {
        program = local;
    }
    let started = Instant::now();
    let mut command = Command::new(&program);
    command
        .args(args)
        .arg(prompt)
        .current_dir(&workspace)
        .stdin(Stdio::null());
    providers::configure(&mut command, &config.provider, &config.model);
    prepare::apply(
        &mut command,
        &config.provider,
        &config.model,
        codex_executable.as_deref(),
    )?;
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
    if json {
        render_json(&result, output)?;
    } else {
        render_human(&result, output)?;
    }
    if child.status.success() {
        Ok(())
    } else {
        Err(Error::Run("agent exited unsuccessfully".into()))
    }
}

fn render_json(result: &RunResult<'_>, output: &mut dyn Write) -> Result<(), Error> {
    serde_json::to_writer_pretty(&mut *output, result)
        .map_err(|error| Error::Io(error.to_string()))?;
    writeln!(output).map_err(io_error)
}

fn render_human(result: &RunResult<'_>, output: &mut dyn Write) -> Result<(), Error> {
    writeln!(
        output,
        "{} benchmark: agent={} workload={} provider={} model={} elapsed={}ms exit={}",
        result.status,
        result.agent,
        result.workload,
        result.provider,
        result.model,
        result.elapsed_ms,
        result
            .exit_code
            .map_or_else(|| "unknown".to_owned(), |code| code.to_string())
    )
    .map_err(io_error)?;
    if !result.stdout.trim().is_empty() {
        writeln!(output, "output:\n{}", excerpt(&result.stdout)).map_err(io_error)?;
    }
    if !result.stderr.trim().is_empty() {
        writeln!(output, "stderr:\n{}", excerpt(&result.stderr)).map_err(io_error)?;
    }
    Ok(())
}

fn excerpt(value: &str) -> &str {
    const MAX_HUMAN_OUTPUT: usize = 8 * 1024;
    if value.len() <= MAX_HUMAN_OUTPUT {
        value
    } else {
        &value[..value.floor_char_boundary(MAX_HUMAN_OUTPUT)]
    }
}

fn command_for(config: &BenchmarkConfig) -> (String, Vec<String>) {
    if config.provider.eq_ignore_ascii_case("cli2key") && config.agent.eq_ignore_ascii_case("codex")
    {
        return ("cli2key".into(), Vec::new());
    }
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
        "qwen-code" => ("qwen".into(), Vec::new()),
        "mini-swe" => ("mini".into(), Vec::new()),
        other => (other.into(), Vec::new()),
    }
}

fn local_agent(agent: &str, project_root: &Path, tools_dir: &str) -> Option<String> {
    let executable = match agent {
        "qwen-code" => "qwen",
        "mini-swe" => "mini",
        "cli2key" => "codex-bridge",
        "codeagent" => "code-agent",
        "workbuddy" => "codebuddy",
        other => other,
    };
    let mut candidates = Vec::new();
    if let Some(directory) = std::env::var_os("ASB_TOOL_DIR") {
        candidates.push(std::path::PathBuf::from(directory).join(executable));
    }
    candidates.push(
        project::resolve(project_root, tools_dir)
            .join(agent)
            .join("bin")
            .join(executable),
    );
    candidates.push(
        project::resolve(project_root, tools_dir)
            .join(agent)
            .join("node_modules/.bin")
            .join(executable),
    );
    candidates.push(
        project::resolve(project_root, tools_dir)
            .join(agent)
            .join("venv/bin")
            .join(executable),
    );
    candidates.push(Path::new(".asb-tools/bin").join(executable));
    if executable != agent {
        candidates.push(
            project::resolve(project_root, tools_dir)
                .join(agent)
                .join("bin")
                .join(agent),
        );
    }
    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .and_then(|candidate| candidate.canonicalize().ok())
        .map(|candidate| candidate.to_string_lossy().into_owned())
}

fn bounded(bytes: &[u8]) -> String {
    String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_OUTPUT)]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::{RunResult, render_human, render_json};

    fn result<'a>() -> RunResult<'a> {
        RunResult {
            status: "completed",
            agent: "codex",
            workload: "bug-fix",
            provider: "openrouter",
            model: "test-model",
            elapsed_ms: 12,
            exit_code: Some(0),
            stdout: "done".into(),
            stderr: String::new(),
        }
    }

    #[test]
    fn human_output_is_concise_and_not_json() {
        let mut output = Vec::new();
        render_human(&result(), &mut output).expect("human rendering succeeds");
        let text = String::from_utf8(output).expect("utf8");
        assert!(text.starts_with("completed benchmark:"));
        assert!(text.contains("output:\ndone"));
        assert!(!text.starts_with('{'));
    }

    #[test]
    fn json_output_remains_machine_readable() {
        let mut output = Vec::new();
        render_json(&result(), &mut output).expect("json rendering succeeds");
        let value: serde_json::Value = serde_json::from_slice(&output).expect("valid json");
        assert_eq!(value["status"], "completed");
        assert_eq!(value["stdout"], "done");
    }
}
