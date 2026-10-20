//! Common execution/evaluation protocol for workload-owned official engines.
use crate::workloads::controller::{Controller, decode_base64};
use crate::workloads::session::{
    BaselineEvidence, ExecutionContext, ExecutionResult, PreparationContext, TaskInvocation,
};
use crate::workloads::{Evaluation, TaskIdentity, WorkloadSession};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

pub(crate) fn prefetch(context: &super::PreflightContext<'_>) -> Result<(), crate::Error> {
    let tools = crate::project::resolve(context.project_root, &context.project_config.tools_dir);
    crate::install::guest_bootstrap(context.agent, context.project_root, &tools)?;
    Ok(())
}

pub(crate) fn runtime_inputs(
    tool: &str,
    context: &crate::runtime_inputs::Context<'_>,
) -> Result<Vec<crate::runtime_inputs::Dependency>, crate::Error> {
    use crate::runtime_inputs::Dependency;
    use std::path::Path;
    // This controller owns the Python SDK and Docker CLI transport, not
    // benchmark dispatch or the selected agent/provider implementation.
    let mut inputs = vec![
        Dependency::Python {
            path: context.tool_root(tool).join("venv/bin/python"),
            site: true,
        },
        Dependency::program("docker"),
    ];
    if crate::tools::host_executable(Path::new("sudo")).is_some() {
        inputs.push(Dependency::program("sudo"));
    }
    if let Some(home) = std::env::var_os("HOME") {
        for path in [
            ".docker/config.json",
            ".docker/contexts",
            ".docker/cli-plugins",
        ] {
            inputs.push(Dependency::file(Path::new(&home).join(path)));
        }
    }
    for path in [
        "/usr/libexec/docker/cli-plugins",
        "/usr/local/lib/docker/cli-plugins",
        "/usr/lib/docker/cli-plugins",
    ] {
        inputs.push(Dependency::file(path));
    }
    Ok(inputs)
}

struct Session {
    controller: Controller,
    invocation: TaskInvocation,
    baseline: BaselineEvidence,
    guest_workspace: String,
    agent_timeout: Duration,
    measurement_active: bool,
    preparation_active: bool,
    cleanup_attempted: bool,
    artifacts: PathBuf,
}

pub(crate) fn require_sdk(root: &std::path::Path, workload: &str) -> Result<(), crate::Error> {
    if !root.join("venv/bin/python").is_file() {
        return Err(crate::Error::Input(format!(
            "the official evaluator for `{workload}` is not installed; run `asb workload install {workload}` before starting the benchmark"
        )));
    }
    Ok(())
}

pub(crate) fn launch(
    tool_id: &str,
    helper: &str,
    operation: Option<&str>,
    workload: &str,
    dataset: super::session::DatasetIdentity,
    context: &PreparationContext<'_>,
) -> Result<Box<dyn WorkloadSession>, String> {
    let task = context
        .trial
        .ok_or("select an official instance ID in setup before running this benchmark")?;
    if context.trial_set.is_some() {
        crate::workloads::selected_trial(
            workload,
            task,
            context.trial_set,
            context.workload_directory,
        )?;
    }
    let project_root = context.project_root;
    let tool = context.tool_root(tool_id);
    let attempts = project_root.join(".asb/attempts");
    std::fs::create_dir_all(&attempts).map_err(|error| error.to_string())?;
    let artifacts = attempts.join(TaskIdentity::local(workload, Some(task)).attempt_id);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let error_log = options
        .open(artifacts.with_extension("controller.log"))
        .map_err(|error| error.to_string())?;
    let mut command = Command::new(tool.join("venv/bin/python"));
    command.stderr(error_log);
    command.arg(tool.join(helper));
    if let Some(operation) = operation {
        command.arg(operation);
    }
    command
        .arg("--catalog")
        .arg(
            context
                .workload_directory
                .join(workload)
                .join("catalog.json"),
        )
        .arg("--task")
        .arg(task)
        .arg("--artifacts")
        .arg(&artifacts)
        .env("PYTHONDONTWRITEBYTECODE", "1");
    crate::project::apply_network_proxy(&mut command, project_root)
        .map_err(|error| error.to_string())?;
    let mut controller = Controller::spawn(&mut command)?;
    let prepared = (|| -> Result<_, String> {
        let ready = controller.receive(Instant::now() + Duration::from_secs(900))?;
        if ready["event"] != "ready" {
            return Err("official task did not become ready".into());
        }
        let invocation = &ready["invocation"];
        let identity: TaskIdentity = serde_json::from_value(invocation["identity"].clone())
            .map_err(|_| "official task returned invalid identity")?;
        if identity.workload != workload
            || identity.task_id != task
            || identity.dataset.as_ref() != Some(&dataset)
        {
            return Err("prepared task does not match the selected dataset instance".into());
        }
        let prompt = invocation["prompt"]
            .as_str()
            .filter(|text| !text.is_empty() && text.len() <= 64 * 1024)
            .ok_or("official task returned invalid public instructions")?
            .to_owned();
        let guest_workspace = invocation["workspace"]
            .as_str()
            .filter(|path| path.starts_with('/') && !path.contains(".."))
            .ok_or("official task returned invalid environment workspace")?
            .to_owned();
        let baseline: BaselineEvidence = serde_json::from_value(invocation["baseline"].clone())
            .map_err(|_| "official baseline evidence is invalid")?;
        if !baseline.supports("upstream-harness") {
            return Err("official task definition does not establish baseline identity".into());
        }
        let agent_timeout = invocation
            .get("agent_timeout_seconds")
            .and_then(Value::as_f64)
            .unwrap_or(900.0);
        if !agent_timeout.is_finite() || !(0.001..=86400.0).contains(&agent_timeout) {
            return Err("official task returned an invalid agent time budget".into());
        }
        Ok((identity, prompt, guest_workspace, baseline, agent_timeout))
    })();
    let (identity, prompt, guest_workspace, baseline, agent_timeout) = match prepared {
        Ok(prepared) => prepared,
        Err(error) => {
            return Err(match controller.cancel(Duration::from_secs(240)) {
                Ok(()) => error,
                Err(cleanup) => format!("{error}; workload cancellation cleanup failed: {cleanup}"),
            });
        }
    };
    Ok(Box::new(Session {
        controller,
        invocation: TaskInvocation {
            identity,
            prompt,
            workspace: context.workspace.to_path_buf(),
        },
        baseline,
        guest_workspace,
        agent_timeout: Duration::from_secs_f64(agent_timeout),
        measurement_active: false,
        preparation_active: false,
        cleanup_attempted: false,
        artifacts,
    }))
}

impl Session {
    fn cancel(&mut self) -> Result<(), String> {
        self.cleanup_attempted = true;
        self.controller.cancel(Duration::from_secs(240))
    }

    fn run(
        &mut self,
        context: &mut ExecutionContext<'_>,
        output: &mut dyn std::io::Write,
    ) -> Result<ExecutionResult, String> {
        let connection = context
            .connection
            .ok_or("isolated execution needs a negotiated API connection")?;
        if let Some(session) = context.measurements.as_deref_mut() {
            session.event(crate::collectors::LifecycleEvent::PhaseBegin(
                crate::collectors::LifecyclePhase::AgentPreparation,
            ))?;
            self.preparation_active = true;
        }
        let binary = runtime_binary()?;
        let tools_root =
            crate::project::resolve(context.project_root, &context.project_config.tools_dir);
        let bootstrap = crate::install::guest_bootstrap(
            &context.config.agent,
            context.project_root,
            &tools_root,
        )
        .map_err(|error| {
            format!("cannot prepare selected agent installation artifacts: {error}")
        })?;
        let mut config = context.config.clone();
        config.workspace = self.guest_workspace.clone();
        config.concurrency = 1;
        let mut project = context.project_config.clone();
        project.tools_dir = ".asb-tools".into();
        project.workloads_dir = "workloads".into();
        project.workspace_dir = "workspace".into();
        let mut environment = serde_json::Map::new();
        let namespace_collectors = context
            .measurements
            .as_deref_mut()
            .map(|session| session.execution_namespace_collectors())
            .unwrap_or_default();
        for name in [
            "http_proxy",
            "HTTP_PROXY",
            "https_proxy",
            "HTTPS_PROXY",
            "all_proxy",
            "ALL_PROXY",
            "socks_proxy",
            "SOCKS_PROXY",
            "no_proxy",
            "NO_PROXY",
            "NPM_CONFIG_OFFLINE",
            "ASB_MEASURE_INTERVAL_MS",
        ] {
            if let Ok(value) = std::env::var(name) {
                environment.insert(name.into(), Value::String(value));
            }
        }
        self.controller.send(&json!({"op":"agent", "binary":binary, "project_root":context.project_root,
            "tools_root":tools_root, "bootstrap":bootstrap, "measure_target":context.measurements.is_some(),
            "project_configuration":toml::to_string(&project).map_err(|_| "cannot encode guest project configuration")?,
            "environment":environment, "network":std::env::var("ASB_BENCHMARK_NETWORK").ok(),
            "request":{"config":config,"connection":connection,"prompt":self.invocation.prompt,
                "project_root":"/opt/asb-project","runtime_dir":"/opt/asb-private","run_dir":"/opt/asb-private/attempt","replay":context.replay,
                "namespace_collectors":namespace_collectors}}))?;
        let deadline = Instant::now() + self.agent_timeout + Duration::from_secs(1820);
        loop {
            let event = self.controller.receive(deadline)?;
            match event["event"].as_str() {
                Some("paused") => {
                    if let Some(session) = context.measurements.as_deref_mut() {
                        use crate::collectors::LifecycleEvent as Event;
                        if self.measurement_active {
                            return Err(
                                "isolated execution returned a second measurement target".into()
                            );
                        }
                        let target = super::target::verify(&event["target"])?;
                        crate::measurements::write_json(
                            &session.dir.join("execution-target.json"),
                            &json!({
                                "schema_version":1,"scope":"verified-local-isolated-process-tree",
                                "host_pid":target.pid,"namespace_pid":target.namespace_pid,
                                "start_time_ticks":target.start_time_ticks,"cgroup":target.cgroup,
                                "cgroup_membership":"preserved"
                            }),
                        )?;
                        if let Err(error) =
                            session.attach_isolated_pid(target.pid, target.start_time_ticks)
                        {
                            session.event(Event::AgentAttachFailed)?;
                            return Err(error);
                        }
                        session.event(Event::PhaseEnd(
                            crate::collectors::LifecyclePhase::AgentPreparation,
                        ))?;
                        self.preparation_active = false;
                        session.event(Event::MeasurementsReady)?;
                        session.event(Event::AgentSpawn)?;
                        session.event(Event::AgentExecuteBegin)?;
                        self.measurement_active = true;
                        session.event(Event::AgentStarted)?;
                    }
                    self.controller
                        .send(&json!({"op":"resume","exec_id":event["target"]["exec_id"]}))?;
                }
                Some("output") => {
                    let bytes = decode_base64(&event["data"])?;
                    if context.show_output {
                        output
                            .write_all(&bytes)
                            .map_err(|_| "cannot write agent output")?;
                    }
                }
                Some("execution_ended") => {
                    if let Some(session) = context.measurements.as_deref_mut() {
                        if !self.measurement_active {
                            return Err(
                                "isolated execution ended without an active measured target".into(),
                            );
                        }
                        session.event(crate::collectors::LifecycleEvent::AgentExecuteEnd)?;
                        session.event(crate::collectors::LifecycleEvent::AgentExited)?;
                        self.measurement_active = false;
                    }
                }
                Some("completed") => {
                    if self.measurement_active {
                        return Err("isolated execution omitted its completion boundary".into());
                    }
                    let result = &event["result"];
                    let metadata: crate::agents::driver::GuestResult =
                        serde_json::from_value(result["metadata"].clone())
                            .map_err(|_| "guest returned invalid agent result metadata")?;
                    if !metadata.tree_quiesced {
                        return Err("isolated agent process tree was not confirmed stopped before evaluation".into());
                    }
                    if let Some(session) = context.measurements.as_deref_mut() {
                        session.import_execution_namespace_state(&metadata.namespace_collectors)?;
                    }
                    let code = result["exit_code"].as_i64().unwrap_or(1).clamp(0, 255) as i32;
                    #[cfg(unix)]
                    let status = {
                        use std::os::unix::process::ExitStatusExt;
                        std::process::ExitStatus::from_raw(code << 8)
                    };
                    #[cfg(not(unix))]
                    return Err("isolated execution requires Unix".into());
                    return Ok(ExecutionResult {
                        output: std::process::Output {
                            status,
                            stdout: decode_base64(&result["stdout"])?,
                            stderr: decode_base64(&result["stderr"])?,
                        },
                        metadata,
                        elapsed_ms: result["elapsed_seconds"]
                            .as_f64()
                            .filter(|seconds| {
                                seconds.is_finite() && (0.0..=86410.0).contains(seconds)
                            })
                            .map(|seconds| (seconds * 1000.0).round() as u128),
                    });
                }
                _ => return Err("unexpected isolated execution event".into()),
            }
        }
    }
}

fn runtime_binary() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("ASB_GUEST_BINARY") {
        return Ok(PathBuf::from(path));
    }
    // Pin the actual running executable, even if an update atomically replaces
    // its pathname while the SDK prepares the task. Numeric PID is essential:
    // /proc/self/exe would refer to the Python controller when it reads it.
    #[cfg(target_os = "linux")]
    return Ok(PathBuf::from(format!("/proc/{}/exe", std::process::id())));
    #[cfg(not(target_os = "linux"))]
    std::env::current_exe().map_err(|_| "cannot find portable ASB runtime".into())
}

impl WorkloadSession for Session {
    fn checkpoint_evidence(&self) -> Vec<PathBuf> {
        vec![self.artifacts.clone()]
    }
    fn finish(&mut self) -> Result<(), String> {
        self.cleanup_attempted = true;
        match self.controller.finish(Duration::from_secs(240)) {
            Ok(()) => Ok(()),
            Err(error) => Err(match self.cancel() {
                Ok(()) => error,
                Err(cleanup) => format!("{error}; workload cancellation cleanup failed: {cleanup}"),
            }),
        }
    }
    fn invocation(&self) -> &TaskInvocation {
        &self.invocation
    }
    fn evaluation_kind(&self) -> &'static str {
        "upstream-harness"
    }
    fn baseline_passed(&self) -> Option<bool> {
        None
    }
    fn baseline_evidence(&self) -> BaselineEvidence {
        self.baseline.clone()
    }
    fn execute(
        &mut self,
        context: &mut ExecutionContext<'_>,
        output: &mut dyn std::io::Write,
    ) -> Option<Result<ExecutionResult, String>> {
        let mut result = self.run(context, output);
        if result.is_err() && self.preparation_active {
            self.preparation_active = false;
            if let Some(session) = context.measurements.as_deref_mut() {
                let _ = session.event(crate::collectors::LifecycleEvent::PhaseFailed(
                    crate::collectors::LifecyclePhase::AgentPreparation,
                ));
            }
        }
        if result.is_err() && self.measurement_active {
            self.measurement_active = false;
            if let Some(session) = context.measurements.as_deref_mut() {
                let _ = session.event(crate::collectors::LifecycleEvent::AgentExecuteEnd);
                let _ = session.event(crate::collectors::LifecycleEvent::AgentWaitFailed);
            }
        }
        if let Err(error) = &mut result
            && let Err(cleanup) = self.cancel()
        {
            error.push_str(&format!(
                "; workload cancellation cleanup failed: {cleanup}"
            ));
        }
        Some(result)
    }
    fn evaluate(&mut self, _response: &str) -> Result<Evaluation, String> {
        let event = self.controller.request(
            json!({"op":"evaluate","timeout":1800}),
            "evaluated",
            Duration::from_secs(1830),
        )?;
        let result = &event["result"];
        if result["workload"] != self.invocation.identity.workload
            || result["task_id"] != self.invocation.identity.task_id
            || result["dataset"]
                != serde_json::to_value(&self.invocation.identity.dataset)
                    .map_err(|_| "cannot compare evaluation dataset identity")?
        {
            return Err("official evaluation does not match this dataset instance".into());
        }
        match result["outcome"].as_str() {
            Some("resolved") => Ok(Evaluation::Passed),
            Some("unresolved" | "empty_submission") => Ok(Evaluation::Failed("official evaluator did not resolve the selected instance".into())),
            _ => Err("official evaluator did not return a completed task-specific grade; inspect task artifacts".into()),
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Preparation can succeed before provider/proxy setup fails. Those
        // callers never reach execute/finish; still give the owner its full
        // bounded teardown budget instead of the controller's kill fallback.
        // Explicit paths report failures; Drop cannot replace a caller error.
        if !self.cleanup_attempted {
            let _ = self.cancel();
        }
    }
}
