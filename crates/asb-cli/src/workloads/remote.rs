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

struct Session {
    controller: Controller,
    invocation: TaskInvocation,
    baseline: BaselineEvidence,
    guest_workspace: String,
    agent_timeout: Duration,
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
    }))
}

impl Session {
    fn run(
        &mut self,
        context: &ExecutionContext<'_>,
        output: &mut dyn std::io::Write,
    ) -> Result<ExecutionResult, String> {
        if context.measured {
            return Err("isolated benchmark collector targeting is still being integrated; disable measurements for this development execution".into());
        }
        let connection = context
            .connection
            .ok_or("isolated execution needs a negotiated API connection")?;
        let authority = connection
            .base_url
            .split("://")
            .nth(1)
            .unwrap_or("")
            .split('/')
            .next()
            .unwrap_or("");
        if authority.starts_with("127.")
            || authority.starts_with("localhost:")
            || authority.starts_with("[::1]")
        {
            return Err("the selected endpoint is host loopback; isolated endpoint forwarding is still being integrated".into());
        }
        let binary = std::env::var_os("ASB_GUEST_BINARY")
            .map(PathBuf::from)
            .or_else(|| std::env::current_exe().ok())
            .ok_or("cannot find portable ASB runtime")?;
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
        for name in [
            "http_proxy",
            "HTTP_PROXY",
            "https_proxy",
            "HTTPS_PROXY",
            "all_proxy",
            "ALL_PROXY",
            "no_proxy",
            "NO_PROXY",
            "NPM_CONFIG_OFFLINE",
        ] {
            if let Ok(value) = std::env::var(name) {
                environment.insert(name.into(), Value::String(value));
            }
        }
        self.controller.send(&json!({"op":"agent", "binary":binary, "project_root":context.project_root,
            "tools_root":tools_root, "bootstrap":bootstrap,
            "project_configuration":toml::to_string(&project).map_err(|_| "cannot encode guest project configuration")?,
            "environment":environment, "network":std::env::var("ASB_BENCHMARK_NETWORK").ok(),
            "request":{"config":config,"connection":connection,"prompt":self.invocation.prompt,
                "project_root":"/opt/asb-project","runtime_dir":"/opt/asb-private","run_dir":"/opt/asb-private/attempt","replay":context.replay}}))?;
        let deadline = Instant::now() + self.agent_timeout + Duration::from_secs(1820);
        loop {
            let event = self.controller.receive(deadline)?;
            match event["event"].as_str() {
                Some("paused") => self
                    .controller
                    .send(&json!({"op":"resume","exec_id":event["target"]["exec_id"]}))?,
                Some("output") => {
                    let bytes = decode_base64(&event["data"])?;
                    if context.show_output {
                        output
                            .write_all(&bytes)
                            .map_err(|_| "cannot write agent output")?;
                    }
                }
                Some("completed") => {
                    let result = &event["result"];
                    let metadata = serde_json::from_value(result["metadata"].clone())
                        .map_err(|_| "guest returned invalid agent result metadata")?;
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

impl WorkloadSession for Session {
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
        context: &ExecutionContext<'_>,
        output: &mut dyn std::io::Write,
    ) -> Option<Result<ExecutionResult, String>> {
        Some(self.run(context, output))
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
