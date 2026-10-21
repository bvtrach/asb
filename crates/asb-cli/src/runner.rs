//! Live execution of one configured benchmark.
mod batch;
mod completion;
mod repeated;
pub(crate) mod replica;
pub(crate) mod reuse;
#[cfg(target_os = "linux")]
pub(crate) mod workload_process;

use super::{Error, io_error, read_config};
use crate::agents;
use crate::benchmark::BenchmarkConfig;
use crate::collectors::{LifecycleEvent, LifecyclePhase};
use crate::measurements;
use crate::models;
use crate::project;
use crate::providers;
use crate::suggestions;
use crate::tape;
use crate::workloads;
use serde::Serialize;
use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::Mutex;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;
use std::time::Instant;

const MAX_OUTPUT: usize = 1024 * 1024;
const MAX_AGENT_RUNTIME: Duration = Duration::from_secs(15 * 60);

fn emit_warning(
    warnings: &Mutex<BTreeSet<String>>,
    output: &mut dyn Write,
    message: String,
) -> Result<(), Error> {
    let mut seen = warnings
        .lock()
        .map_err(|_| Error::Run("warning state unavailable".into()))?;
    if !seen.contains(&message) {
        writeln!(output, "{message}").map_err(io_error)?;
        seen.insert(message);
    }
    Ok(())
}

#[derive(Serialize)]
struct RunResult<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    execution: Option<repeated::ExecutionIdentity>,
    replica_index: Option<usize>,
    execution_cohort: Option<&'a measurements::ExecutionCohort>,
    schema_version: u32,
    status: &'a str,
    agent: &'a str,
    workload: &'a str,
    task: workloads::TaskIdentity,
    provider: &'a str,
    model: &'a str,
    process_success: bool,
    workload_success: Option<bool>,
    workload_evaluation: &'static str,
    workload_baseline_passed: Option<bool>,
    workload_baseline: workloads::BaselineEvidence,
    workload_failure: Option<workloads::EvaluationFailure>,
    failure_kind: Option<&'static str>,
    failure_reason: Option<String>,
    success_rate: Option<f64>,
    adapter_smoke_success: Option<bool>,
    elapsed_ms: u128,
    exit_code: Option<i32>,
    token_usage: measurements::TokenUsage,
    steps: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cassette: Option<String>,
    /// Actual capture directory, including a CLI --measurements override.
    #[serde(skip_serializing_if = "Option::is_none")]
    measurement_directory: Option<PathBuf>,
    stdout: String,
    stderr: String,
}

/// Execution choices shared by single runs, replicas, and sweeps.
#[derive(Clone, Copy, Default)]
pub(crate) struct RunOptions<'a> {
    pub repeat: Option<u64>,
    pub batch: Option<&'a repeated::BatchIdentity>,
    pub verify_initial_material: bool,
    pub preparation_gate: Option<&'a batch::Gate>,
    pub execution_cohort: Option<&'a measurements::ExecutionCohort>,
    pub json: bool,
    pub show_output: bool,
    pub measurement_dir: Option<&'a str>,
    pub mode: Option<tape::Mode>,
    pub cassette: Option<&'a str>,
    pub respect_record_timings: bool,
    pub replica: replica::Identity,
    pub on_completion: Option<&'a dyn replica::CompletionObserver>,
    pub reused: Option<&'a std::collections::BTreeMap<usize, serde_json::Value>>,
}

/// Execute one configured benchmark, optionally through a local llmtape proxy.
pub(crate) fn run_with_tape(
    path: &str,
    options: RunOptions<'_>,
    output: &mut dyn Write,
) -> Result<(), Error> {
    run_with_diagnostics(path, options, output, &Mutex::new(BTreeSet::new()))
}

pub(crate) fn run_with_diagnostics(
    path: &str,
    options: RunOptions<'_>,
    output: &mut dyn Write,
    warnings: &Mutex<BTreeSet<String>>,
) -> Result<(), Error> {
    let config_path = Path::new(path);
    let text = read_config(config_path)?;
    let config: BenchmarkConfig = toml::from_str(&text)
        .map_err(|error| Error::Input(format!("cannot parse {path}: {error}")))?;
    if let Some(repeat) = options.repeat {
        return repeated::run(config_path, config, repeat, options, output, warnings);
    }
    run_loaded(config_path, config, options, output, warnings)
}

fn run_loaded(
    config_path: &Path,
    config: BenchmarkConfig,
    options: RunOptions<'_>,
    output: &mut dyn Write,
    warnings: &Mutex<BTreeSet<String>>,
) -> Result<(), Error> {
    if !options.replica.valid() {
        return Err(Error::Input("invalid benchmark replica identity".into()));
    }
    if let Some(reused) = options.reused {
        if !reused.is_empty()
            && (options.verify_initial_material || options.preparation_gate.is_some())
        {
            return Err(Error::Input(
                "fresh preparation cannot reuse historical replica slots".into(),
            ));
        }
        if reused
            .keys()
            .any(|index| !(1..=config.concurrency).contains(index))
        {
            return Err(Error::Input(
                "reused replica slots are outside the selected population".into(),
            ));
        }
        if config.concurrency == 1
            && let Some(summary) = reused.get(&1)
        {
            return reuse::render_reused(std::slice::from_ref(summary), options.json, output);
        }
    }
    let cohort = measurements::ExecutionCohort {
        id: workloads::TaskIdentity::local("execution-cohort", None).attempt_id,
        selected_replicas: config.concurrency,
        admitted_slots: (1..=config.concurrency)
            .filter(|slot| !options.reused.is_some_and(|saved| saved.contains_key(slot)))
            .collect(),
    };
    let options = RunOptions {
        execution_cohort: Some(&cohort),
        ..options
    };
    if let Some(measures) = config.measurements.as_ref().filter(|value| value.enabled) {
        // Parallel sweep configurations can share project-local tool paths.
        // Serialize dependency publication, not benchmark execution.
        static TOOL_PREPARATION: Mutex<()> = Mutex::new(());
        let _preparation = TOOL_PREPARATION
            .lock()
            .map_err(|_| Error::Run("shared tool preparation failed; retry the sweep".into()))?;
        let (root, project_config) = project::load(config_path)?;
        let tools_root = project::resolve(&root, &project_config.tools_dir);
        let mut messages = Vec::new();
        crate::install::ensure_collector_dependencies(
            &measures.collectors,
            &root,
            &tools_root,
            &mut messages,
            &mut |_, _| {},
            false,
        )?;
        if !options.json {
            output.write_all(&messages).map_err(io_error)?;
        }
    }
    let gate = (options.verify_initial_material && options.preparation_gate.is_none())
        .then(|| batch::Gate::new(config.concurrency, None, Duration::from_secs(1800)))
        .transpose()?;
    let options = RunOptions {
        preparation_gate: options.preparation_gate.or(gate.as_ref()),
        ..options
    };
    if config.concurrency > 1 {
        return run_concurrent(config_path, &config, options, output, warnings);
    }
    run_mode(config_path, config, options, output, warnings)
}

/// Execute identical replicas concurrently. Each replica gets an isolated
/// workspace, log, measurement directory, and record cassette so agents never
/// mutate one another's files. Output is buffered per replica and emitted in
/// replica order after completion to keep the terminal readable.
fn run_concurrent(
    config_path: &Path,
    config: &BenchmarkConfig,
    options: RunOptions<'_>,
    output: &mut dyn Write,
    warnings: &Mutex<BTreeSet<String>>,
) -> Result<(), Error> {
    let tape_request = options.mode.map(|mode| (mode, options.cassette));
    let mut jobs = Vec::with_capacity(config.concurrency);
    let mut completed = Vec::with_capacity(config.concurrency);
    for index in 1..=config.concurrency {
        if let Some(summary) = options.reused.and_then(|reused| reused.get(&index)) {
            let mut buffer = Vec::new();
            let result =
                reuse::render_reused(std::slice::from_ref(summary), options.json, &mut buffer);
            completed.push((index - 1, result, buffer));
            continue;
        }
        let replica = replica::configuration(
            config,
            replica::Identity {
                index,
                total: config.concurrency,
            },
        )?;
        let cassette = tape_request.map(|(mode, requested)| {
            let base = requested
                .map(PathBuf::from)
                .unwrap_or_else(|| default_cassette(config_path, config));
            (mode, replica_cassette(mode, &base, index))
        });
        let measurement = options
            .measurement_dir
            .map(|path| PathBuf::from(parallel_path(path, index)));
        jobs.push((index - 1, replica, measurement, cassette));
    }

    std::thread::scope(|scope| {
        let handles: Vec<_> = jobs
            .into_iter()
            .map(|(index, replica, measurement, cassette)| {
                scope.spawn(move || {
                    let mut buffer = Vec::new();
                    let mode = cassette.as_ref().map(|(mode, _)| *mode);
                    let cassette_path = cassette.as_ref().map(|(_, path)| path.to_string_lossy());
                    let cassette_path = cassette_path.as_deref();
                    let measurement_path = measurement.as_deref().and_then(Path::to_str);
                    let result = run_mode(
                        config_path,
                        replica,
                        RunOptions {
                            replica: replica::Identity {
                                index: index + 1,
                                total: config.concurrency,
                            },
                            measurement_dir: measurement_path,
                            mode,
                            cassette: cassette_path,
                            ..options
                        },
                        &mut buffer,
                        warnings,
                    );
                    (index, result, buffer)
                })
            })
            .collect();
        for handle in handles {
            completed.push(
                handle
                    .join()
                    .expect("benchmark concurrency worker panicked"),
            );
        }
    });

    completed.sort_by_key(|(index, _, _)| *index);
    let mut failures: Vec<(String, Vec<usize>)> = Vec::new();
    for (index, result, buffer) in completed {
        output.write_all(&buffer).map_err(io_error)?;
        if let Err(error) = result {
            add_replica_failure(&mut failures, index + 1, error.to_string());
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(Error::Run(format!(
            "{} concurrent benchmark replica(s) failed: {}",
            failures
                .iter()
                .map(|(_, indexes)| indexes.len())
                .sum::<usize>(),
            failures
                .iter()
                .map(|(message, indexes)| format!(
                    "replica{} {}: {message}",
                    if indexes.len() == 1 { "" } else { "s" },
                    indexes
                        .iter()
                        .map(usize::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
                .collect::<Vec<_>>()
                .join("; ")
        )))
    }
}

fn add_replica_failure(failures: &mut Vec<(String, Vec<usize>)>, index: usize, message: String) {
    if let Some((_, indexes)) = failures.iter_mut().find(|(item, _)| *item == message) {
        indexes.push(index);
    } else {
        failures.push((message, vec![index]));
    }
}

// A single recording can be replayed independently by every replica. When no
// single recording exists, retain compatibility with concurrent recordings.
fn replica_cassette(mode: tape::Mode, base: &Path, index: usize) -> PathBuf {
    if mode == tape::Mode::Replay && base.is_file() {
        base.to_path_buf()
    } else {
        let candidate = append_path_suffix(base, index);
        if mode != tape::Mode::Replay || candidate.is_file() {
            return candidate;
        }
        // Sweep replay may intentionally use a larger concurrency than the
        // recording. Reuse an existing replica recording rather than waiting
        // forever for a cassette that can never be produced. Keep the
        // requested path as the fail-closed fallback when no recording exists.
        (1..=crate::benchmark::MAX_CONCURRENCY)
            .map(|recorded| append_path_suffix(base, recorded))
            .find(|path| path.is_file())
            .unwrap_or(candidate)
    }
}

fn parallel_path(value: &str, index: usize) -> String {
    append_path_suffix(Path::new(value), index)
        .to_string_lossy()
        .into_owned()
}

fn append_path_suffix(path: &Path, index: usize) -> PathBuf {
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("benchmark");
    let extension = path.extension().and_then(|value| value.to_str());
    let name = match extension {
        Some(extension) => format!("{stem}-concurrency-{index}.{extension}"),
        None => format!("{stem}-concurrency-{index}"),
    };
    if let Some(parent) = path.parent() {
        parent.join(name)
    } else {
        PathBuf::from(name)
    }
}

fn run_mode(
    config_path: &Path,
    config: BenchmarkConfig,
    options: RunOptions<'_>,
    output: &mut dyn Write,
    warnings: &Mutex<BTreeSet<String>>,
) -> Result<(), Error> {
    // Register before any fallible setup. Early return or unwinding aborts peers.
    let mut arrival = options
        .preparation_gate
        .map(|gate| gate.register(options.replica))
        .transpose()?;
    let RunOptions {
        json,
        show_output,
        measurement_dir,
        mode,
        cassette,
        respect_record_timings,
        ..
    } = options;
    if options
        .execution_cohort
        .is_some_and(|cohort| !cohort.admits(options.replica.index, options.replica.total))
    {
        return Err(Error::Input(
            "dispatch cohort does not admit the selected replica".into(),
        ));
    }
    if let Some(observer) = options.on_completion {
        observer.starting(options.replica, options.execution_cohort)?;
    }
    let tape_request = mode.map(|mode| (mode, cassette));
    let tape_cassette = tape_request.map(|(mode, requested)| {
        (
            mode,
            requested
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| default_cassette(config_path, &config)),
        )
    });
    let (project_root, project_config) = project::load(config_path)?;
    let project_root = project_root.canonicalize().map_err(|error| {
        Error::Input(format!(
            "cannot resolve benchmark project directory: {error}"
        ))
    })?;
    let _network_project = project::network_project(&project_root);
    let configured_measurements = config
        .measurements
        .as_ref()
        .filter(|value| value.enabled)
        .map(|value| project::resolve(&project_root, &value.directory));
    let measurement_path = measurement_dir
        .map(std::path::PathBuf::from)
        .or(configured_measurements);
    let measurement_collectors = config
        .measurements
        .as_ref()
        .filter(|value| value.enabled)
        .map_or(&[][..], |value| value.collectors.as_slice());
    let mut measurement_session = measurement_path
        .as_deref()
        .map(|dir| {
            measurements::Session::start_with_collectors_and_parent(
                &project_root,
                dir,
                measurement_collectors,
                Some(&project::resolve(&project_root, &project_config.tools_dir)),
                std::env::var_os("ASB_CGROUP_PARENT")
                    .is_none()
                    .then_some(project_config.cgroup_parent.as_deref())
                    .flatten()
                    .map(Path::new),
            )
        })
        .transpose()
        .map_err(Error::Run)?;
    if let Some(session) = measurement_session.as_mut() {
        session
            .event(LifecycleEvent::RunStarted)
            .map_err(Error::Run)?;
    }
    if let Some((tape::Mode::Replay, cassette)) = tape_cassette.as_ref() {
        models::ensure_replay_tool_capable(cassette, &config.model)?;
    }
    let mut discovered_tool_support = None;
    let selected_offer = instrument_phase(
        measurement_session.as_mut(),
        LifecyclePhase::ProviderDiscovery,
        || {
            agents::find(&config.agent)
        .map(|adapter| {
            let offered = if let Some((tape::Mode::Replay, cassette)) = tape_cassette.as_ref() {
                providers::replay_endpoints(&config.provider, &config.model, models::replay_interface(cassette)?)
            } else {
                let (offers, snapshot_fallback, tool_support) = providers::discover_endpoints(&config.provider, &config.model);
                discovered_tool_support = tool_support;
                if snapshot_fallback && !json {
                    emit_warning(warnings, output, format!("Warning: current endpoint metadata is unavailable for provider `{}` and model `{}`; using the built-in endpoint snapshot, which may be out of date.", config.provider, config.model))?;
                }
                offers
            };
            crate::endpoint::choose_from(
                adapter.id(),
                adapter.supported_endpoints(),
                &config.provider,
                &config.model,
                &offered,
            )
        })
        .transpose()
        },
    )?;
    let tape_mode = tape_request.map(|(mode, _)| mode);
    let bridge = instrument_phase(
        measurement_session.as_mut(),
        LifecyclePhase::ProviderSetup,
        || {
            let bridge = providers::start_local_bridge(
                &config.provider,
                &project_root,
                &project_config.tools_dir,
                tape_mode == Some(tape::Mode::Replay),
            )?;
            if tape_mode != Some(tape::Mode::Replay) {
                let endpoint =
                    providers::local_bridge_endpoint(&config.provider, bridge.as_deref());
                let bridge_catalog = providers::local_bridge_credentials(
                    &config.provider,
                    bridge.as_deref(),
                    endpoint,
                )
                .map(|(key, url)| (url, key));
                if discovered_tool_support == Some(false) {
                    return Err(Error::Input(format!(
                        "provider metadata reports that model `{}` does not support tools",
                        config.model
                    )));
                }
                if !models::ensure_tool_capable(&config.provider, &config.model, bridge_catalog)?
                    && discovered_tool_support != Some(true)
                    && !json
                {
                    emit_warning(
                        warnings,
                        output,
                        format!(
                            "Warning: provider `{}` lists model `{}` but does not report whether it supports tools; the agent may fail if the model cannot use tools.",
                            config.provider, config.model
                        ),
                    )?;
                }
            }
            Ok(bridge)
        },
    )?;
    let workspace_name = if config.workspace == "asb-workspace" {
        &project_config.workspace_dir
    } else {
        &config.workspace
    };
    let workspace = project::resolve(&project_root, workspace_name);
    let workload_directory = std::env::var_os("ASB_WORKLOAD_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| project::resolve(&project_root, &project_config.workloads_dir));
    let allocation = |reference: &workloads::recovery::AttemptReference| {
        options.on_completion.map_or(Ok(()), |observer| {
            observer
                .allocated(options.replica, reference)
                .map_err(|error| error.to_string())
        })
    };
    let mut workload_session = instrument_phase(
        measurement_session.as_mut(),
        LifecyclePhase::WorkloadPreparation,
        || {
            workloads::preflight(
                &config.workload,
                &workloads::PreflightContext {
                    project_root: &project_root,
                    project_config: &project_config,
                    agent: &config.agent,
                },
            )?;
            std::fs::create_dir_all(&workspace).map_err(|error| {
                Error::Run(format!(
                    "cannot create workspace {}: {error}",
                    workspace.display()
                ))
            })?;
            workloads::prepare_task(&workloads::PreparationContext {
                verify_initial_material: options.preparation_gate.is_some()
                    || options.verify_initial_material,
                on_allocation: Some(&allocation),
                workload: &config.workload,
                trial: config.trial.as_deref(),
                trial_set: config.trial_set.as_deref(),
                workload_directory: &workload_directory,
                workspace: &workspace,
                project_root: &project_root,
                project_config: &project_config,
            })
            .map_err(Error::Run)
        },
    )?;
    let prompt = workload_session.invocation().prompt.clone();
    let workspace = workload_session.invocation().workspace.clone();
    if let Some(arrival) = arrival.as_mut() {
        let identity = (|| {
            let task = &workload_session.invocation().identity;
            let material_sha256 = workload_session
                .initial_material_sha256()
                .map_err(Error::Run)?;
            let dataset_sha256 = task
                .dataset
                .as_ref()
                .map(|dataset| {
                    serde_json::to_vec(dataset).map(|bytes| crate::cassette::digest(&bytes))
                })
                .transpose()
                .map_err(|error| Error::Run(error.to_string()))?;
            Ok(crate::cassette::Compatibility {
                agent: config.agent.clone(),
                provider: config.provider.clone(),
                model: config.model.clone(),
                workload: task.workload.clone(),
                task: task.task_id.clone(),
                dataset_sha256,
                prompt_sha256: crate::cassette::digest(prompt.as_bytes()),
                material_sha256,
            })
        })();
        let ready = identity.and_then(|identity| arrival.ready(identity));
        if let Err(error) = ready {
            return Err(abort_prepared(error, arrival, workload_session.as_mut()));
        }
    }
    let workload_baseline = workload_session.baseline_evidence();
    let workload_baseline_passed = workload_baseline.measured_passed();
    if !json && workload_baseline_passed == Some(true) {
        emit_warning(warnings, output, "The workload already passes before the agent starts. Files are preserved, but this run cannot earn task-resolution credit. Use a fresh workspace for a new benchmark attempt.".into())?;
    }
    if !json
        && !workload_baseline.supports(workload_session.evaluation_kind())
        && workload_baseline_passed != Some(true)
        && workloads::resolution_eligible(workload_session.evaluation_kind())
    {
        emit_warning(warnings, output, "The initial workload baseline could not be verified. This run cannot earn task-resolution credit. Inspect the workload fixture and use a fresh workspace.".into())?;
    }
    if !json && workload_session.evaluation_kind() == "adapter-smoke" {
        emit_warning(warnings, output, "Selected external workload runs only an adapter smoke exercise, not an upstream benchmark task. Benchmark task success will be unavailable; an official dataset and evaluator are not integrated yet.".into())?;
    }
    let mut tape_session = instrument_phase(
        measurement_session.as_mut(),
        LifecyclePhase::ProxyPreparation,
        || {
            if let Some((mode, cassette)) = tape_cassette.as_ref() {
                let bridge_url =
                    providers::local_bridge_endpoint(&config.provider, bridge.as_deref());
                let upstream = if *mode == tape::Mode::Replay {
                    None
                } else if let Some(offer) = selected_offer.as_ref() {
                    Some(providers::selected_tape_upstream(
                        &config.provider,
                        offer.clone(),
                        bridge_url,
                    )?)
                } else {
                    providers::tape_upstream(&config.provider, bridge_url)
                };
                Ok(Some(tape::Session::start(
                    *mode,
                    cassette,
                    upstream.as_deref(),
                    respect_record_timings,
                    &workspace,
                )?))
            } else {
                Ok(None)
            }
        },
    )?;
    let tape_url = tape_session.as_ref().map(|session| session.url());
    let provider_endpoint =
        tape_url.or_else(|| providers::local_bridge_endpoint(&config.provider, bridge.as_deref()));
    let provider_credentials =
        providers::local_bridge_credentials(&config.provider, bridge.as_deref(), provider_endpoint);
    let connection = selected_offer
        .map(|offer| {
            providers::endpoint_connection(
                &config.provider,
                offer,
                provider_credentials,
                tape_url.zip(tape_mode).map(|(url, mode)| (mode, url)),
            )
        })
        .transpose()?;
    let started = Instant::now();
    let isolated = workload_session.execute(
        &mut workloads::ExecutionContext {
            config: &config,
            connection: connection.as_ref(),
            project_root: &project_root,
            project_config: &project_config,
            replay: tape_mode == Some(tape::Mode::Replay),
            measurements: measurement_session.as_mut(),
            show_output,
        },
        output,
    );
    let (child, run_dir, guest, execution_elapsed_ms, runtime_output) = if let Some(result) =
        isolated
    {
        let result = result.map_err(Error::Run)?;
        (
            result.output,
            PathBuf::new(),
            Some(result.metadata),
            result.elapsed_ms,
            None,
        )
    } else {
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
            &prompt,
            connection.as_ref().map(|connection| connection.interface),
        )?;
        let mut program = specification.program;
        if let Some(local) =
            agents::resolve(&config.agent, &project_root, &project_config.tools_dir)
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
            command.arg(&prompt);
        }
        command
            .current_dir(&workspace)
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
        let runtime_path = replica::runtime_path(&workspace);
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
        instrument_phase(
            measurement_session.as_mut(),
            LifecyclePhase::AgentPreparation,
            || {
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
            },
        )?;
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
        project::apply_python_path(&mut command, &project_root, Path::new(&program))?;
        crate::tools::apply_runtime_path(&mut command, &project_root, &project_config.tools_dir)?;
        // The process gate must execute the fully prepared command, not the
        // adapter's original executable. Some adapters (for example OpenJiuwen)
        // install a process wrapper that carries compatibility and output fixes;
        // bypassing it at the gate would silently undo those fixes while still
        // appearing to attach collectors correctly.
        let gate_program = command.get_program().to_string_lossy().into_owned();
        let gate_args = command
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let child = run_measured_agent(
            &mut command,
            &gate_program,
            Some(&gate_args),
            measurement_session.as_mut(),
            show_output,
            output,
        )?;
        (child, run_dir, None, None, Some(runtime_path))
    };
    let (proxy_evidence, mut run_error) = match tape_session.take() {
        Some(session) => {
            let completed = session.finish();
            (
                completed.evidence,
                completed.result.err().map(|error| error.to_string()),
            )
        }
        None => (
            crate::proxy::Evidence {
                output_files: Vec::new(),
                cleanup_confirmed: true,
            },
            None,
        ),
    };
    // A failed replay can have served only part of the recording. Its full
    // cassette is not evidence of this attempt's token usage. Keep native
    // usage available, but trust cassette totals only after proxy validation.
    let cassette_usage_verified = tape_mode != Some(tape::Mode::Replay) || run_error.is_none();
    if let Some((tape::Mode::Record, cassette)) = tape_cassette.as_ref() {
        match std::fs::metadata(cassette) {
            Ok(metadata) if metadata.len() > 0 => {}
            Ok(_) => {
                run_error = Some(format!(
                    "record created an empty cassette {}; the agent made no provider request; verify its provider base URL and credentials",
                    cassette.display()
                ));
            }
            Err(error) => {
                run_error = Some(format!(
                    "record completed without creating cassette {}; verify the provider endpoint and proxy adapter installation: {error}",
                    cassette.display()
                ));
            }
        }
    }
    let elapsed_ms = execution_elapsed_ms.unwrap_or_else(|| started.elapsed().as_millis());
    // Prefer the agent's structured final response when deciding whether a
    // zero-exit process succeeded. Tool output can legitimately contain a
    // transient failed command that the agent repairs before its final answer;
    // scanning that whole transcript would incorrectly fail the run.
    let final_response = match guest.as_ref() {
        Some(metadata) => metadata.final_response.clone(),
        None => agents::final_response(&config.agent, &bounded(&child.stdout), &workspace),
    };
    let adapter_failure = guest
        .as_ref()
        .and_then(|metadata| metadata.execution_failure.clone())
        .or_else(|| {
            agents::find(&config.agent).and_then(|adapter| {
                adapter.execution_failure(&bounded(&child.stdout), &bounded(&child.stderr))
            })
        });
    let agent_failure = if adapter_failure.is_some() {
        adapter_failure
    } else if config.agent == "openjiuwen" && final_response.is_none() {
        Some("OpenJiuwen exited without a final response; the last event appears to be an unexecuted tool call. Retry with a compatible OpenJiuwen client or inspect the benchmark log.".into())
    } else {
        process_failure_reason(
            &child.status,
            &child.stdout,
            &child.stderr,
            final_response.as_deref(),
        )
    };
    let process_success = agent_failure.is_none();
    // A usable submission can survive a failed client or proxy teardown.
    // Evaluator errors and process failures are recorded independently.
    let (workload_success, grading_failure) = {
        match instrument_operation(
            measurement_session.as_mut(),
            LifecyclePhase::WorkloadEvaluation,
            || workload_session.evaluate(final_response.as_deref().unwrap_or_default()),
        )? {
            Ok(workloads::Evaluation::Passed) => (Some(true), None),
            Ok(workloads::Evaluation::Failed(reason)) => {
                if let Some(session) = measurement_session.as_mut() {
                    session
                        .event(LifecycleEvent::PhaseFailed(
                            LifecyclePhase::WorkloadEvaluation,
                        ))
                        .map_err(Error::Run)?;
                }
                (
                    Some(false),
                    Some((
                        "workload_check_failed",
                        format!("workload check failed: {reason}"),
                    )),
                )
            }
            Err(error) => (
                None,
                Some((
                    "grader_error",
                    format!("workload grader could not evaluate the result: {error}"),
                )),
            ),
        }
    };
    let cleanup = instrument_operation(
        measurement_session.as_mut(),
        LifecyclePhase::WorkloadCleanup,
        || workload_session.finish(),
    )?;
    let workload_cleanup_confirmed = cleanup.is_ok();
    if let Err(error) = cleanup {
        let message = format!("workload cleanup failed: {error}");
        match run_error.as_mut() {
            Some(previous) => {
                previous.push_str("; ");
                previous.push_str(&message);
            }
            None => run_error = Some(message),
        }
    }
    let success = run_error.is_none() && process_success && workload_success == Some(true);
    let workload_failure =
        grading_failure
            .as_ref()
            .map(|(kind, reason)| workloads::EvaluationFailure {
                kind,
                reason: reason.clone(),
            });
    let failure = if let Some(error) = run_error.as_ref() {
        Some(("run_error", error.clone()))
    } else if let Some(error) = agent_failure {
        Some(("agent_failure", error))
    } else {
        grading_failure
    };
    let adapter_artifacts = guest
        .is_none()
        .then(|| agents::find(&config.agent))
        .flatten()
        .map(|adapter| {
            adapter.collect(&agents::CollectionContext {
                workspace: &workspace,
                run_dir: &run_dir,
                interface: connection.as_ref().map(|connection| connection.interface),
                anthropic_input_includes_cache: crate::providers::anthropic_input_includes_cache(
                    &config.provider,
                ),
            })
        })
        .unwrap_or_default();
    let token_usage = measurements::token_usage_from_sources(
        &config.provider,
        &config.agent,
        &String::from_utf8_lossy(&child.stdout),
        &String::from_utf8_lossy(&child.stderr),
        tape_cassette
            .as_ref()
            .filter(|_| cassette_usage_verified)
            .map(|(_, cassette)| cassette.as_path()),
    );
    let token_usage = measurements::with_artifact_usage(token_usage, adapter_artifacts.token_usage);
    let token_usage = measurements::with_artifact_usage(
        token_usage,
        guest.as_ref().map(|metadata| metadata.token_usage.clone()),
    );
    for path in adapter_artifacts.cleanup_paths {
        let _ = std::fs::remove_file(path);
    }
    let steps = if let Some(metadata) = guest.as_ref() {
        metadata.steps
    } else {
        adapter_artifacts.steps.or_else(|| {
            measurements::steps_from_sources(
                &config.agent,
                &bounded(&child.stdout),
                &bounded(&child.stderr),
            )
        })
    };
    let mut stderr = bounded(&child.stderr);
    if let Some(error) = run_error.as_deref() {
        stderr.push_str("\nASB run error: ");
        stderr.push_str(error);
        stderr.push('\n');
    }
    if let Some((kind, reason)) = &failure {
        stderr.push_str("\nASB failure (");
        stderr.push_str(kind);
        stderr.push_str("): ");
        stderr.push_str(reason);
        stderr.push('\n');
    }
    let run_metrics = crate::collectors::run_metrics(&crate::collectors::RunObservation {
        elapsed_ms,
        success,
    });
    let configured_kind = workload_session.evaluation_kind();
    let evaluation_kind = workloads::scored_kind(configured_kind, &workload_baseline);
    let resolution_eligible = workloads::resolution_eligible(evaluation_kind);
    let result = RunResult {
        execution: options.batch.map(|batch| batch.execution(options.replica)),
        replica_index: Some(options.replica.index),
        execution_cohort: options.execution_cohort,
        schema_version: 2,
        status: if success && evaluation_kind == "invalid-baseline" {
            "unscored"
        } else if success && !resolution_eligible {
            "smoke-passed"
        } else if success {
            "completed"
        } else {
            "failed"
        },
        agent: &config.agent,
        workload: &config.workload,
        task: workload_session.invocation().identity.clone(),
        provider: &config.provider,
        model: &config.model,
        process_success,
        workload_success: resolution_eligible.then_some(workload_success).flatten(),
        workload_evaluation: evaluation_kind,
        workload_baseline_passed,
        workload_baseline,
        workload_failure,
        failure_kind: failure.as_ref().map(|(kind, _)| *kind),
        failure_reason: failure.as_ref().map(|(_, reason)| reason.clone()),
        success_rate: resolution_eligible.then_some(if workload_success == Some(true) {
            1.0
        } else {
            0.0
        }),
        adapter_smoke_success: (evaluation_kind == "adapter-smoke").then_some(success),
        elapsed_ms: run_metrics.elapsed_ms,
        exit_code: child.status.code(),
        token_usage,
        steps,
        cassette: tape_cassette
            .as_ref()
            .map(|(_, cassette)| cassette.display().to_string()),
        measurement_directory: measurement_path
            .as_ref()
            .map(std::path::absolute)
            .transpose()
            .map_err(io_error)?,
        stdout: bounded(&child.stdout),
        stderr,
    };
    write_log(config_path, &config.log_file, &result)?;
    if let Some(session) = measurement_session {
        let mut session = session;
        if !success {
            session
                .event(LifecycleEvent::RunFailed)
                .map_err(Error::Run)?;
        }
        session
            .event(LifecycleEvent::RunFinishing)
            .map_err(Error::Run)?;
        let measured = measurements::RunMeasurement {
            replica_index: result.replica_index,
            execution_cohort: result.execution_cohort,
            schema_version: 2,
            run_id: format!("{}-{}", result.workload, result.elapsed_ms),
            status: result.status,
            success: result.success_rate == Some(1.0),
            process_success: result.process_success,
            workload_success: result.workload_success,
            failure_kind: result.failure_kind,
            failure_reason: result.failure_reason.as_deref(),
            workload_evaluation: result.workload_evaluation,
            workload_baseline_passed: result.workload_baseline_passed,
            workload_baseline: Some(&result.workload_baseline),
            workload_failure: result.workload_failure.as_ref(),
            adapter_smoke_success: result.adapter_smoke_success,
            success_rate: result.success_rate,
            agent: result.agent,
            workload: result.workload,
            task: Some(&result.task),
            provider: result.provider,
            model: result.model,
            elapsed_ms,
            exit_code: result.exit_code,
            token_usage: result.token_usage.clone(),
            steps: result.steps,
            started_unix_ms: session.started_unix_ms(),
        };
        session.finish(measured).map_err(Error::Run)?;
    }
    // This private record preserves evaluator outcomes, including unresolved
    // tasks. Its absence never changes a benchmark grade or blocks execution;
    // a future continuation must also validate its input and evidence seals.
    let mut completion_outputs = vec![workspace.clone()];
    if let Some(path) = measurement_path.as_ref() {
        completion_outputs.push(path.clone());
    }
    if let Some(path) = runtime_output {
        completion_outputs.push(path);
    }
    if let Ok(record) = completion::persist(
        config_path,
        &config.log_file,
        &result,
        workload_cleanup_confirmed,
        &workload_session.checkpoint_evidence(),
        &proxy_evidence,
        &completion::Context {
            replica: options.replica,
            outputs: &completion_outputs,
        },
    ) && let Some(observer) = options.on_completion
    {
        let _ = observer.completed(options.replica, &record);
    }
    if json {
        render_json(&result, output)?;
    } else {
        render_human(&result, output)?;
    }
    failure.map_or(Ok(()), |(_, reason)| Err(Error::Run(reason)))
}

/// Prevent replayed agents from contacting any live provider, updater, or
/// telemetry endpoint while preserving access to the local tape proxy.
///
/// Replay is intentionally fail-closed at the child boundary.  A refused
/// loopback proxy is preferable to an inherited corporate proxy or a direct
/// outbound connection; `NO_PROXY` keeps the ASB-owned loopback endpoint
/// reachable.  Node/Bun clients require the explicit opt-in to honor these
/// standard proxy variables.
fn apply_replay_network_guard(command: &mut Command) {
    const DENY_PROXY: &str = "http://127.0.0.1:9";
    const LOOPBACK: &str = "127.0.0.1,localhost,::1";
    for variable in [
        "http_proxy",
        "HTTP_PROXY",
        "https_proxy",
        "HTTPS_PROXY",
        "all_proxy",
        "ALL_PROXY",
    ] {
        command.env(variable, DENY_PROXY);
    }
    command
        .env("no_proxy", LOOPBACK)
        .env("NO_PROXY", LOOPBACK)
        .env("NODE_USE_ENV_PROXY", "1");
}

fn abort_prepared(
    error: Error,
    arrival: &mut batch::Arrival<'_>,
    session: &mut dyn workloads::WorkloadSession,
) -> Error {
    // Wake siblings before remote preservation/cleanup, which can take minutes.
    arrival.abort(&error.to_string());
    match session.cancel_prepared() {
        Ok(()) => error,
        Err(cleanup) => Error::Run(format!(
            "{error}; preparation cancellation failed: {cleanup}"
        )),
    }
}

fn instrument_operation<T, E>(
    mut session: Option<&mut measurements::Session>,
    phase: LifecyclePhase,
    operation: impl FnOnce() -> Result<T, E>,
) -> Result<Result<T, E>, Error> {
    if let Some(session) = session.as_deref_mut() {
        session
            .event(LifecycleEvent::PhaseBegin(phase))
            .map_err(Error::Run)?;
    }
    let result = operation();
    if let Some(session) = session {
        session
            .event(LifecycleEvent::PhaseEnd(phase))
            .map_err(Error::Run)?;
        if result.is_err() {
            session
                .event(LifecycleEvent::PhaseFailed(phase))
                .map_err(Error::Run)?;
        }
    }
    Ok(result)
}

fn instrument_phase<T>(
    session: Option<&mut measurements::Session>,
    phase: LifecyclePhase,
    operation: impl FnOnce() -> Result<T, Error>,
) -> Result<T, Error> {
    instrument_operation(session, phase, operation)?
}

/// Run the agent while delivering every lifecycle event through the collector
/// interface, including failures after the execution span has begun.
fn run_measured_agent(
    command: &mut Command,
    program: &str,
    target_args: Option<&[String]>,
    mut session: Option<&mut measurements::Session>,
    show_output: bool,
    output: &mut dyn Write,
) -> Result<Output, Error> {
    if let Some(session) = session.as_deref_mut() {
        session.agent_launch_prepared(command);
    }
    let mut gated = false;
    #[cfg(unix)]
    if (session.is_some() || cfg!(target_os = "linux"))
        && let Some(target_args) = target_args
        && gate_target_available(program)
    {
        let executable = crate::process_gate::executable()
            .map_err(|error| Error::Run(format!("cannot resolve ASB process gate: {error}")))?;
        crate::process_gate::install(command, &executable, program, target_args);
        gated = true;
    }
    let mut child = {
        let mut emit = |event| {
            if let Some(session) = session.as_deref_mut() {
                session.event(event).map_err(Error::Run)?;
            }
            Ok(())
        };
        emit(LifecycleEvent::MeasurementsReady)?;
        emit(LifecycleEvent::AgentSpawn)?;
        emit(LifecycleEvent::AgentExecuteBegin)?;
        let child = match command.spawn() {
            Ok(child) => OwnedChild(child, gated),
            Err(error) => {
                emit(LifecycleEvent::AgentExecuteEnd)?;
                emit(LifecycleEvent::AgentSpawnFailed)?;
                return Err(Error::Run(format!("could not start {program}: {error}")));
            }
        };
        emit(LifecycleEvent::AgentStarted)?;
        child
    };
    if let Some(session) = session.as_deref_mut()
        && let Err(error) = session.attach_pid(child.id())
    {
        session
            .event(LifecycleEvent::AgentExecuteEnd)
            .map_err(Error::Run)?;
        session
            .event(LifecycleEvent::AgentAttachFailed)
            .map_err(Error::Run)?;
        return Err(Error::Run(error));
    }
    #[cfg(unix)]
    if gated {
        crate::process_gate::resume(&mut child.0).map_err(|error| {
            Error::Run(format!("could not resume measured agent process: {error}"))
        })?;
    }
    let result = wait_with_output(child, show_output, output);
    if let Some(session) = session {
        session
            .event(LifecycleEvent::AgentExecuteEnd)
            .map_err(Error::Run)?;
        session
            .event(if result.is_ok() {
                LifecycleEvent::AgentExited
            } else {
                LifecycleEvent::AgentWaitFailed
            })
            .map_err(Error::Run)?;
    }
    result.map_err(|error| Error::Run(format!("could not wait for {program}: {error}")))
}

#[cfg(unix)]
fn gate_target_available(program: &str) -> bool {
    let path = Path::new(program);
    if !path.is_file() {
        return !program.contains('/');
    }
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    let Some(first_line) = bytes.split(|byte| *byte == b'\n').next() else {
        return true;
    };
    if !first_line.starts_with(b"#!") {
        return true;
    }
    let interpreter = first_line[2..]
        .split(|byte| byte.is_ascii_whitespace())
        .find(|part| !part.is_empty())
        .and_then(|part| std::str::from_utf8(part).ok());
    interpreter.is_none_or(|value| !value.starts_with('/') || Path::new(value).is_file())
}

fn default_cassette(config_path: &Path, config: &BenchmarkConfig) -> std::path::PathBuf {
    let stem = config_path
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("benchmark");
    config_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("cassettes")
        .join(format!(
            "{stem}-{}-{}-{}.jsonl",
            safe_name(&config.agent),
            safe_name(&config.workload),
            safe_name(&config.model)
        ))
}

fn safe_name(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '-'
            }
        })
        .collect()
}

fn write_log(
    config_path: &Path,
    configured_path: &str,
    result: &RunResult<'_>,
) -> Result<(), Error> {
    let path = resolve_log_path(config_path, configured_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            Error::Run(format!(
                "cannot create log directory {}: {error}",
                parent.display()
            ))
        })?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| Error::Run(format!("cannot open log file {}: {error}", path.display())))?;
    serde_json::to_writer(&mut file, result).map_err(|error| {
        Error::Run(format!("cannot write log file {}: {error}", path.display()))
    })?;
    writeln!(file).map_err(|error| {
        Error::Run(format!(
            "cannot finish log file {}: {error}",
            path.display()
        ))
    })
}

pub(crate) fn resolve_log_path(config_path: &Path, configured_path: &str) -> PathBuf {
    let path = Path::new(configured_path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        config_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(path)
    }
}

fn render_json(result: &RunResult<'_>, output: &mut dyn Write) -> Result<(), Error> {
    let summary = serde_json::json!({
        "execution": result.execution,
        "replica_index": result.replica_index,
        "execution_cohort": result.execution_cohort,
        "schema_version": result.schema_version,
        "agent": result.agent,
        "workload": result.workload,
        "provider": result.provider,
        "model": result.model,
        "task": result.task,
        "process_success": result.process_success,
        "workload_success": result.workload_success,
        "workload_evaluation": result.workload_evaluation,
        "workload_baseline_passed": result.workload_baseline_passed,
        "workload_baseline": result.workload_baseline,
        "workload_failure": result.workload_failure,
        "adapter_smoke_success": result.adapter_smoke_success,
        "failure_kind": result.failure_kind,
        "failure_reason": result.failure_reason,
        "success_rate": result.success_rate,
        "elapsed_ms": result.elapsed_ms,
        "steps": result.steps,
        "token_usage": result.token_usage,
    });
    // Sweep output is JSONL: keep each run summary on exactly one line so
    // callers can stream and parse it without having to recover pretty JSON
    // objects from a mixed human transcript.
    serde_json::to_writer(&mut *output, &summary).map_err(|error| Error::Io(error.to_string()))?;
    writeln!(output).map_err(io_error)
}

fn render_human(result: &RunResult<'_>, output: &mut dyn Write) -> Result<(), Error> {
    writeln!(
        output,
        "success_rate={} elapsed_ms={} steps={} input_tokens={} output_tokens={} cached_tokens={} total_tokens={} evaluation={}",
        result.success_rate.map_or_else(|| "unavailable".into(), |value| format!("{value:.3}")),
        result.elapsed_ms,
        format_token(result.steps),
        format_token(result.token_usage.input_tokens),
        format_token(result.token_usage.output_tokens),
        format_token(result.token_usage.cached_tokens),
        format_token(result.token_usage.total_tokens),
        result.workload_evaluation,
    )
    .map_err(io_error)?;
    Ok(())
}

fn format_token(value: Option<u64>) -> String {
    value.map_or_else(|| "unknown".into(), |value| value.to_string())
}

#[cfg(test)]
fn process_successful(status: &std::process::ExitStatus, stdout: &[u8], stderr: &[u8]) -> bool {
    process_successful_with_final_response(status, stdout, stderr, None)
}

#[cfg(test)]
fn process_successful_with_final_response(
    status: &std::process::ExitStatus,
    stdout: &[u8],
    stderr: &[u8],
    final_response: Option<&str>,
) -> bool {
    process_failure_reason(status, stdout, stderr, final_response).is_none()
}

fn process_failure_reason(
    status: &std::process::ExitStatus,
    stdout: &[u8],
    stderr: &[u8],
    final_response: Option<&str>,
) -> Option<String> {
    let transcript = format!(
        "{}\n{}",
        String::from_utf8_lossy(stdout),
        String::from_utf8_lossy(stderr)
    );
    // Once a structured final response exists, inspect that response only.
    // Earlier command output is diagnostic evidence, not the agent outcome.
    let output = final_response.unwrap_or(&transcript);
    let has_completion_evidence = final_response.is_some()
        || output.contains("Agent finished")
        || output.contains("Saved trajectory to '.asb-mini-swe.traj.json'")
        || output.contains("Saved trajectory to \".asb-mini-swe.traj.json\"");
    // Provider and authentication failures are unambiguous and must remain
    // fatal even when an agent emitted a later structured response. Generic
    // command errors are ambiguous (the agent may have repaired them), so
    // inspect those only in the final response when one is available.
    let has_provider_fatal_marker = [
        "Authentication required. Please use /login command to sign in to your account",
        "Endpoint /v1/chat/completions was not found",
        "litellm.NotFoundError",
        "litellm.InternalServerError",
        "AnthropicException",
        "The API provider's servers are down or overloaded",
    ]
    .iter()
    .any(|marker| transcript.contains(marker));
    let has_command_fatal_marker = ["unexpected keyword argument"]
        .iter()
        .any(|marker| output.contains(marker));
    let cannot_run_tests = [
        "I can’t run tests",
        "I can't run tests",
        "I cannot run tests",
    ]
    .iter()
    .any(|marker| output.contains(marker));
    let has_refusal_marker = [
        "can't inspect",
        "cannot inspect",
        "don't have access to the terminal",
        "do not have access to the terminal",
        "no terminal or filesystem",
        "workspace is read-only",
        "workspace appears to be read-only",
        "unable to modify files",
        "can't modify files",
        "cannot modify files",
    ]
    .iter()
    .any(|marker| output.contains(marker));
    if has_provider_fatal_marker {
        let message = if transcript.contains(
            "Authentication required. Please use /login command to sign in to your account",
        ) {
            "agent reported an authentication failure; check its credentials and provider configuration"
        } else if transcript.contains("Endpoint /v1/chat/completions was not found")
            || transcript.contains("litellm.NotFoundError")
        {
            "agent reported a missing provider endpoint; check the provider URL and API compatibility"
        } else {
            "agent reported a provider error; inspect the benchmark log and check provider availability"
        };
        Some(if status.success() {
            message.into()
        } else {
            format!("{message} ({status})")
        })
    } else if !status.success() {
        Some(format!(
            "agent exited unsuccessfully ({status}); inspect the benchmark log for agent output"
        ))
    } else if (final_response.is_some() || !has_completion_evidence) && has_command_fatal_marker {
        Some("agent reported an incompatible command or argument; inspect the benchmark log and agent configuration".into())
    } else if (final_response.is_some() || !has_completion_evidence) && cannot_run_tests {
        Some(
            "agent reported it could not run tests; inspect the benchmark log and test environment"
                .into(),
        )
    } else if (final_response.is_some() || !has_completion_evidence) && has_refusal_marker {
        Some("agent reported it could not access the terminal or workspace, or could not modify files; inspect the benchmark log and permissions".into())
    } else {
        None
    }
}

enum OutputChunk {
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
    Finished,
}

// std::process::Child does not stop a process on drop. Own it from spawn
// through all collector attachment, output and wait error paths.
struct OwnedChild(Child, bool);

impl OwnedChild {
    fn stop(&mut self) -> std::io::Result<std::process::ExitStatus> {
        #[cfg(target_os = "linux")]
        if self.1 {
            return crate::process_gate::cancel_gate(&mut self.0);
        }
        let _ = self.0.kill();
        self.0.wait()
    }
}

impl std::ops::Deref for OwnedChild {
    type Target = Child;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for OwnedChild {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.stop();
        }
        let _ = self.0.wait();
    }
}

/// Wait for an agent while optionally forwarding both output streams live.
/// Readers run independently so a verbose agent cannot deadlock on a full
/// stdout or stderr pipe. The complete bounded-by-the-existing-runner capture
/// is still returned for logging and token parsing.
fn wait_with_output(
    child: OwnedChild,
    show_output: bool,
    output: &mut dyn Write,
) -> std::io::Result<Output> {
    wait_with_output_streams(
        child,
        show_output,
        output,
        None,
        MAX_AGENT_RUNTIME,
        &mut || Ok(()),
    )
}

pub(crate) fn run_guest_process(
    command: &mut Command,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    runtime: Duration,
) -> Result<Output, Error> {
    #[cfg(target_os = "linux")]
    let mut descendants = crate::process_gate::DescendantScope::new()
        .map_err(|error| Error::Run(format!("cannot retain guest agent descendants: {error}")))?;
    let child = command
        .spawn()
        .map(|child| OwnedChild(child, false))
        .map_err(|error| Error::Run(format!("cannot start guest agent: {error}")))?;
    wait_with_output_streams(child, true, stdout, Some(stderr), runtime, &mut || {
        #[cfg(target_os = "linux")]
        return descendants.quiesce();
        #[cfg(not(target_os = "linux"))]
        Ok(())
    })
    .map_err(|error| Error::Run(format!("cannot wait for guest agent: {error}")))
}

fn wait_with_output_streams(
    mut child: OwnedChild,
    show_output: bool,
    output: &mut dyn Write,
    mut error_output: Option<&mut dyn Write>,
    runtime: Duration,
    after_exit: &mut dyn FnMut() -> std::io::Result<()>,
) -> std::io::Result<Output> {
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("agent stdout was not piped"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| std::io::Error::other("agent stderr was not piped"))?;
    let (sender, receiver) = mpsc::channel();
    let stdout_sender = sender.clone();
    std::thread::spawn(move || read_stream(stdout, OutputStream::Stdout, stdout_sender));
    std::thread::spawn(move || read_stream(stderr, OutputStream::Stderr, sender));

    let mut stdout_capture = Vec::new();
    let mut stderr_capture = Vec::new();
    let mut finished_streams = 0;
    let mut status = None;
    let mut quiesced = false;
    let deadline = Instant::now() + runtime;
    let mut timed_out = false;
    while (finished_streams < 2 || status.is_none()) && !timed_out {
        match receiver.recv_timeout(Duration::from_millis(50)) {
            Ok(OutputChunk::Stdout(chunk)) => {
                if show_output {
                    output.write_all(&chunk)?;
                    output.flush()?;
                }
                stdout_capture.extend_from_slice(&chunk);
            }
            Ok(OutputChunk::Stderr(chunk)) => {
                if show_output {
                    if let Some(error_output) = error_output.as_deref_mut() {
                        error_output.write_all(&chunk)?;
                        error_output.flush()?;
                    } else {
                        output.write_all(&chunk)?;
                        output.flush()?;
                    }
                }
                stderr_capture.extend_from_slice(&chunk);
            }
            Ok(OutputChunk::Finished) => finished_streams += 1,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if status.is_none() {
            status = child.try_wait()?;
            if status.is_some() {
                after_exit()?;
                quiesced = true;
            }
        }
        if status.is_none() && Instant::now() >= deadline {
            timed_out = true;
            status = Some(child.stop()?);
            after_exit()?;
            quiesced = true;
        }
    }
    let status = status.unwrap_or(child.wait()?);
    if !quiesced {
        after_exit()?;
    }
    if timed_out {
        stderr_capture.extend_from_slice(
            format!(
                "agent execution exceeded {} seconds\n",
                runtime.as_secs_f64()
            )
            .as_bytes(),
        );
    }
    Ok(Output {
        status,
        stdout: stdout_capture,
        stderr: stderr_capture,
    })
}

enum OutputStream {
    Stdout,
    Stderr,
}

fn read_stream<R: Read>(mut stream: R, kind: OutputStream, sender: mpsc::Sender<OutputChunk>) {
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(size) => {
                let chunk = buffer[..size].to_vec();
                let message = match kind {
                    OutputStream::Stdout => OutputChunk::Stdout(chunk),
                    OutputStream::Stderr => OutputChunk::Stderr(chunk),
                };
                if sender.send(message).is_err() {
                    return;
                }
            }
            Err(_) => break,
        }
    }
    let _ = sender.send(OutputChunk::Finished);
}

type CommandSpec = agents::CommandSpec;

#[cfg(test)]
fn command_for(
    config: &BenchmarkConfig,
    provider_base_url: Option<&str>,
    prompt: &str,
) -> Result<CommandSpec, Error> {
    let interface = agents::find(&config.agent).and_then(|adapter| {
        crate::endpoint::select(
            adapter.supported_endpoints(),
            &providers::offered_endpoints(&config.provider, &config.model),
        )
        .map(|offer| offer.interface)
    });
    command_for_with_interface(config, provider_base_url, prompt, interface)
}

fn command_for_with_interface(
    config: &BenchmarkConfig,
    provider_base_url: Option<&str>,
    prompt: &str,
    interface: Option<crate::endpoint::Interface>,
) -> Result<CommandSpec, Error> {
    if let Some(adapter) = agents::find(&config.agent) {
        return Ok(adapter.command(&agents::CommandContext {
            config,
            endpoint: provider_base_url,
            interface,
            prompt,
        }));
    }
    let agent_names = agents::names().collect::<Vec<_>>();
    if let Some(candidate) = suggestions::closest(&config.agent, &agent_names) {
        return Err(Error::Input(format!(
            "unknown agent `{}`; did you mean `{candidate}`?",
            config.agent
        )));
    }
    Ok(CommandSpec {
        program: config.agent.clone(),
        args: Vec::new(),
        append_prompt: true,
    })
}

fn bounded(bytes: &[u8]) -> String {
    String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_OUTPUT)]).into_owned()
}

#[cfg(test)]
#[path = "../tests/unit/runner_boundaries.rs"]
mod boundary_tests;

#[cfg(test)]
mod tests {
    use super::{
        RunResult, command_for, render_human, render_json, replica_cassette, resolve_log_path,
        run_with_tape, write_log,
    };
    use crate::benchmark::BenchmarkConfig;
    use crate::measurements::TokenUsage;

    #[test]
    fn intent_publication_failure_stops_before_provider_workload_or_measurement_startup() {
        struct FailedIntent;
        impl super::replica::CompletionObserver for FailedIntent {
            fn starting(
                &self,
                _: super::replica::Identity,
                _: Option<&crate::measurements::ExecutionCohort>,
            ) -> Result<(), crate::Error> {
                Err(crate::Error::Io("fixture intent publication failed".into()))
            }
            fn completed(
                &self,
                _: super::replica::Identity,
                _: &std::path::Path,
            ) -> Result<(), crate::Error> {
                panic!("no attempt should have started")
            }
        }
        let root = std::env::temp_dir()
            .join(crate::workloads::TaskIdentity::local("failed-intent", None).attempt_id);
        std::fs::create_dir_all(&root).unwrap();
        let config = root.join("benchmark.toml");
        std::fs::write(
            &config,
            "agent='fixture'\nworkload='bug-fix'\nprovider='fixture'\nmodel='fixture'\n",
        )
        .unwrap();
        let mut output = Vec::new();
        let error = super::run_with_diagnostics(
            config.to_str().unwrap(),
            super::RunOptions {
                on_completion: Some(&FailedIntent),
                ..Default::default()
            },
            &mut output,
            &std::sync::Mutex::new(Default::default()),
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("fixture intent publication failed")
        );
        assert!(output.is_empty());
        assert_eq!(
            std::fs::read_dir(&root).unwrap().count(),
            1,
            "no workspace, measurements or project state may be created"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn replica_failures_group_only_exact_matches() {
        let mut failures = Vec::new();
        super::add_replica_failure(&mut failures, 1, "permission denied\nrepair A".into());
        super::add_replica_failure(&mut failures, 2, "permission denied\nrepair B".into());
        super::add_replica_failure(&mut failures, 3, "permission denied\nrepair A".into());
        assert_eq!(
            failures,
            vec![
                ("permission denied\nrepair A".into(), vec![1, 3]),
                ("permission denied\nrepair B".into(), vec![2]),
            ]
        );
    }

    #[test]
    fn identical_perfetto_marker_errors_are_grouped_across_four_replicas() {
        let mut failures = Vec::new();
        let error = "cannot write ASB lifecycle markers to /sys/kernel/tracing/trace_marker: Permission denied.\nWhat to do:\n  repair command";
        for replica in 1..=4 {
            super::add_replica_failure(&mut failures, replica, error.into());
        }
        assert_eq!(failures, vec![(error.into(), vec![1, 2, 3, 4])]);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn every_instrumented_phase_closes_and_reports_failure_through_the_collector() {
        use crate::collectors::LifecyclePhase;
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("asb-phase-{}-{unique}", std::process::id()));
        let mut session =
            crate::measurements::Session::start_with_collectors(&dir, &[], None).unwrap();
        let marker = dir.join("marker");
        session.set_test_marker(&marker);
        let phases = [
            LifecyclePhase::ProviderDiscovery,
            LifecyclePhase::ProviderSetup,
            LifecyclePhase::WorkloadPreparation,
            LifecyclePhase::ProxyPreparation,
            LifecyclePhase::AgentPreparation,
            LifecyclePhase::WorkloadEvaluation,
        ];
        let mut expected = String::new();
        for phase in phases {
            let result = super::instrument_operation(Some(&mut session), phase, || {
                Err::<(), _>("test failure")
            })
            .unwrap();
            assert!(result.is_err());
            expected.push_str(&format!(
                "B|{}|asb.{}.{}EI|{}|asb.{}.failed.{}",
                std::process::id(),
                phase.name(),
                session.trace_id(),
                std::process::id(),
                phase.name(),
                session.trace_id()
            ));
        }
        assert_eq!(std::fs::read_to_string(marker).unwrap(), expected);
        drop(session);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(target_os = "linux")]
    fn check_lifecycle(command: &mut std::process::Command, expected: &[&str], fail_attach: bool) {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("asb-events-{}-{unique}", std::process::id()));
        let mut session =
            crate::measurements::Session::start_with_collectors(&dir, &[], None).unwrap();
        let trace_id = session.trace_id().to_owned();
        session.test_attach_failure = fail_attach;
        let marker = dir.join("marker");
        session.set_test_marker(&marker);
        let _ = super::run_measured_agent(
            command,
            "fixture",
            None,
            Some(&mut session),
            false,
            &mut Vec::new(),
        );
        let actual = std::fs::read_to_string(&marker).unwrap();
        let pid = std::process::id();
        let expected = expected
            .iter()
            .map(|event| {
                if *event == "end" {
                    "E".to_owned()
                } else {
                    let (kind, name) = event.split_once(':').unwrap();
                    format!("{kind}|{pid}|{name}.{trace_id}")
                }
            })
            .collect::<String>();
        assert_eq!(actual, expected);
        drop(session);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn measured_agent_emits_all_success_events_in_order() {
        let mut command = std::process::Command::new("/bin/true");
        command
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        check_lifecycle(
            &mut command,
            &[
                "I:asb.measurements.ready",
                "I:asb.agent.spawn",
                "B:asb.agent.execute",
                "I:asb.agent.started",
                "end",
                "I:asb.agent.exited",
            ],
            false,
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn measured_agent_emits_spawn_failure_events_in_order() {
        let mut command = std::process::Command::new("/asb-test-path-that-does-not-exist");
        check_lifecycle(
            &mut command,
            &[
                "I:asb.measurements.ready",
                "I:asb.agent.spawn",
                "B:asb.agent.execute",
                "end",
                "I:asb.agent.spawn_failed",
            ],
            false,
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn measured_agent_emits_wait_failure_events_in_order() {
        let mut command = std::process::Command::new("/bin/true");
        check_lifecycle(
            &mut command,
            &[
                "I:asb.measurements.ready",
                "I:asb.agent.spawn",
                "B:asb.agent.execute",
                "I:asb.agent.started",
                "end",
                "I:asb.agent.wait_failed",
            ],
            false,
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn measured_agent_emits_attach_failure_events_in_order() {
        let mut command = std::process::Command::new("/bin/sleep");
        command
            .arg("10")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        check_lifecycle(
            &mut command,
            &[
                "I:asb.measurements.ready",
                "I:asb.agent.spawn",
                "B:asb.agent.execute",
                "I:asb.agent.started",
                "end",
                "I:asb.agent.attach_failed",
            ],
            true,
        );
    }
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires a project-local Perfetto installation and host tracing privileges"]
    fn live_perfetto_captures_measured_agent_events_in_order() {
        let tools = std::path::PathBuf::from(
            std::env::var_os("ASB_TEST_PERFETTO_TOOLS").expect("set ASB_TEST_PERFETTO_TOOLS"),
        );
        let dir = std::env::var_os("ASB_TEST_PERFETTO_OUTPUT")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| tools.parent().unwrap().join("trace-test-output"));
        capture_perfetto_fixture(&tools, &dir, false, None, false);
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires project-local Perfetto, isolated tracefs delegation and user namespaces"]
    fn live_perfetto_survivor_keeps_markers_and_scope_after_a_sibling_fails() {
        let tools = std::path::PathBuf::from(std::env::var_os("ASB_TEST_PERFETTO_TOOLS").unwrap());
        let root = tools
            .parent()
            .unwrap()
            .join(crate::workloads::TaskIdentity::local("perfetto-overlap", None).attempt_id);
        let barrier =
            std::sync::Arc::new((std::sync::Mutex::new(0usize), std::sync::Condvar::new()));
        let threads = [true, false].map(|fail| {
            let tools = tools.clone();
            let dir = root.join(if fail { "failed-sibling" } else { "survivor" });
            let barrier = std::sync::Arc::clone(&barrier);
            std::thread::spawn(move || {
                capture_perfetto_fixture(&tools, &dir, fail, Some(&barrier), false)
            })
        });
        let outcomes = threads.map(|thread| thread.join());
        let [failed, survivor] = outcomes.map(Result::unwrap);
        assert!(
            failed.0 < survivor.1 && survivor.0 < failed.1,
            "captures did not overlap: {failed:?}, {survivor:?}"
        );
        assert!(
            failed.1 < survivor.1,
            "survivor did not remain active after failure: {failed:?}, {survivor:?}"
        );
        assert!(
            failed.3 < survivor.2,
            "failed sibling recorder had not stopped before survivor execution ended"
        );
        println!("Overlapping Perfetto qualification: {}", root.display());
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires project-local Perfetto, delegated tracefs and user namespaces"]
    fn live_perfetto_streaming_preserves_markers_beyond_the_service_ring_capacity() {
        let tools = std::path::PathBuf::from(std::env::var_os("ASB_TEST_PERFETTO_TOOLS").unwrap());
        let dir = tools
            .parent()
            .unwrap()
            .join(crate::workloads::TaskIdentity::local("perfetto-streaming", None).attempt_id);
        capture_perfetto_fixture(&tools, &dir, false, None, true);
        let output = std::process::Command::new(tools.join("perfetto/bin/trace_processor_shell"))
            .arg("query")
            .arg(dir.join("perfetto-scheduling.perfetto-trace"))
            .arg("SELECT SUM(value) FROM stats WHERE name='traced_buf_bytes_written'")
            .output()
            .unwrap();
        assert!(output.status.success());
        let text = String::from_utf8(output.stdout).unwrap();
        let written: u64 = text.lines().nth(1).unwrap().parse().unwrap();
        assert!(
            written > 128 * 1024 * 1024,
            "did not exceed the configured ring: {written}"
        );
        println!("Streaming trace bytes before compression: {written}");
    }

    #[cfg(target_os = "linux")]
    fn capture_perfetto_fixture(
        tools: &std::path::Path,
        dir: &std::path::Path,
        fail: bool,
        barrier: Option<&(std::sync::Mutex<usize>, std::sync::Condvar)>,
        pressure: bool,
    ) -> (u64, u64, std::time::Instant, std::time::Instant) {
        let mut session = crate::measurements::Session::start_with_collectors(
            dir,
            &["perfetto".into()],
            Some(tools),
        )
        .expect("start real Perfetto");
        let id = session.trace_id().to_owned();
        if pressure {
            // Fixed non-secret test data, paced below the owned kernel buffer's
            // drain capacity. Compression happens after the service ring, so
            // this exceeds the old ring without retaining an enormous file.
            let marker = format!(
                "I|{}|fixture.pressure.{}",
                std::process::id(),
                "x".repeat(1000)
            );
            for _ in 0..640 {
                for _ in 0..256 {
                    session.write_trace_marker(&marker).unwrap();
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        if let Some(barrier) = barrier {
            let mut ready = barrier.0.lock().unwrap();
            *ready += 1;
            barrier.1.notify_all();
            let (ready, _) = barrier
                .1
                .wait_timeout_while(ready, std::time::Duration::from_secs(15), |count| {
                    *count < 2
                })
                .unwrap();
            assert_eq!(
                *ready, 2,
                "sibling did not start within the qualification bound"
            );
        }
        let mut command = std::process::Command::new("/bin/sh");
        command
            .args(["-c", if fail { "sleep 0.2; exit 1" } else { "sleep 6" }])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let execution_started = std::time::Instant::now();
        let output = super::run_measured_agent(
            &mut command,
            "sh",
            None,
            Some(&mut session),
            false,
            &mut Vec::new(),
        )
        .expect("run fixture");
        let agent_ended = std::time::Instant::now();
        let elapsed_ms = execution_started.elapsed().as_millis();
        assert_eq!(output.status.success(), !fail);
        if fail {
            session
                .event(crate::collectors::LifecycleEvent::RunFailed)
                .unwrap();
        }
        session
            .event(crate::collectors::LifecycleEvent::RunFinishing)
            .unwrap();
        session
            .finish(crate::measurements::RunMeasurement {
                replica_index: None,
                execution_cohort: None,
                schema_version: 1,
                run_id: "test".into(),
                status: if fail { "failed" } else { "completed" },
                success: !fail,
                process_success: !fail,
                workload_success: None,
                failure_kind: None,
                failure_reason: None,
                success_rate: None,
                adapter_smoke_success: None,
                agent: "fixture",
                workload: "fixture",
                task: None,
                workload_evaluation: "fixture",
                workload_baseline_passed: Some(false),
                workload_baseline: None,
                workload_failure: None,
                provider: "fixture",
                model: "fixture",
                elapsed_ms,
                exit_code: output.status.code(),
                token_usage: crate::measurements::TokenUsage::default(),
                steps: Some(1),
                started_unix_ms: 0,
            })
            .expect("finish collector");
        let recorder_stopped = std::time::Instant::now();
        let trace = dir.join("perfetto-scheduling.perfetto-trace");
        let query = format!("SELECT name FROM slice WHERE name GLOB 'asb.*.{id}' ORDER BY ts");
        let result = std::process::Command::new(tools.join("perfetto/bin/trace_processor_shell"))
            .arg("query")
            .arg(&trace)
            .arg(query)
            .output()
            .expect("trace processor");
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let rows = String::from_utf8_lossy(&result.stdout);
        let expected = [
            "asb.measurements.ready",
            "asb.agent.spawn",
            "asb.agent.execute",
            "asb.agent.started",
            "asb.agent.exited",
        ];
        let mut cursor = 0;
        for name in expected {
            let needle = format!("{name}.{id}");
            let offset = rows[cursor..]
                .find(&needle)
                .unwrap_or_else(|| panic!("missing {needle} in {rows}"));
            cursor += offset + needle.len();
        }
        let markers: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("perfetto-tracepoints.json")).unwrap())
                .unwrap();
        assert_eq!(markers["marker_verified"], true);
        assert!(markers["marker_warning"].is_null(), "{markers}");
        assert!(markers["scope_warning"].is_null(), "{markers}");
        assert_eq!(markers["capture_complete"], true, "{markers}");
        assert!(markers["capture_warning"].is_null(), "{markers}");
        let data: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("perfetto.json")).unwrap()).unwrap();
        assert!(
            data["metrics"]["perfetto.scheduler_events"]
                .as_f64()
                .unwrap_or(0.0)
                > 0.0,
            "{data}"
        );
        let bounds = std::process::Command::new(tools.join("perfetto/bin/trace_processor_shell"))
            .arg("query").arg(&trace)
            .arg(format!("SELECT MIN(ts), MAX(ts) FROM slice WHERE name IN ('asb.agent.started.{id}', 'asb.agent.exited.{id}')"))
            .output().unwrap();
        assert!(bounds.status.success());
        let text = String::from_utf8(bounds.stdout).unwrap();
        let values: Vec<u64> = text
            .lines()
            .nth(1)
            .unwrap()
            .split(',')
            .map(|s| s.parse().unwrap())
            .collect();
        assert_eq!(values.len(), 2);
        assert!(values[1] > values[0]);
        println!("Perfetto qualification: {}", dir.display());
        (values[0], values[1], agent_ended, recorder_stopped)
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn child_is_killed_and_reaped_if_collector_attachment_aborts() {
        let child = std::process::Command::new("sleep")
            .arg("10")
            .spawn()
            .unwrap();
        let pid = child.id();
        let owned = super::OwnedChild(child, false);
        // Simulate returning from the runner after attachment fails.
        drop(owned);
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn child_is_killed_and_reaped_when_output_writer_fails() {
        struct FailingWriter;
        impl std::io::Write for FailingWriter {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("fixture output failure"))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let child = std::process::Command::new("sh")
            .args(["-c", "printf ready; exec sleep 10"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let pid = child.id();
        let error =
            super::wait_with_output(super::OwnedChild(child, false), true, &mut FailingWriter)
                .unwrap_err();
        assert!(error.to_string().contains("fixture output failure"));
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    }

    #[test]
    fn concurrent_replay_fans_out_single_recording_and_supports_replica_recordings() {
        let root =
            std::env::temp_dir().join(format!("asb-replica-cassettes-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let base = root.join("record.jsonl");
        assert_eq!(
            replica_cassette(crate::tape::Mode::Replay, &base, 8),
            root.join("record-concurrency-8.jsonl")
        );
        std::fs::write(&base, "test").unwrap();
        for index in 1..=8 {
            assert_eq!(
                replica_cassette(crate::tape::Mode::Replay, &base, index),
                base
            );
        }
        std::fs::remove_file(&base).unwrap();
        let recorded_replica = root.join("record-concurrency-2.jsonl");
        std::fs::write(&recorded_replica, "test").unwrap();
        assert_eq!(
            replica_cassette(crate::tape::Mode::Replay, &base, 3),
            recorded_replica
        );
        assert_eq!(
            replica_cassette(crate::tape::Mode::Record, &base, 8),
            root.join("record-concurrency-8.jsonl")
        );
        std::fs::remove_file(&recorded_replica).unwrap();
        let last_replica = root.join("record-concurrency-64.jsonl");
        std::fs::write(&last_replica, "test").unwrap();
        assert_eq!(
            replica_cassette(crate::tape::Mode::Replay, &base, 1),
            last_replica
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn concurrent_replay_without_explicit_cassette_fails_closed() {
        let root =
            std::env::temp_dir().join(format!("asb-replay-no-cassette-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let config = root.join("benchmark.toml");
        std::fs::write(&config, "agent='pi'\nworkload='bug-fix'\nprovider='not-a-provider'\nmodel='model'\nconcurrency=8\n").unwrap();
        let error = run_with_tape(
            config.to_str().unwrap(),
            super::RunOptions {
                mode: Some(crate::tape::Mode::Replay),
                ..Default::default()
            },
            &mut Vec::new(),
        )
        .unwrap_err();
        let message = error.to_string();
        assert!(message.contains("replay cassette"), "{message}");
        assert!(!message.contains("unsupported provider"), "{message}");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn completion_records_preserve_unresolved_outcomes_cleanup_and_private_evidence() {
        use super::completion;
        use crate::workloads;
        let root = std::env::temp_dir()
            .join(workloads::TaskIdentity::local("completion-record-test", None).attempt_id);
        std::fs::create_dir_all(&root).unwrap();
        let config = root.join("benchmark.toml");
        let mut result = result();
        result.stdout = "private agent transcript must not enter checkpoint".into();
        result.stderr = "private agent stderr must not enter checkpoint".into();
        let evidence = vec![root.join("official-evaluator-evidence")];
        for resolved in [true, false] {
            result.workload_success = Some(resolved);
            result.success_rate = Some(if resolved { 1.0 } else { 0.0 });
            completion::persist(
                &config,
                "run.log",
                &result,
                resolved,
                &evidence,
                &crate::proxy::Evidence {
                    output_files: Vec::new(),
                    cleanup_confirmed: true,
                },
                &Default::default(),
            )
            .unwrap();
        }
        let directory = root.join(".asb-completions");
        let files: Vec<_> = std::fs::read_dir(&directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert_eq!(files.len(), 2, "outcomes must not overwrite one another");
        let mut outcomes = Vec::new();
        for file in files {
            assert_eq!(file.extension().unwrap(), "json");
            let bytes = std::fs::read(&file).unwrap();
            let text = String::from_utf8(bytes.clone()).unwrap();
            assert!(!text.contains("private agent"));
            let record: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(record["schema_version"], 2);
            assert_eq!(record["replica"], serde_json::json!({"index":1,"total":1}));
            assert_eq!(record["outputs"], serde_json::json!([root.join("run.log")]));
            assert_eq!(record["scope"], "task-outcome-not-yet-a-resume-receipt");
            assert_eq!(
                record["summary"]["task"]["attempt_id"],
                result.task.attempt_id
            );
            assert_eq!(
                record["workload_evidence"][0],
                evidence[0].to_str().unwrap()
            );
            let resolved = record["summary"]["workload_success"].as_bool().unwrap();
            assert_eq!(record["workload_cleanup_confirmed"], resolved);
            assert_eq!(record["proxy_cleanup_confirmed"], true);
            assert_eq!(record["proxy_evidence"], serde_json::json!([]));
            outcomes.push(resolved);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    std::fs::metadata(file).unwrap().permissions().mode() & 0o777,
                    0o600
                );
            }
        }
        outcomes.sort();
        assert_eq!(outcomes, [false, true]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
                0o700
            );
            let other = root.join("other");
            std::fs::create_dir(&other).unwrap();
            std::os::unix::fs::symlink(&other, root.join("redirect")).unwrap();
            std::os::unix::fs::symlink(&other, other.join(".asb-completions")).unwrap();
            assert!(
                completion::persist(
                    &root.join("redirect/benchmark.toml"),
                    "run.log",
                    &result,
                    true,
                    &[],
                    &crate::proxy::Evidence {
                        output_files: Vec::new(),
                        cleanup_confirmed: true
                    },
                    &Default::default(),
                )
                .is_err()
            );
            assert_eq!(std::fs::read_dir(&other).unwrap().count(), 1);
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    fn result<'a>() -> RunResult<'a> {
        RunResult {
            execution: None,
            replica_index: None,
            execution_cohort: None,
            schema_version: 2,
            status: "completed",
            agent: "codex",
            workload: "bug-fix",
            task: crate::workloads::TaskIdentity::local("bug-fix", None),
            provider: "openrouter",
            model: "test-model",
            process_success: true,
            workload_success: Some(true),
            workload_evaluation: "bundled-fixture",
            workload_baseline_passed: Some(false),
            workload_baseline: crate::workloads::BaselineEvidence::Measured {
                passed: Some(false),
            },
            workload_failure: None,
            failure_kind: None,
            failure_reason: None,
            success_rate: Some(1.0),
            adapter_smoke_success: None,
            elapsed_ms: 12,
            exit_code: Some(0),
            token_usage: TokenUsage {
                input_tokens: Some(3),
                output_tokens: Some(4),
                cached_tokens: Some(1),
                total_tokens: Some(7),
                source: Some("test".into()),
            },
            steps: Some(2),
            cassette: None,
            measurement_directory: None,
            stdout: "done".into(),
            stderr: String::new(),
        }
    }

    #[test]
    fn human_output_is_concise_and_not_json() {
        let mut output = Vec::new();
        render_human(&result(), &mut output).expect("human rendering succeeds");
        let text = String::from_utf8(output).expect("utf8");
        assert!(text.starts_with("success_rate=1.000"));
        assert!(text.contains("elapsed_ms=12"));
        assert!(text.contains("steps=2"));
        assert!(text.contains("cached_tokens=1"));
        assert!(text.contains("total_tokens=7"));
        assert!(!text.contains("done"));
        assert!(!text.starts_with('{'));
    }

    #[test]
    fn json_output_remains_machine_readable() {
        let mut output = Vec::new();
        render_json(&result(), &mut output).expect("json rendering succeeds");
        let value: serde_json::Value = serde_json::from_slice(&output).expect("valid json");
        assert_eq!(value["success_rate"], 1.0);
        assert_eq!(value["elapsed_ms"], 12);
        assert_eq!(value["steps"], 2);
        assert_eq!(value["token_usage"]["total_tokens"], 7);
        assert_eq!(value["token_usage"]["cached_tokens"], 1);
        let expected = result();
        assert_eq!(value["agent"], expected.agent);
        assert_eq!(value["workload"], expected.workload);
        assert_eq!(value["provider"], expected.provider);
        assert_eq!(value["model"], expected.model);
        assert!(value.get("stdout").is_none());
        assert_eq!(output.iter().filter(|byte| **byte == b'\n').count(), 1);
    }

    #[test]
    fn log_path_is_relative_to_configuration_file() {
        assert_eq!(
            resolve_log_path(std::path::Path::new("projects/foo.toml"), "foo.log"),
            std::path::PathBuf::from("projects/foo.log")
        );
        assert_eq!(
            resolve_log_path(
                std::path::Path::new("projects/foo.toml"),
                "/tmp/asb-run.log"
            ),
            std::path::PathBuf::from("/tmp/asb-run.log")
        );
    }

    #[test]
    fn log_contains_full_bounded_result_while_summary_hides_agent_output() {
        let directory = std::env::temp_dir().join(format!("asb-runner-log-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("log test directory");
        let config = directory.join("foo.toml");
        write_log(&config, "foo.log", &result()).expect("log writes");
        let text = std::fs::read_to_string(directory.join("foo.log")).expect("log reads");
        let value: serde_json::Value = serde_json::from_str(text.trim()).expect("log JSON");
        assert_eq!(value["stdout"], "done");
        assert_eq!(value["success_rate"], 1.0);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn cli2key_command_mappings_place_workload_once() {
        let config = BenchmarkConfig {
            agent: "openhands".into(),
            workload: "bug-fix".into(),
            provider: "cli2key".into(),
            model: "gpt-test".into(),
            concurrency: 1,
            trial_set: None,
            trial: None,
            workspace: "workspace".into(),
            log_file: "run.log".into(),
            measurements: None,
        };
        let command = command_for(&config, Some("http://127.0.0.1:3456/v1"), "prompt")
            .expect("known agent has a command");
        assert_eq!(command.program, "openhands");
        assert!(!command.append_prompt);
        assert!(command.args.contains(&"--json".into()));
        assert_eq!(command.args.last().map(String::as_str), Some("prompt"));
    }

    #[test]
    fn token_reporting_commands_request_machine_readable_agent_output() {
        let base = BenchmarkConfig {
            agent: "mini-swe".into(),
            workload: "bug-fix".into(),
            provider: "cli2key".into(),
            model: "gpt-test".into(),
            concurrency: 1,
            trial_set: None,
            trial: None,
            workspace: "workspace".into(),
            log_file: "run.log".into(),
            measurements: None,
        };
        let mini = command_for(&base, Some("http://127.0.0.1:3456/v1"), "prompt").unwrap();
        assert!(
            mini.args
                .windows(2)
                .any(|pair| pair[0] == "--output" && pair[1] == ".asb-mini-swe.traj.json")
        );

        let mut openjiuwen = base.clone();
        openjiuwen.agent = "openjiuwen".into();
        let command = command_for(&openjiuwen, Some("http://127.0.0.1:3456/v1"), "prompt").unwrap();
        assert!(
            command
                .args
                .windows(2)
                .any(|pair| pair[0] == "-f" && pair[1] == "stream-json")
        );
    }

    #[test]
    fn newly_supported_agent_commands_are_noninteractive_and_machine_readable() {
        for (agent, program, marker) in [
            ("claude-code", "claude", "--output-format"),
            ("goose", "goose", "--output-format"),
            ("cline", "cline", "--json"),
            ("continue", "cn", "--auto"),
        ] {
            let config = BenchmarkConfig {
                agent: agent.into(),
                workload: "bug-fix".into(),
                provider: "cli2key".into(),
                model: "gpt-test".into(),
                concurrency: 1,
                trial_set: None,
                trial: None,
                workspace: "workspace".into(),
                log_file: "run.log".into(),
                measurements: None,
            };
            let command = command_for(&config, Some("http://127.0.0.1:3456/v1"), "prompt")
                .expect("new agent command");
            assert_eq!(command.program, program, "{agent} program");
            assert!(
                command.args.iter().any(|value| value == marker),
                "{agent} marker"
            );
            assert!(!command.append_prompt, "{agent} embeds prompt exactly once");
        }
    }

    #[test]
    fn pi_cli2key_uses_openai_bridge_and_print_mode() {
        let config = BenchmarkConfig {
            agent: "pi".into(),
            workload: "bug-fix".into(),
            provider: "cli2key".into(),
            model: "gpt-6.1-sol".into(),
            concurrency: 1,
            trial_set: None,
            trial: None,
            workspace: "workspace".into(),
            log_file: "run.log".into(),
            measurements: None,
        };
        let command = command_for(&config, Some("http://127.0.0.1:3456/v1"), "prompt")
            .expect("pi has a command");
        assert_eq!(command.program, "pi");
        assert_eq!(command.args[0], "--provider");
        assert_eq!(command.args[1], "asb");
        assert!(command.args.contains(&"--print".into()));
        assert!(command.args.contains(&"--mode".into()));
        assert!(command.args.contains(&"json".into()));
        assert!(command.append_prompt);
    }

    #[test]
    fn zero_exit_provider_errors_are_not_successes() {
        let status = std::process::Command::new("true")
            .status()
            .expect("true exists");
        assert!(!super::process_successful(
            &status,
            b"Authentication required. Please use /login command to sign in to your account",
            b""
        ));
        assert!(!super::process_successful(
            &status,
            b"litellm.NotFoundError: Endpoint /v1/chat/completions was not found.",
            b""
        ));
        assert!(!super::process_successful(
            &status,
            b"litellm.InternalServerError: AnthropicException - unexpected keyword argument 'system'",
            b""
        ));
        assert!(super::process_successful(&status, b"completed", b""));
    }

    #[test]
    fn zero_exit_tool_refusals_are_not_successes() {
        let status = std::process::Command::new("true")
            .status()
            .expect("true exists");
        assert!(!super::process_successful(
            &status,
            b"I can't run tests or inspect src/parser.py because I do not have access to the terminal",
            b""
        ));
        assert!(!super::process_successful(
            &status,
            b"The workspace appears to be read-only; I can't modify files",
            b""
        ));
    }

    #[test]
    fn agent_failure_reasons_are_specific_without_exposing_transcript() {
        let status = std::process::Command::new("true").status().unwrap();
        for (output, expected) in [
            (
                "Authentication required. Please use /login command to sign in to your account secret-value",
                "authentication failure",
            ),
            (
                "Endpoint /v1/chat/completions was not found secret-value",
                "missing provider endpoint",
            ),
            ("litellm.InternalServerError secret-value", "provider error"),
            (
                "unexpected keyword argument secret-value",
                "incompatible command or argument",
            ),
            ("I can't run tests secret-value", "could not run tests"),
            (
                "workspace is read-only secret-value",
                "could not access the terminal or workspace",
            ),
        ] {
            let reason =
                super::process_failure_reason(&status, output.as_bytes(), b"", None).unwrap();
            assert!(reason.contains(expected), "{reason}");
            assert!(!reason.contains("secret-value"));
        }
        let failed_status = std::process::Command::new("false").status().unwrap();
        let reason = super::process_failure_reason(
            &failed_status,
            b"Authentication required. Please use /login command to sign in to your account secret-value",
            b"",
            None,
        ).unwrap();
        assert!(reason.contains("authentication failure"), "{reason}");
        assert!(reason.contains("exit status"), "{reason}");
        assert!(!reason.contains("secret-value"));
    }

    #[test]
    fn transient_agent_test_errors_do_not_hide_completion() {
        let status = std::process::Command::new("true")
            .status()
            .expect("true exists");
        assert!(super::process_successful(
            &status,
            b"Tool returned TypeError: unexpected keyword argument 'reverse'\nAgent finished",
            b""
        ));
        assert!(super::process_successful(
            &status,
            b"Tool returned TypeError: unexpected keyword argument 'reverse'\nSaved trajectory to '.asb-mini-swe.traj.json'",
            b""
        ));
        assert!(super::process_successful_with_final_response(
            &status,
            b"Tool returned TypeError: unexpected keyword argument 'reverse'",
            b"",
            Some("Updated the implementation and verified the supplied test passes."),
        ));
        assert!(!super::process_successful_with_final_response(
            &status,
            b"Tool output",
            b"",
            Some("I cannot modify files in the workspace."),
        ));
        assert!(!super::process_successful_with_final_response(
            &status,
            b"litellm.InternalServerError: upstream unavailable",
            b"",
            Some("Updated the implementation and verified the supplied test passes."),
        ));
        assert!(!super::process_successful_with_final_response(
            &status,
            b"litellm.InternalServerError: upstream unavailable\nAgent finished",
            b"",
            None,
        ));
    }

    #[test]
    fn opencode_command_uses_generic_provider_model_namespace() {
        let config = BenchmarkConfig {
            agent: "opencode".into(),
            workload: "bug-fix".into(),
            provider: "opencode-zen".into(),
            model: "mimo-v2.6-flash-free".into(),
            concurrency: 1,
            trial_set: None,
            trial: None,
            workspace: "workspace".into(),
            log_file: "run.log".into(),
            measurements: None,
        };
        let command = command_for(&config, None, "prompt").expect("known agent");
        assert!(
            command.args.windows(2).any(|pair| {
                pair == ["--model".to_owned(), "asb/mimo-v2.6-flash-free".to_owned()]
            })
        );
        assert!(
            command
                .args
                .windows(2)
                .any(|pair| { pair == ["--dir".to_owned(), "workspace".to_owned()] })
        );
    }

    #[test]
    fn unknown_agent_suggests_a_close_match() {
        let config = BenchmarkConfig {
            agent: "opencod".into(),
            workload: "bug-fix".into(),
            provider: "local".into(),
            model: "test-model".into(),
            concurrency: 1,
            trial_set: None,
            trial: None,
            workspace: "workspace".into(),
            log_file: "run.log".into(),
            measurements: None,
        };
        let error = match command_for(&config, None, "prompt") {
            Ok(_) => panic!("typo rejected"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("did you mean `opencode`?"));
    }

    #[test]
    fn replay_network_guard_allows_only_loopback_proxy_targets() {
        let mut command = std::process::Command::new("agent");
        super::apply_replay_network_guard(&mut command);
        let values = command
            .get_envs()
            .filter_map(|(key, value)| {
                value.map(|value| {
                    (
                        key.to_string_lossy().into_owned(),
                        value.to_string_lossy().into_owned(),
                    )
                })
            })
            .collect::<std::collections::HashMap<_, _>>();
        for variable in [
            "http_proxy",
            "HTTP_PROXY",
            "https_proxy",
            "HTTPS_PROXY",
            "all_proxy",
            "ALL_PROXY",
        ] {
            assert_eq!(values[variable], "http://127.0.0.1:9");
        }
        assert_eq!(values["NO_PROXY"], "127.0.0.1,localhost,::1");
        assert_eq!(values["no_proxy"], "127.0.0.1,localhost,::1");
        assert_eq!(values["NODE_USE_ENV_PROXY"], "1");
    }
}
