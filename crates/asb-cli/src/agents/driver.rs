//! Workload/provider-neutral agent driver for isolated execution environments.
//!
//! Uses the same adapter registry as native runs. The negotiated connection
//! arrives through a private environment value, never argv or a request file.
use crate::{Error, benchmark::BenchmarkConfig, endpoint::Connection, project};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

const REQUEST: &str = "ASB_GUEST_AGENT_REQUEST";

#[cfg(test)]
#[path = "../../tests/unit/driver_validation.rs"]
mod validation_tests;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkloadOwner {
    pub controller_namespace: u64,
    pub guest_namespace: u64,
    pub token: String,
}

pub(crate) fn namespace_id() -> Result<u64, Error> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata("/proc/self/ns/pid")
            .map(|metadata| metadata.ino())
            .map_err(crate::io_error)
    }
    #[cfg(not(target_os = "linux"))]
    Err(Error::Input(
        "workload process ownership requires Linux PID namespaces".into(),
    ))
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GuestRequest {
    #[serde(default)]
    pub process_lifetime: crate::workloads::session::ProcessLifetime,
    #[serde(default)]
    pub workload_owner: Option<WorkloadOwner>,
    pub config: BenchmarkConfig,
    pub connection: Connection,
    pub prompt: String,
    pub project_root: PathBuf,
    pub runtime_dir: PathBuf,
    pub run_dir: PathBuf,
    #[serde(default)]
    pub replay: bool,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: f64,
    #[serde(default)]
    pub namespace_collectors: Vec<String>,
}

fn default_timeout() -> f64 {
    900.0
}

#[derive(Deserialize, Serialize)]
pub(crate) struct GuestResult {
    #[serde(default)]
    pub artifact_error: Option<String>,
    #[serde(default)]
    pub elapsed_ms: Option<u64>,
    #[serde(default)]
    pub timed_out: bool,
    #[serde(default)]
    pub workload_owner: Option<WorkloadOwner>,
    pub schema_version: u32,
    pub exit_code: Option<i32>,
    pub final_response: Option<String>,
    pub token_usage: crate::measurements::TokenUsage,
    pub steps: Option<u64>,
    pub execution_failure: Option<String>,
    #[serde(default)]
    pub tree_quiesced: bool,
    #[serde(default)]
    pub namespace_collectors: std::collections::BTreeMap<String, serde_json::Value>,
}

/// Remove enclosing host/guest orchestration state from a benchmark client.
pub(crate) fn isolate_parent_state(command: &mut Command) {
    for name in [
        REQUEST,
        "CODEX_REMOTE_PAYLOAD",
        "CODEX_SESSION_ID",
        "CODEX_THREAD_ID",
        "CODEX_PERMISSION_PROFILE",
        "CODEX_APPLY_PATCH_PRESERVE_LINE_ENDINGS",
        "CODEX_MANAGED_PACKAGE_ROOT",
        "CODEX_MANAGED_BY_NPM",
        "CODEX_MANAGED_BY_BUN",
        "CODEX_MANAGED_BY_PNPM",
        "CODEX_MANAGED_BY_VITE_PLUS",
        "CODEX_CI",
    ] {
        command.env_remove(name);
    }
}

fn absolute(path: &Path) -> bool {
    path.is_absolute()
        && !path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
}

struct PreparedGuest {
    command: Command,
    artifact_owner: super::ArtifactOwner,
}
impl std::fmt::Debug for PreparedGuest {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        output.write_str("PreparedGuest { private command and owned artifacts }")
    }
}
impl std::ops::Deref for PreparedGuest {
    type Target = Command;
    fn deref(&self) -> &Command {
        &self.command
    }
}
fn prepare(request: &GuestRequest) -> Result<PreparedGuest, Error> {
    if !request.timeout_seconds.is_finite() || !(0.001..=86400.0).contains(&request.timeout_seconds)
    {
        return Err(Error::Input(
            "guest agent time budget must be between 0.001 and 86400 seconds".into(),
        ));
    }
    if request.config.concurrency != 1 || request.prompt.len() > 64 * 1024 {
        return Err(Error::Input(
            "guest execution requires one bounded task invocation".into(),
        ));
    }
    let workspace = Path::new(&request.config.workspace);
    if ![
        workspace,
        &request.project_root,
        &request.runtime_dir,
        &request.run_dir,
    ]
    .into_iter()
    .all(absolute)
        || request.runtime_dir.starts_with(workspace)
        || request.run_dir.starts_with(workspace)
        || !request.run_dir.starts_with(&request.runtime_dir)
    {
        return Err(Error::Input("guest workspace and private runtime paths must be absolute, separate and owned by this attempt".into()));
    }
    if !request.project_root.join(".asb/project.toml").is_file() {
        return Err(Error::Input(
            "guest tool project is not initialized; install the selected agent before execution"
                .into(),
        ));
    }
    let adapter = super::find(&request.config.agent).ok_or_else(|| {
        Error::Input("guest execution requires a registered agent adapter".into())
    })?;
    if !adapter.supported_endpoints().iter().any(|support| {
        support.interface == request.connection.interface
            && support
                .authentication
                .contains(&request.connection.authentication)
    }) {
        return Err(Error::Input(
            "negotiated endpoint is not supported by the selected agent".into(),
        ));
    }
    let (_, project) = project::load(&request.project_root)?;
    let program = super::resolve(
        &request.config.agent,
        &request.project_root,
        &project.tools_dir,
    )
    .ok_or_else(|| {
        Error::Run("selected agent is not installed in the guest tool project".into())
    })?;
    let specification = adapter.command(&super::CommandContext {
        config: &request.config,
        endpoint: Some(&request.connection.base_url),
        interface: Some(request.connection.interface),
        prompt: &request.prompt,
    });
    let mut command = adapter.process(&program)?.direct();
    command.args(specification.args);
    if specification.append_prompt {
        command.arg(&request.prompt);
    }
    command
        .current_dir(workspace)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    isolate_parent_state(&mut command);
    if request.replay {
        super::mask_replay_credentials(&mut command);
    }
    project::apply_network_proxy(&mut command, &request.project_root)?;
    // Do not replace a benchmark's Python PATH with the agent's interpreter.
    // Agent entry points/wrappers use their own absolute venv interpreter.
    crate::tools::apply_runtime_path(&mut command, &request.project_root, &project.tools_dir)?;
    let artifact_owner = super::ArtifactOwner::prepare(&request.run_dir, adapter.artifact_specs())?;
    adapter.prepare(&mut super::PreparationContext {
        command: &mut command,
        model: &request.config.model,
        connection: &request.connection,
        runtime_dir: &request.runtime_dir,
        run_dir: &request.run_dir,
    })?;
    Ok(PreparedGuest {
        command,
        artifact_owner,
    })
}

fn run(request: GuestRequest) -> Result<u8, Error> {
    let retained =
        request.process_lifetime == crate::workloads::session::ProcessLifetime::ThroughEvaluation;
    if retained {
        let owner = request
            .workload_owner
            .as_ref()
            .ok_or_else(|| Error::Input("workload lifetime has no namespace owner".into()))?;
        if owner.controller_namespace <= 1
            || owner.guest_namespace != namespace_id()?
            || owner.guest_namespace == owner.controller_namespace
            || owner.token.len() != 32
            || !owner.token.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(Error::Input(
                "workload lifetime is not bound to a distinct owned PID namespace".into(),
            ));
        }
    } else if request.workload_owner.is_some() {
        return Err(Error::Input(
            "quiescent guest execution cannot declare retained ownership".into(),
        ));
    }
    let PreparedGuest {
        mut command,
        mut artifact_owner,
    } = prepare(&request)?;
    fs::create_dir_all(&request.run_dir)
        .map_err(|error| Error::Io(format!("cannot create private guest artifacts: {error}")))?;
    let mut capture = if request.namespace_collectors.is_empty() {
        None
    } else {
        let mut session = crate::measurements::Session::start_in_execution_namespace(
            &request.run_dir.join("collector-capture"),
            &request.namespace_collectors,
        )
        .map_err(Error::Run)?;
        session.agent_launch_prepared(&crate::launch::TargetLaunch::direct(&command));
        session.attach_pid(std::process::id()).map_err(Error::Run)?;
        Some(session)
    };
    // Preparation and command construction are complete before the outer
    // collector attaches. Credentials remain only in process memory/env.
    println!("__ASB_GATE__:{}", std::process::id());
    std::io::stdout().flush().map_err(crate::io_error)?;
    #[cfg(unix)]
    crate::process_gate::pause_current()
        .map_err(|error| Error::Run(format!("cannot stop prepared guest agent: {error}")))?;
    #[cfg(not(unix))]
    return Err(Error::Run(
        "guest agent gate currently requires Unix signals".into(),
    ));
    #[cfg(target_os = "linux")]
    let held = if retained {
        Some(crate::runner::workload_process::start(
            &mut command,
            &mut std::io::stdout(),
            &mut std::io::stderr(),
            std::time::Duration::from_secs_f64(request.timeout_seconds),
            |status, elapsed, timed_out| {
                let owner = request
                    .workload_owner
                    .as_ref()
                    .expect("retained owner validated");
                println!(
                    "\n__ASB_AGENT_ENDED__:{}:{}:{}:{}:{}",
                    std::process::id(),
                    owner.token,
                    status.code().unwrap_or(1),
                    elapsed,
                    u8::from(timed_out)
                );
                std::io::stdout().flush().map_err(crate::io_error)
            },
        )?)
    } else {
        None
    };
    let output = if retained {
        #[cfg(target_os = "linux")]
        {
            held.as_ref().expect("retained process").output.clone()
        }
        #[cfg(not(target_os = "linux"))]
        return Err(Error::Input(
            "retained workload processes require Linux".into(),
        ));
    } else {
        crate::runner::run_guest_process(
            &mut command,
            &mut std::io::stdout(),
            &mut std::io::stderr(),
            std::time::Duration::from_secs_f64(request.timeout_seconds),
        )?
    };
    #[cfg(target_os = "linux")]
    if held.as_ref().is_some_and(|held| held.truncated) {
        return Err(Error::Run("retained client output exceeded its capture limit; grading must preserve the incomplete attempt".into()));
    }
    let namespace_collectors = capture
        .as_mut()
        .map(|session| session.export_execution_namespace_state())
        .transpose()
        .map_err(Error::Run)?
        .unwrap_or_default();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let adapter = super::find(&request.config.agent).expect("validated agent");
    let frozen = artifact_owner.freeze();
    let artifacts = adapter.collect(&super::CollectionContext {
        frozen: &frozen,
        run_dir: &request.run_dir,
        interface: Some(request.connection.interface),
        anthropic_input_includes_cache: crate::providers::anthropic_input_includes_cache(
            &request.config.provider,
        ),
    });
    let usage = crate::measurements::token_usage_from_sources(
        &request.config.provider,
        &request.config.agent,
        &stdout,
        &stderr,
        None,
    );
    let usage = crate::measurements::with_artifact_usage(usage, artifacts.token_usage);
    let artifact_error = artifact_owner.settle().err().or_else(|| {
        frozen.error.then(|| {
            "agent artifact evidence unavailable; retained scratch requires inspection".into()
        })
    });
    let result = GuestResult {
        artifact_error,
        #[cfg(target_os = "linux")]
        elapsed_ms: held.as_ref().map(|held| held.elapsed_ms),
        #[cfg(not(target_os = "linux"))]
        elapsed_ms: None,
        #[cfg(target_os = "linux")]
        timed_out: held.as_ref().is_some_and(|held| held.timed_out),
        #[cfg(not(target_os = "linux"))]
        timed_out: false,
        workload_owner: request.workload_owner,
        schema_version: 1,
        exit_code: output.status.code(),
        final_response: artifacts.final_response.or_else(|| {
            super::final_response(
                &request.config.agent,
                &stdout,
                Path::new(&request.config.workspace),
            )
        }),
        token_usage: usage,
        steps: artifacts
            .steps
            .or_else(|| adapter.step_count(&stdout, &stderr)),
        execution_failure: {
            #[cfg(target_os = "linux")]
            let timed_out = held.as_ref().is_some_and(|held| held.timed_out);
            #[cfg(not(target_os = "linux"))]
            let timed_out = false;
            if timed_out {
                Some("agent execution exceeded its time budget".into())
            } else {
                adapter.execution_failure(&stdout, &stderr)
            }
        },
        tree_quiesced: cfg!(target_os = "linux") && !retained,
        namespace_collectors,
    };
    let destination = request.run_dir.join("agent-result.json");
    let temporary = request
        .run_dir
        .join(format!(".agent-result-{}", std::process::id()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|error| Error::Io(error.to_string()))?;
    serde_json::to_writer(&mut file, &result)
        .map_err(|_| Error::Io("cannot encode guest result".into()))?;
    file.sync_all()
        .and_then(|()| fs::rename(&temporary, &destination))
        .map_err(|error| Error::Io(error.to_string()))?;
    #[cfg(target_os = "linux")]
    if let Some(held) = held {
        let owner = result
            .workload_owner
            .as_ref()
            .expect("retained namespace owner");
        println!(
            "\n__ASB_WORKLOAD_READY__:{}:{}:{}",
            std::process::id(),
            owner.token,
            output.status.code().unwrap_or(1)
        );
        std::io::stdout().flush().map_err(crate::io_error)?;
        held.hold_for_evaluation();
    }
    Ok(output
        .status
        .code()
        .and_then(|code| u8::try_from(code).ok())
        .unwrap_or(1))
}

pub(crate) fn entry() -> std::process::ExitCode {
    let result = (|| {
        let text = std::env::var(REQUEST)
            .map_err(|_| Error::Input("private guest agent request is missing".into()))?;
        if text.len() > 1024 * 1024 {
            return Err(Error::Input("guest request exceeds 1 MiB".into()));
        }
        let request = serde_json::from_str(&text)
            .map_err(|_| Error::Input("invalid guest execution request JSON".into()))?;
        run(request)
    })();
    match result {
        Ok(code) => std::process::ExitCode::from(code),
        Err(error) => {
            eprintln!("ASB guest agent could not execute: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
