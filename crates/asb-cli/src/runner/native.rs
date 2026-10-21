//! Native command preparation is complete before process gating or launch.
use super::*;

pub(super) struct NativeRoute<'a> {
    pub connection: Option<&'a crate::endpoint::Connection>,
    pub provider_endpoint: Option<&'a str>,
    pub provider_credentials: Option<(&'a str, &'a str)>,
    pub tape_url: Option<&'a str>,
    pub tape_mode: Option<tape::Mode>,
}

/// Immutable intent, not shared mutable runtime state.
pub(super) struct NativePlan<'a> {
    pub config_path: &'a Path,
    pub config: &'a BenchmarkConfig,
    pub project_root: &'a Path,
    pub project_config: &'a project::ProjectConfig,
    pub invocation: &'a workloads::session::TaskInvocation,
    /// Explicit repeated attempts isolate agent state before adapter setup.
    pub fresh_state: bool,
    pub route: NativeRoute<'a>,
}

pub(super) struct PreparedNative {
    command: Command,
    run_dir: PathBuf,
    runtime_path: PathBuf,
}

impl NativePlan<'_> {
    pub fn prepare(
        self,
        measurements: Option<&mut measurements::Session>,
    ) -> Result<PreparedNative, Error> {
        let Self {
            config_path,
            config,
            project_root,
            project_config,
            invocation,
            fresh_state,
            route,
        } = self;
        let NativeRoute {
            connection,
            provider_endpoint,
            provider_credentials,
            tape_url,
            tape_mode,
        } = route;
        let workspace = &invocation.workspace;
        let prompt = &invocation.prompt;
        // Adapters run with the benchmark workspace as their current directory.
        // Pass the canonical workspace explicitly so a relative configuration
        // value is not resolved a second time inside that directory (which is
        // especially important for OpenJiuwen's persistent workspace).
        let mut command_config = config.clone();
        command_config.workspace = workspace.to_string_lossy().into_owned();
        let specification = command_for_with_interface(
            &command_config,
            connection
                .as_ref()
                .map(|connection| connection.base_url.as_str())
                .or(provider_endpoint),
            prompt,
            connection.as_ref().map(|connection| connection.interface),
        )?;
        let mut program = specification.program;
        if let Some(local) = agents::resolve(&config.agent, project_root, &project_config.tools_dir)
        {
            program = local;
        }
        let mut command = agents::find(&config.agent).map_or_else(
            || Ok(Command::new(&program)),
            |adapter| adapter.process(&program),
        )?;
        project::apply_network_proxy(&mut command, config_path)?;
        command.args(specification.args);
        // Do not leak an enclosing Codex session into the benchmark agent. These
        // variables are host-runner integration state; if inherited, a project
        // local Codex CLI can advertise the coordinator's tools as an
        // `additional_tools` request that cli2key does not support.
        agents::driver::isolate_parent_state(&mut command);
        if specification.append_prompt {
            command.arg(prompt);
        }
        command
            .current_dir(workspace)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        providers::configure(
            &mut command,
            &config.provider,
            &config.model,
            tape_mode == Some(tape::Mode::Replay),
        );
        if tape_mode == Some(tape::Mode::Replay) {
            agents::mask_replay_credentials(&mut command);
        }
        providers::apply_with_tape(
            &mut command,
            &config.provider,
            &config.model,
            provider_credentials,
            tape_url,
            tape_mode == Some(tape::Mode::Replay),
        )?;
        // Agent state must not appear inside the workload checkout, where it can
        // affect git status, tools, and therefore the recorded agent trajectory.
        let runtime_path = replica::runtime_path(workspace);
        // Run-specific agent artifacts must not be confused with earlier runs or
        // simultaneous replicas, including after process IDs are reused.
        static RUN_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let run_dir = runtime_path.join("runs").join(format!(
            "{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            RUN_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        instrument_phase(measurements, LifecyclePhase::AgentPreparation, || {
            if fresh_state {
                agents::driver::prepare_fresh_state(&mut command, &run_dir.join("state"))?;
            }
            if let Some(connection) = connection.as_ref() {
                agents::prepare(
                    &config.agent,
                    &mut agents::PreparationContext {
                        command: &mut command,
                        model: &config.model,
                        connection,
                        runtime_dir: &runtime_path,
                        run_dir: &run_dir,
                    },
                )?;
            }
            Ok(())
        })?;
        if let Some(url) = tape_url {
            providers::configure_tape(
                &mut command,
                &config.provider,
                url,
                tape_mode.expect("tape URL implies tape mode"),
            );
        }
        if tape_mode == Some(tape::Mode::Replay) {
            apply_replay_network_guard(&mut command);
        }
        project::apply_python_path(&mut command, project_root, Path::new(&program))?;
        crate::tools::apply_runtime_path(&mut command, project_root, &project_config.tools_dir)?;

        Ok(PreparedNative {
            command,
            run_dir,
            runtime_path,
        })
    }
}

impl PreparedNative {
    pub fn execute(
        mut self,
        measurements: Option<&mut measurements::Session>,
        show_output: bool,
        output: &mut dyn Write,
        launch: Option<&dyn workloads::session::LaunchReadiness>,
    ) -> Result<(ExecutionEvidence, AttemptIssues), Error> {
        // The process gate must execute the fully prepared command, not the
        // adapter's original executable. Some adapters (for example OpenJiuwen)
        // install a process wrapper that carries compatibility and output fixes;
        // bypassing it at the gate would silently undo those fixes while still
        // appearing to attach collectors correctly.
        let gate_program = self.command.get_program().to_string_lossy().into_owned();
        let gate_args = self
            .command
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let child = run_measured_agent_with_launch(
            &mut self.command,
            &gate_program,
            Some(&gate_args),
            measurements,
            show_output,
            output,
            launch,
        )?;
        Ok((
            ExecutionEvidence {
                output: child.output,
                run_dir: self.run_dir,
                guest: None,
                elapsed_ms: child.elapsed_ms,
                runtime_output: Some(self.runtime_path),
            },
            child.issues,
        ))
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn prepared_command_keeps_adapter_wrapper_route_and_replay_guard_before_gating() {
        for fresh_state in [false, true] {
            prepared_command(fresh_state);
        }
    }

    fn prepared_command(fresh_state: bool) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/native-plan-fixtures")
            .join(workloads::TaskIdentity::local("native", None).attempt_id);
        let tool = root.join("tools/openjiuwen/bin/openjiuwen");
        let interpreter = root.join("fixture/python3");
        std::fs::create_dir_all(tool.parent().unwrap()).unwrap();
        std::fs::create_dir_all(interpreter.parent().unwrap()).unwrap();
        std::fs::write(&interpreter, b"fixture interpreter; never executed").unwrap();
        std::fs::write(
            &tool,
            format!(
                "#!{}\nfixture client; never executed\n",
                interpreter.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o700)).unwrap();
        let config: BenchmarkConfig = toml::from_str(
            "agent='openjiuwen'\nprovider='openrouter'\nmodel='fixture'\nworkload='fixture'\n",
        )
        .unwrap();
        let project = project::ProjectConfig {
            tools_dir: "tools".into(),
            ..Default::default()
        };
        let invocation = workloads::session::TaskInvocation {
            identity: workloads::TaskIdentity::local("fixture", None),
            prompt: "private fixture prompt".into(),
            workspace: root.join("workspace"),
        };
        let connection = crate::endpoint::Connection {
            interface: crate::endpoint::Interface::AnthropicMessages,
            authentication: crate::endpoint::Authentication::ApiKey,
            base_url: "http://127.0.0.1:1/v1".into(),
            credential: Some("asb-replay".into()),
        };
        let prepared = NativePlan {
            config_path: &root.join("benchmark.toml"),
            config: &config,
            project_root: &root,
            project_config: &project,
            invocation: &invocation,
            fresh_state,
            route: NativeRoute {
                connection: Some(&connection),
                provider_endpoint: Some(&connection.base_url),
                provider_credentials: None,
                tape_url: Some("http://127.0.0.1:1"),
                tape_mode: Some(tape::Mode::Replay),
            },
        }
        .prepare(None)
        .unwrap();
        assert_eq!(prepared.command.get_program(), interpreter.as_os_str());
        let args: Vec<_> = prepared
            .command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args[0], "-c");
        assert!(
            args[1].contains("runpy"),
            "adapter compatibility wrapper must remain the gated target"
        );
        assert_eq!(Path::new(&args[2]), tool.canonicalize().unwrap());
        assert_eq!(
            prepared.command.get_current_dir(),
            Some(invocation.workspace.as_path())
        );
        let vars: std::collections::HashMap<_, _> = prepared.command.get_envs().collect();
        assert_eq!(
            vars[std::ffi::OsStr::new("HTTPS_PROXY")],
            Some(std::ffi::OsStr::new("http://127.0.0.1:9"))
        );
        assert_eq!(
            vars[std::ffi::OsStr::new("NO_PROXY")],
            Some(std::ffi::OsStr::new("127.0.0.1,localhost,::1"))
        );
        assert_eq!(vars[std::ffi::OsStr::new("CODEX_SESSION_ID")], None);
        let home = vars.get(std::ffi::OsStr::new("HOME"));
        if fresh_state {
            let expected = prepared.run_dir.join("state/home");
            assert_eq!(home, Some(&Some(expected.as_os_str())));
            assert!(expected.is_dir());
        } else {
            assert!(home.is_none(), "legacy runs inherit the user's home");
        }
        assert!(!prepared.run_dir.starts_with(&invocation.workspace));
        std::fs::remove_dir_all(root).unwrap();
    }
}
