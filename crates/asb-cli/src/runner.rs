//! Live execution of one configured benchmark.
mod batch;
pub(crate) mod campaign;
mod completion;
pub(crate) mod diagnostics;
pub(crate) mod disposition;
#[cfg(test)]
use disposition::RunStatus;
use disposition::{FailureKind, RunDisposition, RunFacts, RunFailure};
mod native;
mod phases;
pub(crate) mod recording;
mod repeated;
use phases::{
    AdmittedAttempt, AttemptIssues, ExecutionEvidence, FinalizedAttempt, MeasuredOutput,
    PreparedAttempt,
};
pub(crate) mod replica;
pub(crate) mod reuse;
#[cfg(target_os = "linux")]
pub(crate) mod workload_process;

use super::{Error, io_error, read_config};
use crate::agents;
use crate::benchmark::BenchmarkConfig;
use crate::benchmark_paths::component;
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
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

const MAX_OUTPUT: usize = 1024 * 1024;
const MAX_AGENT_RUNTIME: Duration =
    Duration::from_secs(crate::planning::DEFAULT_NATIVE_EXECUTION_SECONDS);

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
    recording: Option<recording::Evidence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    execution: Option<repeated::ExecutionIdentity>,
    planning: crate::planning::ResolvedPlan,
    effective_budgets: crate::planning::EffectiveBudgets,
    execution_plan: Option<PathBuf>,
    execution_plan_fingerprint: Option<crate::planning::ExecutionPlanFingerprint>,
    module: Option<workloads::contracts::ModuleDescriptor>,
    benchmark_outcome: Option<workloads::outcome::BenchmarkOutcome>,
    replica_index: Option<usize>,
    execution_cohort: Option<&'a measurements::ExecutionCohort>,
    schema_version: u32,
    #[serde(flatten)]
    disposition: RunDisposition,
    agent: &'a str,
    workload: &'a str,
    task: workloads::TaskIdentity,
    provider: &'a str,
    model: &'a str,
    process_success: bool,
    workload_success: Option<bool>,
    workload_resource_disposition: Option<workloads::ResourceDisposition>,
    workload_artifacts: Vec<PathBuf>,
    workload_evaluation: &'static str,
    workload_baseline_passed: Option<bool>,
    workload_baseline: workloads::BaselineEvidence,
    workload_failure: Option<workloads::EvaluationFailure>,
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
    pub track: Option<crate::cassette::Selector>,
    pub replay_seed: Option<u64>,
    pub recording: Option<&'a recording::Recorder>,
    pub repeat: Option<u64>,
    pub batch: Option<&'a repeated::BatchIdentity>,
    pub verify_initial_material: bool,
    pub preparation_gate: Option<&'a batch::Gate>,
    pub launch_gate: Option<&'a batch::Gate>,
    pub policy: Option<&'a crate::planning::ResolvedPlan>,
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
    if (options.track.is_some() || options.replay_seed.is_some())
        && options.mode != Some(tape::Mode::Replay)
    {
        return Err(Error::Input("--track and --seed require --replay".into()));
    }
    let bundled_replay = if options.mode == Some(tape::Mode::Replay) {
        let source = replay_cassette(config_path, &config, options.cassette)?;
        crate::cassette::Source::is_bundle(&source)
            .map_err(|error| Error::Input(error.to_string()))?
    } else {
        false
    };
    if let Some(repeat) = options.repeat.or((bundled_replay
        || options.track.is_some()
        || options.replay_seed.is_some())
    .then_some(1))
    {
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
    crate::planning::resolve_snapshot(&config, options.measurement_dir.is_some(), options.policy)?;
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
    let mut presentation = Ok(());
    for (index, result, buffer) in completed {
        if presentation.is_ok() {
            presentation = output.write_all(&buffer).map_err(io_error);
        }
        if let Err(error) = result {
            add_replica_failure(&mut failures, index + 1, error.to_string());
        }
    }
    let primary = if failures.is_empty() {
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
    };
    diagnostics::preserve_primary(primary, presentation)
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
    crate::benchmark_paths::replica_path(path, index)
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
    let policy = crate::planning::resolve_snapshot(
        &config,
        options.measurement_dir.is_some(),
        options.policy,
    )?;
    let AdmittedAttempt {
        config,
        options,
        project_root,
        project_config,
    } = AdmittedAttempt::admit(config_path, config, options)?;
    let RunOptions {
        json,
        show_output,
        measurement_dir,
        mode,
        cassette,
        respect_record_timings,
        ..
    } = options;
    let tape_request = mode.map(|mode| (mode, cassette));
    let tape_cassette = tape_request.map(|(mode, requested)| {
        (
            mode,
            requested
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| default_cassette(config_path, &config)),
        )
    });
    let _network_project = project::network_project(&project_root);
    let _initial_policy = crate::planning::persist(&project_root, &policy, None, None)?;
    let configured_measurements = config
        .measurements
        .as_ref()
        .filter(|value| value.enabled)
        .map(|value| project::resolve(&project_root, &value.directory));
    let measurement_path = measurement_dir
        .map(std::path::PathBuf::from)
        .or(configured_measurements);
    let measurement_collectors: Vec<_> = policy
        .collectors
        .iter()
        .filter(|collector| collector.selected)
        .map(|collector| collector.id.clone())
        .collect();
    let active_collectors: Vec<_> = policy
        .collectors
        .iter()
        .map(|collector| collector.id.clone())
        .collect();
    let mut measurement_session = measurement_path
        .as_deref()
        .map(|dir| {
            measurements::Session::start_with_resolved_collectors_and_parent(
                &project_root,
                dir,
                &measurement_collectors,
                &active_collectors,
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
    #[cfg(all(test, unix))]
    phases::entry_test_support::configure_measurements(&project_root, measurement_session.as_mut());
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
    let mut bridge = None;
    let attempt = (|| {
        instrument_phase(
            measurement_session.as_mut(),
            LifecyclePhase::ProviderSetup,
            || {
                bridge = providers::start_local_bridge(
                    &config.provider,
                    &project_root,
                    &project_config.tools_dir,
                    tape_mode == Some(tape::Mode::Replay),
                )?;
                #[cfg(all(test, unix))]
                {
                    bridge =
                        phases::entry_test_support::observe_provider(&project_root, bridge.take());
                }
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
                    if !models::ensure_tool_capable(
                        &config.provider,
                        &config.model,
                        bridge_catalog,
                    )? && discovered_tool_support != Some(true)
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
                Ok(())
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
                let workload = workloads::prepare_task(&workloads::PreparationContext {
                    verify_initial_material: options.preparation_gate.is_some()
                        || options.verify_initial_material,
                    budgets: policy.budgets,
                    on_allocation: Some(&allocation),
                    workload: &config.workload,
                    trial: config.trial.as_deref(),
                    trial_set: config.trial_set.as_deref(),
                    workload_directory: &workload_directory,
                    workspace: &workspace,
                    project_root: &project_root,
                    project_config: &project_config,
                })
                .map_err(Error::Run)?;
                #[cfg(all(test, unix))]
                let workload =
                    phases::entry_test_support::observe_workload(&project_root, workload);
                Ok(workload)
            },
        )?;
        // Capture provenance at execution time; report rendering never consults
        // today's adapter registry to label a historical record.
        workload_session
            .invocation()
            .input
            .validate_for_agent(&config.agent)
            .map_err(Error::Input)?;
        let mut effective_budgets = workload_session.effective_budgets();
        if policy.backend == "native" {
            effective_budgets.execution_seconds = Some(
                policy
                    .budgets
                    .execution_seconds
                    .unwrap_or(MAX_AGENT_RUNTIME.as_secs()) as f64,
            );
        }
        let execution_plan = crate::planning::persist(
            &project_root,
            &policy,
            Some(effective_budgets),
            Some(&workload_session.invocation().identity),
        )?;
        let execution_plan_fingerprint = crate::planning::protocol_fingerprint_with_admission(
            &config,
            &policy,
            effective_budgets,
            options.replica.total,
            options.batch.map(repeated::BatchIdentity::admission),
        )?;
        let module = workload_session.module_descriptor();
        let metric_policy = workload_session.metric_policy();
        if !metric_policy.valid()
            || (module.is_none() && metric_policy != workloads::outcome::MetricPolicy::default())
            || module
                .as_ref()
                .is_some_and(|m| !m.valid() || !metric_policy.matches_module(m))
        {
            return Err(Error::Run(
                "module metric policy differs from its declared scoring protocol".into(),
            ));
        }
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
                    prompt_sha256: crate::cassette::digest(
                        workload_session.invocation().input.instructions.as_bytes(),
                    ),
                    material_sha256,
                    protocol_sha256: Some(crate::cassette::digest(
                        &serde_json::to_vec(&serde_json::json!({
                            "contract":"asb-bundled-replay-v1",
                            "agent_input":agents::REPLAY_INPUT_CONTRACT,
                            "tape_matching":tape::REPLAY_MATCHING_CONTRACT,
                            "material_scope":workloads::session::REPLAY_MATERIAL_CONTRACT,
                            "module":module,"metric_policy":metric_policy
                        }))
                        .map_err(|error| Error::Run(error.to_string()))?,
                    )),
                })
            })();
            let ready = identity.and_then(|identity| arrival.ready(identity));
            if let Err(error) = ready {
                return Err(abort_prepared(error, arrival, workload_session.as_mut()));
            }
        }
        let launch = options
            .launch_gate
            .map(|gate| {
                let identity = options
                    .preparation_gate
                    .ok_or_else(|| {
                        Error::Run("launch admission requires verified material".into())
                    })?
                    .confirmed_identity()?;
                batch::LaunchAdmission::new(gate, options.replica, identity)
            })
            .transpose()?;
        let launch = launch
            .as_ref()
            .map(|launch| launch as &dyn workloads::session::LaunchReadiness);
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
        let tape_session = instrument_phase(
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
        let mut prepared = PreparedAttempt {
            workload: workload_session,
            measurements: measurement_session,
            tape: tape_session,
        };
        let _launch_failure = workloads::session::LaunchFailureGuard(launch);
        let tape_url = prepared.tape.as_ref().map(|session| session.url());
        let provider_endpoint = tape_url
            .or_else(|| providers::local_bridge_endpoint(&config.provider, bridge.as_deref()));
        let provider_credentials = providers::local_bridge_credentials(
            &config.provider,
            bridge.as_deref(),
            provider_endpoint,
        );
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
        let isolated = prepared.workload.execute(
            &mut workloads::ExecutionContext {
                launch,
                config: &config,
                connection: connection.as_ref(),
                project_root: &project_root,
                project_config: &project_config,
                replay: tape_mode == Some(tape::Mode::Replay),
                measurements: prepared.measurements.as_mut(),
                show_output,
            },
            output,
        );
        let mut issues = AttemptIssues::default();
        let (child, run_dir, guest, execution_elapsed_ms, runtime_output) =
            if let Some(result) = isolated {
                let result = result.map_err(Error::Run)?;
                issues.collector_errors = result.collector_errors;
                issues.output_error = result.output_error;
                (
                    result.output,
                    PathBuf::new(),
                    Some(result.metadata),
                    result
                        .elapsed_ms
                        .unwrap_or_else(|| started.elapsed().as_millis()),
                    None,
                )
            } else {
                let native = native::NativePlan {
                    config_path,
                    config: &config,
                    project_root: &project_root,
                    project_config: &project_config,
                    invocation: prepared.workload.invocation(),
                    fresh_state: options.batch.is_some(),
                    route: native::NativeRoute {
                        connection: connection.as_ref(),
                        provider_endpoint,
                        provider_credentials,
                        tape_url,
                        tape_mode,
                    },
                }
                .prepare(prepared.measurements.as_mut())?;
                let (evidence, native_issues) =
                    native.execute(prepared.measurements.as_mut(), show_output, output, launch)?;
                issues = native_issues;
                (
                    evidence.output,
                    evidence.run_dir,
                    evidence.guest,
                    evidence.elapsed_ms,
                    evidence.runtime_output,
                )
            };
        let capture_complete = issues.capture_error.is_none();
        let settled = prepared
            .executed(
                ExecutionEvidence {
                    capture_complete,
                    output: child,
                    run_dir,
                    guest,
                    elapsed_ms: execution_elapsed_ms,
                    runtime_output,
                },
                issues,
            )
            .settle(tape_cassette.as_ref());
        let agent_outcome = settled.evidence.agent_outcome(&config.agent, &workspace);
        let process_success = agent_outcome.failure.is_none();
        let agent_failure = agent_outcome.failure;
        let final_response = agent_outcome.final_response;
        // A usable submission can survive a failed client or proxy teardown.
        // Evaluator errors and process failures are recorded independently.
        let FinalizedAttempt {
            workload: workload_session,
            measurements: measurement_session,
            evidence:
                ExecutionEvidence {
                    capture_complete,
                    output: child,
                    run_dir,
                    guest,
                    elapsed_ms,
                    runtime_output,
                },
            proxy_evidence,
            cassette_usage_verified,
            outcome,
            evaluator_evidence,
            cleanup_confirmed: workload_cleanup_confirmed,
            resource_disposition: workload_resource_disposition,
            issues,
        } = settled
            .evaluate_with_policy(
                final_response.as_deref().unwrap_or_default(),
                &metric_policy,
            )
            .finish();
        let workload_artifacts = workload_session.checkpoint_evidence();
        if workload_resource_disposition == Some(workloads::ResourceDisposition::StoppedPreserved)
            && !json
        {
            emit_warning(
                warnings,
                output,
                format!(
                    "Warning: workload resources were stopped and preserved; inspect {}; do not retry the same attempt implicitly",
                    workload_artifacts
                        .iter()
                        .map(|path| path.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )?;
        }
        let provider_cleanup_error = providers::finish_session(&mut bridge)
            .err()
            .map(|error| error.to_string());
        let workload_success = outcome.success();
        let grading_failure = RunFailure::evaluation(&outcome.evaluation);
        let evaluation = outcome.evaluation;
        let collector_error = issues.collector_failure();
        let run_error = issues.run_error;
        let output_error = issues.output_error;
        let configured_kind = workload_session.evaluation_kind();
        let evaluation_kind = workloads::scored_kind(configured_kind, &workload_baseline);
        let resolution_eligible = workloads::resolution_eligible(evaluation_kind);
        let disposition = RunDisposition::from_facts(RunFacts {
            evaluation: &evaluation,
            evaluation_kind,
            process_success,
            agent_failure: agent_failure.as_deref(),
            run_error: run_error.as_deref(),
            collector_error: collector_error.as_deref(),
            output_error: output_error.as_deref(),
            provider_cleanup_error: provider_cleanup_error.as_deref(),
            workload_cleanup_confirmed,
            proxy_cleanup_confirmed: proxy_evidence.cleanup_confirmed,
        });
        let success = disposition.success;
        let workload_failure =
            grading_failure
                .as_ref()
                .map(|failure| workloads::EvaluationFailure {
                    kind: failure.kind.as_str(),
                    reason: failure.reason.clone(),
                });
        let adapter_artifacts = guest
            .is_none()
            .then(|| agents::find(&config.agent))
            .flatten()
            .map(|adapter| {
                adapter.collect(&agents::CollectionContext {
                    workspace: &workspace,
                    run_dir: &run_dir,
                    interface: connection.as_ref().map(|connection| connection.interface),
                    anthropic_input_includes_cache:
                        crate::providers::anthropic_input_includes_cache(&config.provider),
                })
            })
            .unwrap_or_default();
        let token_usage = measurements::token_usage_from_sources(
            &config.provider,
            &config.agent,
            &if capture_complete {
                String::from_utf8_lossy(&child.stdout)
            } else {
                "".into()
            },
            &if capture_complete {
                String::from_utf8_lossy(&child.stderr)
            } else {
                "".into()
            },
            tape_cassette
                .as_ref()
                .filter(|_| cassette_usage_verified)
                .map(|(_, cassette)| cassette.as_path()),
        );
        let token_usage =
            measurements::with_artifact_usage(token_usage, adapter_artifacts.token_usage);
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
                capture_complete
                    .then(|| {
                        measurements::steps_from_sources(
                            &config.agent,
                            &bounded(&child.stdout),
                            &bounded(&child.stderr),
                        )
                    })
                    .flatten()
            })
        };
        let mut stderr = bounded(&child.stderr);
        if let Some(error) = provider_cleanup_error.as_ref() {
            stderr.push_str("\nASB provider cleanup error: ");
            stderr.push_str(error);
            stderr.push('\n');
        }
        if let Some(error) = output_error.as_ref() {
            stderr.push_str("\nASB presentation error: ");
            stderr.push_str(error);
            stderr.push('\n');
        }
        for error in &issues.collector_errors {
            stderr.push_str("\nASB collector error: ");
            stderr.push_str(error);
            stderr.push('\n');
        }
        if let Some(error) = run_error.as_deref() {
            stderr.push_str("\nASB run error: ");
            stderr.push_str(error);
            stderr.push('\n');
        }
        if let Some(failure) = &disposition.failure {
            stderr.push_str("\nASB failure (");
            stderr.push_str(failure.kind.as_str());
            stderr.push_str("): ");
            stderr.push_str(&failure.reason);
            stderr.push('\n');
        }
        let run_metrics = crate::collectors::run_metrics(&crate::collectors::RunObservation {
            elapsed_ms,
            success,
        });
        let mut result = RunResult {
            recording: None,
            execution: options.batch.map(|batch| batch.execution(options.replica)),
            planning: policy,
            effective_budgets,
            execution_plan: Some(execution_plan),
            execution_plan_fingerprint: Some(execution_plan_fingerprint),
            module,
            benchmark_outcome: Some(workloads::outcome::BenchmarkOutcome {
                schema_version: 1,
                execution_success: Some(process_success),
                evaluation,
                cleanup_confirmed: Some(
                    workload_cleanup_confirmed
                        && proxy_evidence.cleanup_confirmed
                        && provider_cleanup_error.is_none(),
                ),
                evidence: evaluator_evidence,
                policy: metric_policy,
                lineage: workloads::outcome::EvaluationLineage {
                    execution_attempt: workload_session.invocation().identity.attempt_id.clone(),
                    evaluation_id: workloads::TaskIdentity::local("evaluation", None).attempt_id,
                    supersedes: None,
                },
            }),
            replica_index: Some(options.replica.index),
            execution_cohort: options.execution_cohort,
            schema_version: 2,
            disposition,
            agent: &config.agent,
            workload: &config.workload,
            task: workload_session.invocation().identity.clone(),
            provider: &config.provider,
            model: &config.model,
            process_success,
            workload_success: resolution_eligible.then_some(workload_success).flatten(),
            workload_resource_disposition,
            workload_artifacts,
            workload_evaluation: evaluation_kind,
            workload_baseline_passed,
            workload_baseline,
            workload_failure,
            success_rate: resolution_eligible
                .then_some(workload_success)
                .flatten()
                .map(|resolved| if resolved { 1.0 } else { 0.0 }),
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
        // A recording failure changes command completion, never the actual
        // process or official workload grade. Commit before sealing outputs.
        let recording_error = options.recording.and_then(|recorder| {
            let batch = options.batch.expect("campaign-owned recording");
            match recorder.commit(batch.number(), options.replica, &result, &proxy_evidence) {
                Ok(evidence) => {
                    result.recording = Some(evidence);
                    None
                }
                Err(error) => Some(error),
            }
        });
        if let Some(error) = &recording_error {
            result.disposition.recording_failed(error.to_string());
        }
        let log_publication =
            phases::LogPublication::publish(config_path, &config.log_file, &result);
        let measurement_completion = phases::finish_measurements(measurement_session, &result);
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
        let mut continuation_warnings = Vec::new();
        if !capture_complete {
            continuation_warnings.push(diagnostics::Warning::new(
                diagnostics::Stage::Outcome,
                config_path,
                &completion::directory(config_path, &config.log_file),
                &Error::Input(
                    "agent output capture is incomplete; continuation evidence cannot be published"
                        .into(),
                ),
            ));
        }
        if log_publication.confirmed()
            && measurement_completion.confirmed()
            && provider_cleanup_error.is_none()
            && recording_error.is_none()
            && capture_complete
        {
            match completion::persist(
                config_path,
                &config.log_file,
                &result,
                workload_cleanup_confirmed,
                &result.workload_artifacts,
                &proxy_evidence,
                &completion::Context {
                    replica: options.replica,
                    outputs: &completion_outputs,
                },
            ) {
                Ok(record) => {
                    if let Some(observer) = options.on_completion
                        && let Err(error) = observer.completed(options.replica, &record)
                    {
                        let destination = observer.evidence_destination(options.replica);
                        let warning = diagnostics::Warning::new(
                            diagnostics::Stage::Replica,
                            config_path,
                            destination.as_deref().unwrap_or(&record),
                            &error,
                        );
                        continuation_warnings.push(if destination.is_some() {
                            warning
                        } else {
                            warning.source_outcome()
                        });
                    }
                }
                Err(error) => continuation_warnings.push(diagnostics::Warning::new(
                    diagnostics::Stage::Outcome,
                    config_path,
                    &completion::directory(config_path, &config.log_file),
                    &error,
                )),
            }
        }
        let presentation = (|| {
            if json {
                render_json(&result, output)?;
            } else {
                render_human(&result, output)?;
            }
            for warning in continuation_warnings {
                warning.emit(json, output, warnings)?;
            }
            Ok(())
        })();
        let outcome = phases::final_outcome(
            result.disposition.failure.clone(),
            log_publication,
            measurement_completion,
        );
        // Output loss is not an ordinary completed benchmark failure: repeated
        // execution must stop even when a separate task failure has higher priority.
        let outcome = match output_error {
            Some(error) => Err(Error::Io(
                outcome.err().map_or(error, |error| error.to_string()),
            )),
            None => outcome,
        };
        let outcome = diagnostics::preserve_primary(outcome, presentation);
        let outcome = match recording_error {
            Some(error) => Err(Error::Io(match outcome {
                Ok(()) => error.to_string(),
                Err(primary) => format!("{primary}; {error}"),
            })),
            None => outcome,
        };
        if let Some(cleanup) = provider_cleanup_error {
            match outcome {
                Ok(()) => Err(Error::Run(cleanup)),
                Err(Error::Io(primary)) => Err(Error::Io(format!(
                    "{primary}; provider cleanup failed: {cleanup}"
                ))),
                Err(primary)
                    if result.disposition.failure.as_ref().map(|f| f.kind)
                        == Some(FailureKind::ProviderCleanupError) =>
                {
                    Err(primary)
                }
                Err(primary) => Err(Error::Run(format!(
                    "{primary}; provider cleanup failed: {cleanup}"
                ))),
            }
        } else {
            outcome
        }
    })();
    finish_provider_attempt(attempt, &mut arrival, &mut bridge)
}

fn finish_provider_attempt(
    attempt: Result<(), Error>,
    arrival: &mut Option<batch::Arrival<'_>>,
    bridge: &mut Option<Box<dyn providers::ProviderSession>>,
) -> Result<(), Error> {
    // Abort unsettled material admission before potentially slow provider
    // teardown. Launch guards inside the attempt have already notified peers.
    if let Err(error) = &attempt
        && let Some(arrival) = arrival.as_mut()
    {
        arrival.abort(&error.to_string());
    }
    providers::finish_result(attempt, bridge)
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
#[cfg(test)]
fn run_measured_agent(
    command: &mut Command,
    target: &crate::launch::TargetLaunch,
    route: Option<crate::launch::CommandRoute>,
    session: Option<&mut measurements::Session>,
    show_output: bool,
    output: &mut dyn Write,
) -> Result<MeasuredOutput, Error> {
    run_measured_agent_with_launch(
        command,
        target,
        route,
        session,
        show_output,
        output,
        NativeLaunch {
            readiness: None,
            runtime: MAX_AGENT_RUNTIME,
        },
    )
}

struct NativeLaunch<'a> {
    readiness: Option<&'a dyn workloads::session::LaunchReadiness>,
    runtime: Duration,
}

fn native_launch_failure(
    child: &mut OwnedChild,
    launch: Option<&dyn workloads::session::LaunchReadiness>,
    primary: impl std::fmt::Display,
) -> Error {
    // Release waiting siblings before potentially slow bounded child cleanup.
    if let Some(launch) = launch {
        launch.abort();
    }
    Error::Run(child.failure(primary))
}

fn run_measured_agent_with_launch(
    command: &mut Command,
    target: &crate::launch::TargetLaunch,
    route: Option<crate::launch::CommandRoute>,
    mut session: Option<&mut measurements::Session>,
    show_output: bool,
    output: &mut dyn Write,
    control: NativeLaunch<'_>,
) -> Result<MeasuredOutput, Error> {
    let NativeLaunch {
        readiness: launch,
        runtime,
    } = control;
    let mut started = Instant::now();
    let gated = route
        .as_ref()
        .is_some_and(crate::launch::CommandRoute::is_gated);
    #[cfg(target_os = "linux")]
    let admission = if let Some(route) = route.filter(|route| route.is_gated()) {
        let validated = route
            .validate_gate(command)
            .map_err(|error| Error::Run(format!("cannot validate ASB gate launch: {error}")))?;
        Some(
            crate::process_gate::Admission::new(validated).map_err(|error| {
                Error::Run(format!(
                    "cannot establish ASB process gate custody: {error}"
                ))
            })?,
        )
    } else {
        None
    };
    if let Some(session) = session.as_deref_mut() {
        session.agent_launch_prepared(target);
    }
    if launch.is_some() && (!gated || !cfg!(target_os = "linux")) {
        return Err(Error::Run(
            "shared launch requires a verifiable stopped native process gate".into(),
        ));
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
        if launch.is_none() {
            emit(LifecycleEvent::AgentExecuteBegin)?;
        }
        #[cfg(target_os = "linux")]
        let spawned = match admission {
            Some(admission) => admission.spawn().map(OwnedChild::Gated),
            None => OwnedChild::spawn_direct(command),
        };
        #[cfg(not(target_os = "linux"))]
        let spawned = OwnedChild::spawn_direct(command);
        match spawned {
            Ok(child) => child,
            Err(error) => {
                let mut primary = format!(
                    "could not start {}: {error}",
                    target.program.to_string_lossy()
                );
                for event in [
                    LifecycleEvent::AgentExecuteEnd,
                    LifecycleEvent::AgentSpawnFailed,
                ] {
                    if let Err(secondary) = emit(event) {
                        primary.push_str(&format!("; {secondary}"));
                    }
                }
                return Err(Error::Run(primary));
            }
        }
    };
    let _launch_failure = workloads::session::LaunchFailureGuard(launch);
    if let Some(session) = session.as_deref_mut()
        && let Err(error) = session.event(LifecycleEvent::AgentStarted)
    {
        return Err(native_launch_failure(&mut child, launch, error));
    }
    #[cfg(unix)]
    if launch.is_some()
        && let Err(error) = child.wait_stopped()
    {
        return Err(native_launch_failure(
            &mut child,
            launch,
            format!("native launch target is not stopped: {error}"),
        ));
    }
    if let Some(session) = session.as_deref_mut()
        && let Err(error) = session.attach_pid(child.id())
    {
        let mut primary = error;
        for event in [
            LifecycleEvent::AgentExecuteEnd,
            LifecycleEvent::AgentAttachFailed,
        ] {
            if let Err(secondary) = session.event(event) {
                primary.push_str(&format!("; {secondary}"));
            }
        }
        return Err(native_launch_failure(&mut child, launch, primary));
    }
    if let Some(launch) = launch {
        if let Err(error) = launch.ready() {
            return Err(native_launch_failure(&mut child, Some(launch), error));
        }
        if let Some(session) = session.as_deref_mut()
            && let Err(error) = session.event(LifecycleEvent::AgentExecuteBegin)
        {
            return Err(native_launch_failure(&mut child, Some(launch), error));
        }
        started = Instant::now();
    }
    #[cfg(unix)]
    if gated && let Err(error) = child.resume() {
        return Err(native_launch_failure(
            &mut child,
            launch,
            format!("could not resume measured agent process: {error}"),
        ));
    }
    let mut execution_finished = None;
    let result = capture_with_output_streams(
        child,
        show_output,
        output,
        None,
        runtime,
        PresentationPolicy::PreserveEvidence,
        &mut || {
            execution_finished = Some(Instant::now());
            Ok(())
        },
    );
    // Freeze execution before collector/proxy teardown, even if that teardown
    // fails. A completed client output is evidence for official grading.
    let elapsed_ms = execution_finished
        .unwrap_or_else(Instant::now)
        .duration_since(started)
        .as_millis();
    let mut issues = AttemptIssues::default();
    if let Some(session) = session {
        issues.event(Some(session), LifecycleEvent::AgentExecuteEnd);
        issues.event(
            Some(session),
            if result.is_ok() {
                LifecycleEvent::AgentExited
            } else {
                LifecycleEvent::AgentWaitFailed
            },
        );
    }
    match result {
        Ok(captured) => {
            issues.output_error = captured.output_error;
            if let Some(error) = captured.capture_error {
                issues.capture_error = Some(error.clone());
                issues.run_error(error);
            }
            Ok(MeasuredOutput {
                output: captured.output,
                elapsed_ms,
                issues,
            })
        }
        Err(error) => {
            let mut primary = format!(
                "could not wait for {}: {error}",
                target.program.to_string_lossy()
            );
            if let Some(secondary) = issues.collector_failure() {
                primary.push_str(&format!("; {secondary}"));
            }
            Err(Error::Run(primary))
        }
    }
}

#[cfg(unix)]
fn gate_target_available(program: impl AsRef<std::ffi::OsStr>) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let program = program.as_ref();
    let path = Path::new(program);
    if !path.is_file() {
        return !program.as_bytes().contains(&b'/');
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
            component(&config.agent),
            component(&config.workload),
            component(&config.model)
        ))
}

/// Explicit paths win. Preserve historical JSONL/base-or-replica routing when
/// present; otherwise discover the new default ZIP recording. Presence includes
/// invalid files/links, which must fail normally rather than silently fall back.
fn replay_cassette(
    config_path: &Path,
    config: &BenchmarkConfig,
    requested: Option<&str>,
) -> Result<PathBuf, Error> {
    if let Some(path) = requested {
        return Ok(PathBuf::from(path));
    }
    let legacy = default_cassette(config_path, config);
    for path in std::iter::once(legacy.clone()).chain(
        (1..=crate::benchmark::MAX_CONCURRENCY)
            .map(|replica| crate::benchmark_paths::replica_path(&legacy, replica)),
    ) {
        match std::fs::symlink_metadata(&path) {
            Ok(_) => return Ok(legacy),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(io_error(error)),
        }
    }
    let bundle = default_bundle_cassette(config_path, config);
    match std::fs::symlink_metadata(&bundle) {
        Ok(_) => Ok(bundle),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(legacy),
        Err(error) => Err(io_error(error)),
    }
}

fn default_bundle_cassette(config_path: &Path, config: &BenchmarkConfig) -> PathBuf {
    default_cassette(config_path, config).with_extension("asb.zip")
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
    let mut summary = serde_json::json!({
        "execution": result.execution,
        "planning": result.planning,
        "effective_budgets": result.effective_budgets,
        "execution_plan": result.execution_plan,
        "execution_plan_fingerprint": result.execution_plan_fingerprint,
        "module": result.module,
        "benchmark_outcome": result.benchmark_outcome,
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
        "workload_resource_disposition": result.workload_resource_disposition,
        "workload_artifacts": result.workload_artifacts,
        "workload_evaluation": result.workload_evaluation,
        "workload_baseline_passed": result.workload_baseline_passed,
        "workload_baseline": result.workload_baseline,
        "workload_failure": result.workload_failure,
        "adapter_smoke_success": result.adapter_smoke_success,
        "failure_kind": result.disposition.failure.as_ref().map(|f| f.kind),
        "failure_reason": result.disposition.failure.as_ref().map(|f| &f.reason),
        "success_rate": result.success_rate,
        "elapsed_ms": result.elapsed_ms,
        "steps": result.steps,
        "token_usage": result.token_usage,
    });
    if let Some(recording) = &result.recording {
        summary["recording"] =
            serde_json::to_value(recording).map_err(|error| Error::Io(error.to_string()))?;
    }
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

// Direct and gated children retain explicit custody on every return path.
#[cfg(test)]
pub(crate) fn fixture_module_descriptor(
    name: &str,
    descriptor: Option<workloads::contracts::ModuleDescriptor>,
) -> Option<workloads::contracts::ModuleDescriptor> {
    phases::entry_test_support::module_descriptor(name, descriptor)
}

mod capture;
#[cfg(unix)]
pub(crate) use capture::transport::GuestTransport;
use capture::{OwnedChild, PresentationPolicy, capture_with_output_streams};

#[cfg(test)]
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
    let child = OwnedChild::spawn_direct(command)
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
    child: OwnedChild,
    show_output: bool,
    output: &mut dyn Write,
    error_output: Option<&mut dyn Write>,
    runtime: Duration,
    after_exit: &mut dyn FnMut() -> std::io::Result<()>,
) -> std::io::Result<Output> {
    let captured = capture_with_output_streams(
        child,
        show_output,
        output,
        error_output,
        runtime,
        PresentationPolicy::CancelOnTransportFailure,
        after_exit,
    )?;
    if let Some(error) = captured.capture_error {
        return Err(std::io::Error::other(error));
    }
    Ok(captured.output)
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
        RunDisposition, RunResult, RunStatus, command_for, render_human, render_json,
        replica_cassette, resolve_log_path, run_with_tape, write_log,
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
        let target = crate::launch::TargetLaunch::direct(command);
        let _ = super::run_measured_agent(
            command,
            &target,
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
        let target = crate::launch::TargetLaunch::direct(&command);
        let output = super::run_measured_agent(
            &mut command,
            &target,
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
                workload_resource_disposition: None,
                workload_artifacts: &[],
                execution_plan_fingerprint: None,
                module: None,
                benchmark_outcome: None,
                replica_index: None,
                execution_cohort: None,
                schema_version: 1,
                run_id: "test".into(),
                status: if fail {
                    RunStatus::Failed
                } else {
                    RunStatus::Completed
                },
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
        let child =
            super::OwnedChild::spawn_direct(std::process::Command::new("sleep").arg("10")).unwrap();
        let pid = child.id();
        let owned = child;
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
        let child = super::OwnedChild::spawn_direct(
            std::process::Command::new("sh")
                .args(["-c", "printf ready; exec sleep 10"])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped()),
        )
        .unwrap();
        let pid = child.id();
        let error = super::wait_with_output(child, true, &mut FailingWriter).unwrap_err();
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
        std::fs::create_dir(root.join(".asb")).unwrap();
        std::fs::write(
            root.join(".asb/project.toml"),
            toml::to_string(&crate::project::ProjectConfig::default()).unwrap(),
        )
        .unwrap();
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
        assert_eq!(
            std::fs::read_dir(root.join(".asb/execution-plans"))
                .unwrap()
                .count(),
            8,
            "each admitted replica must keep its policy in the isolated fixture project"
        );
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
                true,
                &evidence,
                &crate::proxy::Evidence {
                    captures: Vec::new(),
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
            assert_eq!(record["workload_cleanup_confirmed"], true);
            assert_eq!(record["workload_resource_disposition"], "not-required");
            assert_eq!(
                record["workload_resource_disposition"],
                record["summary"]["workload_resource_disposition"]
            );
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
                        captures: Vec::new(),
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
            recording: None,
            execution: None,
            planning: crate::planning::resolve(
                &toml::from_str(
                    "agent='pi'\nprovider='fixture'\nmodel='fixture'\nworkload='bug-fix'",
                )
                .unwrap(),
            )
            .unwrap(),
            effective_budgets: Default::default(),
            execution_plan: None,
            execution_plan_fingerprint: None,
            module: None,
            benchmark_outcome: None,
            replica_index: None,
            execution_cohort: None,
            schema_version: 2,
            disposition: RunDisposition {
                status: RunStatus::Completed,
                failure: None,
                success: true,
            },
            agent: "codex",
            workload: "bug-fix",
            task: crate::workloads::TaskIdentity::local("bug-fix", None),
            provider: "openrouter",
            model: "test-model",
            process_success: true,
            workload_success: Some(true),
            workload_resource_disposition: Some(crate::workloads::ResourceDisposition::NotRequired),
            workload_artifacts: Vec::new(),
            workload_evaluation: "bundled-fixture",
            workload_baseline_passed: Some(false),
            workload_baseline: crate::workloads::BaselineEvidence::Measured {
                passed: Some(false),
            },
            workload_failure: None,
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
            planning: None,
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
            planning: None,
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
                planning: None,
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
            planning: None,
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
            planning: None,
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
            planning: None,
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
