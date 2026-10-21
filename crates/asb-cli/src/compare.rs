//! Offline HTML/SVG reports for benchmark configurations and sweeps.

use crate::{Error, read_config};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const MAX_BYTES: u64 = 16 * 1024 * 1024;
mod coverage;
mod metric_identity;
use metric_identity::{GraphNames, flatten_numbers, insert_unique, path_key};
pub(crate) mod outcomes;
mod reconciliation;
const PLOT_TOP: f64 = 70.0;
const PLOT_BOTTOM: f64 = 390.0;
const PLOT_RIGHT: f64 = 880.0;
const Y_TICKS: usize = 5;

#[derive(Debug)]
struct BenchmarkData {
    legacy_evaluation_provenance: Option<crate::workloads::outcome::LegacyEvaluationProvenance>,
    execution_plan_fingerprint: Option<String>,
    module: Option<crate::workloads::contracts::ModuleDescriptor>,
    benchmark_outcome: Option<crate::workloads::outcome::BenchmarkOutcome>,
    result_source: &'static str,
    reconciled: bool,
    replica_index: Option<usize>,
    execution_cohort: Option<crate::measurements::ExecutionCohort>,
    label: String,
    directory: PathBuf,
    agent: String,
    workload: String,
    task: Option<crate::workloads::TaskIdentity>,
    selection: Option<crate::workloads::TaskSelection>,
    evaluation_completed: bool,
    workload_evaluation: String,
    provider: String,
    model: String,
    metrics: BTreeMap<String, f64>,
    series: BTreeMap<String, TimeSeries>,
    host_metadata: Option<Value>,
    components: Option<Value>,
    status: Option<String>,
    artifacts: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct TimeSeries {
    values: Vec<f64>,
    timestamps_ms: Option<Vec<f64>>,
}

/// Report a benchmark's recorded results or compare the members of a sweep.
pub fn run(path: &str, output: &mut dyn Write) -> Result<(), Error> {
    generate(path, output).map(|_| ())
}

/// Generate an offline report for either a benchmark or a sweep manifest.
pub(crate) fn generate(path: &str, output: &mut dyn Write) -> Result<PathBuf, Error> {
    let sweep_path = Path::new(path);
    let input: toml::Value = toml::from_str(&crate::sweep::read_manifest_text(sweep_path)?)
        .map_err(|error| Error::Input(format!("cannot parse {path}: {error}")))?;
    let single = !input
        .as_table()
        .is_some_and(|table| table.contains_key("configs"));
    let sweep = if single {
        crate::sweep::SweepFile {
            configs: vec![
                sweep_path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            ],
        }
    } else {
        crate::sweep::read_manifest(sweep_path)?
    };
    let parent = sweep_path.parent().unwrap_or_else(|| Path::new("."));
    let stem = sweep_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("sweep");
    let sweep_results = parent.join(format!("{stem}-results"));
    let plan = if single {
        None
    } else {
        crate::sweep::plan::SweepPlan::load(&sweep_results, parent, &sweep)?
    };
    let comparison_dir = parent.join("results").join(format!("{stem}-comparison"));
    fs::create_dir_all(&comparison_dir).map_err(|error| {
        Error::Run(format!(
            "cannot create comparison directory {}: {error}",
            comparison_dir.display()
        ))
    })?;

    let mut benchmark_data = Vec::new();
    let mut actual_attempts = BTreeSet::new();
    for (index, reference) in sweep.configs.iter().enumerate() {
        let config = checked_reference(parent, reference, path)?;
        let mut result_dir = sweep_results.join(format!("{index:03}-{}", safe_name(&config)));
        let (agent, workload, provider, model, concurrency) = benchmark_identity(&config)?;
        let standalone = if single {
            Some(
                toml::from_str::<crate::benchmark::BenchmarkConfig>(&read_config(&config)?)
                    .map_err(|error| {
                        Error::Input(format!(
                            "cannot parse benchmark {}: {error}",
                            config.display()
                        ))
                    })?,
            )
        } else {
            None
        };
        if let Some(configuration) = &standalone {
            result_dir = crate::runner::resolve_log_path(&config, &configuration.log_file)
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf();
        }
        let selection = plan
            .as_ref()
            .map(|plan| plan.entries[index].selection.clone());
        let mut outcomes = if let Some(configuration) = &standalone {
            outcomes::load_run(&config, configuration)?
        } else {
            outcomes::load(&result_dir, concurrency)?
        };
        if let Some(plan) = &plan {
            let mut imports = crate::sweep::recovered::load(
                &sweep_results.join(".checkpoint"),
                &result_dir,
                index,
                &plan.entries[index],
            )?;
            let closed = crate::sweep::closed_slot_reports(
                &sweep_results.join(".checkpoint"),
                &result_dir,
                index,
                &plan.entries[index],
            )?;
            for (slot, (_, value)) in &closed {
                if let Some((_, imported)) = imports.get(slot) {
                    if imported["task"] != value["task"]
                        || imported["workload_success"] != value["workload_success"]
                    {
                        return Err(Error::Input(
                            "closed slot conflicts with its imported grade; preserve both records"
                                .into(),
                        ));
                    }
                    imports.remove(slot);
                }
            }
            outcomes::add_imports(&mut outcomes, imports)?;
            outcomes::add_closed(&mut outcomes, closed)?;
        } else if !single
            && (result_dir.join(".asb-imported-grades").exists()
                || sweep_results.join(".checkpoint/closed-slots").exists())
        {
            return Err(Error::Input(
                "retained grades require their original saved sweep plan".into(),
            ));
        }
        for replica in 1..=concurrency {
            let label = if concurrency == 1 {
                format!("{index:03}-{}", safe_name(&config))
            } else {
                format!("{index:03}-{}-concurrency-{replica}", safe_name(&config))
            };
            let measurement_dir = if let Some(configuration) = &standalone {
                let selected = crate::runner::replica::configuration(
                    configuration,
                    crate::runner::replica::Identity {
                        index: replica,
                        total: concurrency,
                    },
                )?;
                let (root, _) = crate::project::load(&config)?;
                selected
                    .measurements
                    .as_ref()
                    .filter(|value| value.enabled)
                    .map(|value| crate::project::resolve(&root, &value.directory))
            } else {
                Some(result_dir.join(if concurrency == 1 {
                    "measurements".to_owned()
                } else {
                    format!("measurements-concurrency-{replica}")
                }))
            };
            let measurement_dir = if single {
                outcomes
                    .get(&replica)
                    .and_then(|record| record.summary.get("measurement_directory"))
                    .and_then(Value::as_str)
                    .map(PathBuf::from)
                    .or(measurement_dir)
            } else {
                measurement_dir
            };
            let capture_enabled = measurement_dir.is_some();
            let measurement_dir =
                measurement_dir.unwrap_or_else(|| result_dir.join(".disabled-measurements"));
            let mut run_path = measurement_dir.join("run.json");
            let artifacts = if capture_enabled {
                artifact_links(parent, &measurement_dir)
            } else {
                Vec::new()
            };
            let outcome = outcomes.get(&replica);
            let grade_only_closed =
                outcome.is_some_and(|record| record.source == "recovered-official-grade");
            let measured = capture_enabled && run_path.is_file() && !grade_only_closed;
            if !measured && outcome.is_none() {
                let workload_evaluation = selection.as_ref().map_or_else(
                    || crate::workloads::evaluation_kind(&workload, Path::new("")).into(),
                    |selection| selection.evaluation_kind.clone(),
                );
                benchmark_data.push(BenchmarkData {
                    legacy_evaluation_provenance: None,
                    execution_plan_fingerprint: None,
                    module: selection.as_ref().and_then(|s| s.module.clone()),
                    benchmark_outcome: None,
                    result_source: "missing",
                    reconciled: false,
                    replica_index: None,
                    execution_cohort: None,
                    label,
                    directory: result_dir.clone(),
                    agent: agent.clone(),
                    workload: workload.clone(),
                    task: None,
                    selection: selection.clone(),
                    evaluation_completed: false,
                    workload_evaluation,
                    provider: provider.clone(),
                    model: model.clone(),
                    metrics: BTreeMap::new(),
                    series: BTreeMap::new(),
                    host_metadata: None,
                    components: None,
                    status: Some(format!(
                        "missing measurements: expected {} (measurement collection may be disabled, or the run stopped before writing results)",
                        run_path.display()
                    )),
                    artifacts,
                });
                continue;
            }
            let run_value = if measured {
                let measured = read_json(&run_path)?;
                if let Some(outcome) = outcome {
                    outcome.check_measurement(&measured)?;
                }
                measured
            } else {
                let outcome = outcome.expect("a recorded outcome is required without measurements");
                run_path = outcome.path.clone();
                outcome.summary.clone()
            };
            if single || plan.is_some() || outcome.is_some() {
                for (field, expected) in [
                    ("agent", &agent),
                    ("workload", &workload),
                    ("provider", &provider),
                    ("model", &model),
                ] {
                    if run_value.get(field).and_then(Value::as_str) != Some(expected.as_str()) {
                        return Err(Error::Input(format!(
                            "result {} does not match selected `{field}`; inspect the attempt instead of mixing results from another configuration",
                            run_path.display()
                        )));
                    }
                }
            }
            let execution_cohort = run_value
                .get("execution_cohort")
                .filter(|value| !value.is_null())
                .map(|value| {
                    serde_json::from_value::<crate::measurements::ExecutionCohort>(value.clone())
                })
                .transpose()
                .map_err(|error| {
                    Error::Input(format!(
                        "invalid execution cohort in {}: {error}",
                        run_path.display()
                    ))
                })?;
            let replica_index = run_value
                .get("replica_index")
                .filter(|value| !value.is_null())
                .map(|value| {
                    value
                        .as_u64()
                        .and_then(|value| usize::try_from(value).ok())
                        .ok_or_else(|| {
                            Error::Input(format!("invalid replica index in {}", run_path.display()))
                        })
                })
                .transpose()?;
            if execution_cohort.is_some() != replica_index.is_some()
                || execution_cohort.as_ref().is_some_and(|cohort| {
                    replica_index != Some(replica) || !cohort.admits(replica, concurrency)
                })
            {
                return Err(Error::Input(format!(
                    "execution cohort does not match replica selection in {}",
                    run_path.display()
                )));
            }
            let task = run_value
                .get("task")
                .filter(|value| !value.is_null())
                .map(|value| {
                    serde_json::from_value::<crate::workloads::TaskIdentity>(value.clone())
                })
                .transpose()
                .map_err(|error| {
                    Error::Input(format!(
                        "invalid task identity in {}: {error}",
                        run_path.display()
                    ))
                })?;
            if let Some(configuration) = &standalone
                && task.as_ref().is_some_and(|task| {
                    task.workload != workload
                        || configuration
                            .trial
                            .as_ref()
                            .is_some_and(|trial| trial != &task.task_id)
                })
            {
                return Err(Error::Input(format!(
                    "result {} does not match the benchmark's selected task",
                    run_path.display()
                )));
            }
            if let (Some(selection), Some(task)) = (&selection, &task)
                && !selection.matches(task)
            {
                return Err(Error::Input(format!(
                    "measurement {} does not match the planned dataset instance",
                    run_path.display()
                )));
            }
            if (single || plan.is_some() || outcome.is_some())
                && let Some(task) = &task
                && (task.attempt_id.is_empty()
                    || task.attempt_id.len() > 256
                    || task.workload != workload
                    || !actual_attempts.insert(task.attempt_id.clone()))
            {
                return Err(Error::Input(format!(
                    "result {} has a mismatched, missing, oversized or duplicate actual attempt identity; an attempt cannot be counted twice",
                    run_path.display()
                )));
            }
            let identity_bound = selection
                .as_ref()
                .is_none_or(|selection| task.as_ref().is_some_and(|task| selection.matches(task)));
            let run_status = if !measured {
                outcome.map(outcomes::Recorded::status)
            } else {
                match run_value.get("status").and_then(Value::as_str) {
                Some(status)
                    if run_value.get("workload_evaluation").and_then(Value::as_str)
                        == Some("adapter-smoke") =>
                {
                    Some(format!(
                        "{status}: adapter smoke only; upstream task resolution not measured"
                    ))
                }
                Some("unscored") => Some(
                    "unscored: initial task was already passing or its baseline could not be verified".into()
                ),
                Some(status) if run_value.get("success") == Some(&Value::Bool(false))
                    && !(run_value["benchmark_outcome"]["evaluation"]["status"] == "completed"
                        && run_value["benchmark_outcome"]["evaluation"]["resolution"].is_null()
                        && run_value["process_success"] == true) => Some(
                    if let Some(reason) = run_value.get("failure_reason").and_then(Value::as_str) {
                        format!("benchmark run failed: {reason}")
                    } else {
                        format!(
                            "benchmark run failed (status={status}, exit_code={})",
                            run_value
                                .get("exit_code")
                                .map(Value::to_string)
                                .unwrap_or_else(|| "null".into())
                        )
                    },
                ),
                Some(status) => Some(status.to_owned()),
                None => Some("unknown".into()),
            }
            };
            let workload_evaluation = run_value
                .get("workload_evaluation")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_owned();
            let workload_evaluation = crate::workloads::scored_kind(
                &workload_evaluation,
                &crate::workloads::BaselineEvidence::from_record(&run_value),
            )
            .to_owned();
            if let Some(selection) = selection
                .as_ref()
                .filter(|selection| selection.dataset.is_some())
                && crate::workloads::resolution_eligible(&workload_evaluation)
                && workload_evaluation != selection.evaluation_kind
            {
                return Err(Error::Input(format!(
                    "measurement {} does not match the planned evaluator; a different task grader cannot earn this benchmark's score",
                    run_path.display()
                )));
            }
            let mut metrics = BTreeMap::new();
            let legacy_evaluation_provenance: Option<
                crate::workloads::outcome::LegacyEvaluationProvenance,
            > = run_value
                .get("legacy_evaluation_provenance")
                .filter(|value| !value.is_null())
                .map(|v| serde_json::from_value(v.clone()))
                .transpose()
                .map_err(|_| Error::Input("invalid retained grade provenance".into()))?;
            if let Some(provenance) = &legacy_evaluation_provenance
                && (provenance.execution_attempt.as_deref()
                    != task.as_ref().map(|t| t.attempt_id.as_str())
                    || provenance.evaluation_id.is_some()
                    || provenance.supersedes.is_some()
                    || provenance
                        .evidence_manifest_sha256
                        .as_ref()
                        .is_some_and(|hash| {
                            hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit())
                        }))
            {
                return Err(Error::Input("retained grade provenance differs from original attempt or invents evaluation identity".into()));
            }
            let execution_plan_fingerprint: Option<String> = run_value
                .get("execution_plan_fingerprint")
                .filter(|v| !v.is_null())
                .map(|v| serde_json::from_value(v.clone()))
                .transpose()
                .map_err(|_| {
                    Error::Input("malformed recorded execution plan fingerprint".into())
                })?;
            if execution_plan_fingerprint.as_ref().is_some_and(|v| {
                v.is_empty()
                    || v.len() > 256
                    || !v
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b":._-".contains(&b))
            }) {
                return Err(Error::Input(
                    "invalid recorded execution plan fingerprint".into(),
                ));
            }
            let module: Option<crate::workloads::contracts::ModuleDescriptor> = run_value
                .get("module")
                .filter(|v| !v.is_null())
                .map(|value| serde_json::from_value(value.clone()))
                .transpose()
                .map_err(|e| Error::Input(format!("invalid recorded module descriptor: {e}")))?;
            if module.as_ref().is_some_and(|m| {
                !m.valid()
                    || m.id != workload
                    || task.as_ref().is_some_and(|t| m.dataset != t.dataset)
            }) {
                return Err(Error::Input(
                    "recorded module differs from task identity".into(),
                ));
            }
            if let (Some(recorded), Some(planned)) =
                (&module, selection.as_ref().and_then(|s| s.module.as_ref()))
                && recorded != planned
            {
                return Err(Error::Input(
                    "recorded module differs from saved protocol selection".into(),
                ));
            }
            let module = module.or_else(|| selection.as_ref().and_then(|s| s.module.clone()));
            let benchmark_outcome = crate::workloads::outcome::BenchmarkOutcome::read(&run_value)
                .map_err(Error::Input)?;
            if benchmark_outcome.is_some() && task.is_none() {
                return Err(Error::Input(
                    "structured benchmark outcome has no execution task identity".into(),
                ));
            }
            if module.is_none()
                && benchmark_outcome
                    .as_ref()
                    .is_some_and(|o| o.policy != crate::workloads::outcome::MetricPolicy::default())
            {
                return Err(Error::Input(
                    "custom metric policy has no recorded module provenance".into(),
                ));
            }
            if let (Some(module), Some(outcome)) = (&module, &benchmark_outcome)
                && !outcome.policy.matches_module(module)
            {
                return Err(Error::Input(
                    "recorded metric policy differs from recorded module protocol".into(),
                ));
            }
            if let (Some(outcome), Some(task)) = (&benchmark_outcome, &task)
                && outcome.lineage.execution_attempt != task.attempt_id
            {
                return Err(Error::Input(
                    "evaluation lineage differs from execution attempt".into(),
                ));
            }
            if let Some(outcome) = &benchmark_outcome
                && (outcome.execution_success != run_value["process_success"].as_bool()
                    || (crate::workloads::resolution_eligible(&workload_evaluation)
                        && outcome.evaluation.resolution()
                            != run_value["workload_success"].as_bool()))
            {
                return Err(Error::Input(
                    "structured outcome conflicts with legacy execution or resolution".into(),
                ));
            }
            let evaluation_completed = identity_bound
                && crate::workloads::resolution_eligible(&workload_evaluation)
                && benchmark_outcome.as_ref().map_or_else(
                    || {
                        run_value
                            .get("workload_success")
                            .and_then(Value::as_bool)
                            .is_some()
                    },
                    |outcome| outcome.evaluation.completed(),
                );
            flatten_numbers("", &run_value, &mut metrics)?;
            metrics.retain(|key, _| !key.starts_with("benchmark_outcome."));
            if let Some(outcome) = &benchmark_outcome
                && crate::workloads::resolution_eligible(&workload_evaluation)
                && let crate::workloads::outcome::EvaluationOutcome::Completed { rewards, .. } =
                    &outcome.evaluation
            {
                for (name, value) in rewards {
                    let unit = &outcome.policy.rewards[name].unit;
                    metrics.insert(
                        format!(
                            "benchmark_rewards.{}@{}.{}.{}",
                            path_key(&outcome.policy.id),
                            outcome.policy.version,
                            path_key(name),
                            path_key(unit)
                        ),
                        *value,
                    );
                }
            }
            // Correct old presentations without rewriting their raw evidence.
            if !crate::workloads::resolution_eligible(&workload_evaluation)
                || benchmark_outcome
                    .as_ref()
                    .is_some_and(|o| o.evaluation.resolution().is_none())
            {
                metrics.remove("success_rate");
            } else {
                let resolved = evaluation_completed
                    && run_value.get("workload_success").and_then(Value::as_bool) == Some(true);
                metrics.insert("success_rate".into(), if resolved { 1.0 } else { 0.0 });
            }
            if run_value
                .get("schema_version")
                .and_then(Value::as_u64)
                .unwrap_or(1)
                < 2
            {
                metrics.remove("steps");
            }
            let mut series = BTreeMap::new();
            let mut host_metadata = None;
            let mut components = None;
            let collector_files = if measured {
                Some(
                    fs::read_dir(&measurement_dir)
                        .map_err(|error| Error::Input(error.to_string()))?,
                )
            } else {
                None
            };
            for entry in collector_files.into_iter().flatten() {
                let file = entry
                    .map_err(|error| Error::Input(error.to_string()))?
                    .path();
                if file.extension().and_then(|value| value.to_str()) != Some("json")
                    || file.file_name().and_then(|value| value.to_str()) == Some("run.json")
                {
                    continue;
                }
                let collector = read_json(&file)?;
                let name = file
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or("collector");
                if name == "system" {
                    host_metadata = Some(collector.clone());
                    flatten_numbers("host", &collector, &mut metrics)?;
                }
                if name == "components" {
                    components = Some(collector.clone());
                    if let Some(component_values) = collector.get("components") {
                        flatten_numbers("component", component_values, &mut metrics)?;
                    }
                }
                if let Some(collector_metrics) = collector.get("metrics") {
                    flatten_numbers(&path_key(name), collector_metrics, &mut metrics)?;
                }
                // Collector stdout is retained for diagnosis, but is not a
                // numerical series: scraping its last token created noisy and
                // misleading graphs for repeated mpstat/iostat samples.
                if let Some(collector_series) = collector.get("series") {
                    let collector_timestamps = collector.get("series_timestamps");
                    let timestamp_map = collector_timestamps.and_then(Value::as_object);
                    if let Some(series_map) = collector_series.as_object() {
                        for (metric, raw_values) in series_map {
                            let points = series_values(raw_values);
                            if points.len() < 2 {
                                continue;
                            }
                            insert_unique(
                                &mut series,
                                metric.clone(),
                                TimeSeries {
                                    values: points,
                                    timestamps_ms: timestamp_map
                                        .and_then(|map| map.get(metric))
                                        .map(series_values),
                                },
                            )?;
                        }
                    } else {
                        let points = series_values(collector_series);
                        if points.len() >= 2 {
                            insert_unique(
                                &mut series,
                                name.to_owned(),
                                TimeSeries {
                                    values: points,
                                    timestamps_ms: None,
                                },
                            )?;
                        }
                    }
                }
            }
            benchmark_data.push(BenchmarkData {
                legacy_evaluation_provenance,
                execution_plan_fingerprint,
                module,
                benchmark_outcome,
                result_source: if measured {
                    "measurements"
                } else {
                    outcome.expect("recorded outcome").source
                },
                reconciled: if grade_only_closed {
                    true
                } else if single
                    || outcome.is_some_and(|outcome| outcome.source == "imported-official-grade")
                {
                    false
                } else {
                    reconciliation::recorded(
                        &sweep_results,
                        &result_dir,
                        index,
                        replica,
                        concurrency,
                        task.as_ref(),
                    )?
                },
                replica_index,
                execution_cohort,
                label,
                directory: result_dir.clone(),
                agent: agent.clone(),
                workload: workload.clone(),
                task,
                selection: selection.clone(),
                evaluation_completed,
                workload_evaluation,
                provider: provider.clone(),
                model: model.clone(),
                metrics,
                series,
                host_metadata,
                components,
                status: run_status,
                artifacts,
            });
        }
    }

    let mut cohorts = BTreeMap::new();
    for benchmark in &benchmark_data {
        if let Some(cohort) = &benchmark.execution_cohort
            && let Some(previous) = cohorts.insert(&cohort.id, cohort)
            && previous != cohort
        {
            return Err(Error::Input(
                "conflicting execution cohort declarations in sweep measurements".into(),
            ));
        }
    }
    let mut metric_values: BTreeMap<String, Vec<(String, f64)>> = BTreeMap::new();
    let mut series_values: BTreeMap<String, Vec<(String, TimeSeries)>> = BTreeMap::new();
    for benchmark in &benchmark_data {
        for (name, value) in &benchmark.metrics {
            metric_values
                .entry(name.clone())
                .or_default()
                .push((benchmark.label.clone(), *value));
        }
        for (name, values) in &benchmark.series {
            series_values
                .entry(name.clone())
                .or_default()
                .push((benchmark.label.clone(), values.clone()));
        }
    }
    let mut graphs = Vec::new();
    let mut graph_names = GraphNames::default();
    let agent_labels: BTreeMap<_, _> = benchmark_data
        .iter()
        .map(|benchmark| (benchmark.label.clone(), benchmark.agent.clone()))
        .collect();
    for (name, values) in &metric_values {
        let filename = graph_names.allocate("metric", name);
        write_bar_svg(&comparison_dir.join(&filename), name, values, &agent_labels)?;
        graphs.push(filename);
    }
    for (name, values) in &series_values {
        let filename = graph_names.allocate("series", name);
        write_line_svg(&comparison_dir.join(&filename), name, values)?;
        graphs.push(filename);
    }
    if write_execution_timeline_svg(
        &comparison_dir.join("execution-timeline.svg"),
        &benchmark_data,
    )? {
        graphs.push("execution-timeline.svg".into());
    }
    let mut resolution_groups: BTreeMap<String, crate::workloads::ResolutionSummary> =
        BTreeMap::new();
    for benchmark in &benchmark_data {
        if benchmark.benchmark_outcome.as_ref().is_some_and(|outcome| {
            outcome.policy != crate::workloads::outcome::MetricPolicy::default()
                || (outcome.evaluation.completed() && outcome.evaluation.resolution().is_none())
        }) {
            continue;
        }
        for fingerprint in coverage::fingerprints(benchmark, &benchmark_data)? {
            let key = serde_json::to_string(&(
                &benchmark.agent,
                &benchmark.provider,
                &benchmark.model,
                &benchmark.workload,
                coverage::group_kind(benchmark),
                &benchmark.module,
                &fingerprint,
                benchmark
                    .task
                    .as_ref()
                    .and_then(|task| task.dataset.as_ref())
                    .or_else(|| {
                        benchmark
                            .selection
                            .as_ref()
                            .and_then(|selection| selection.dataset.as_ref())
                    }),
            ))
            .map_err(|error| Error::Run(error.to_string()))?;
            resolution_groups.entry(key).or_default().observe(
                coverage::group_kind(benchmark),
                benchmark
                    .metrics
                    .get("success_rate")
                    .is_some_and(|value| *value == 1.0),
            );
        }
    }
    let evaluation_coverage = coverage::evaluation_summary(&benchmark_data);
    let population_coverage = coverage::populations(&benchmark_data)?;
    let summary = json!({
        "schema_version": 2,
        "success_rate_definition": "resolved task attempts / all selected task attempts, including failures; not process exit rate or pass@k; smoke checks excluded",
        "resolution_groups": resolution_groups,
        "selection_plan": plan.as_ref().map(|plan| json!({"run_id":plan.run_id,"configuration_match":true,"execution_mode":plan.execution_mode,"respect_record_timings":plan.respect_record_timings,"scheduling":plan.scheduling})),
        "evaluation_coverage":evaluation_coverage,
        "population_coverage":population_coverage,
        "metric_groups":coverage::metric_groups(&benchmark_data)?,
        "input": sweep_path,
        "input_kind": if single { "benchmark" } else { "sweep" },
        "sweep": if single { Value::Null } else { json!(sweep_path) },
        "results_directory": if single { json!(parent) } else { json!(sweep_results) },
        "comparison_directory": comparison_dir,
        "report": "report.html",
        "benchmarks": benchmark_data.iter().map(|benchmark| json!({
            "name": benchmark.label,
            "module": benchmark.module,
            "legacy_evaluation_provenance": benchmark.legacy_evaluation_provenance,
            "execution_plan_fingerprint": benchmark.execution_plan_fingerprint,
            "benchmark_outcome": benchmark.benchmark_outcome,
            "directory": benchmark.directory,
            "agent": benchmark.agent,
            "workload": benchmark.workload,
            "task": benchmark.task,
            "replica_index": benchmark.replica_index,
            "execution_cohort": benchmark.execution_cohort,
            "reconciled": benchmark.reconciled,
            "result_source": benchmark.result_source,
            "selection": benchmark.selection,
            "evaluation_completed": benchmark.evaluation_completed,
            "workload_evaluation": benchmark.workload_evaluation,
            "provider": benchmark.provider,
            "model": benchmark.model,
            "metrics": benchmark.metrics,
            "host_metadata": benchmark.host_metadata,
            "components": benchmark.components,
            "series": benchmark.series.iter().map(|(name, series)| (name, &series.values)).collect::<BTreeMap<_, _>>(),
            "series_timestamps": benchmark.series.iter().filter_map(|(name, series)| series.timestamps_ms.as_ref().map(|timestamps| (name, timestamps))).collect::<BTreeMap<_, _>>(),
            "status": benchmark.status,
            "artifacts": benchmark.artifacts,
        })).collect::<Vec<_>>(),
        "graphs": graphs,
    });
    let summary_path = comparison_dir.join("comparison.json");
    let mut file = fs::File::create(&summary_path).map_err(|error| {
        Error::Run(format!("cannot create {}: {error}", summary_path.display()))
    })?;
    serde_json::to_writer_pretty(&mut file, &summary)
        .map_err(|error| Error::Run(format!("cannot write {}: {error}", summary_path.display())))?;
    writeln!(file).map_err(|error| Error::Run(error.to_string()))?;
    write_html_report(
        &comparison_dir,
        &benchmark_data,
        &graphs,
        plan.as_ref().map(|plan| plan.execution_mode.as_str()),
        plan.as_ref().and_then(|plan| plan.scheduling),
    )?;
    writeln!(
        output,
        "Compared {} benchmarks; wrote {} graphs, report.html, and {}",
        benchmark_data.len(),
        graphs.len(),
        comparison_dir.display()
    )
    .map_err(|error| Error::Io(error.to_string()))?;
    Ok(comparison_dir.join("report.html"))
}

/// Invoke the platform opener without a shell; report paths are never commands.
pub(crate) fn open_report(path: &Path) -> Result<(), Error> {
    let path = path.canonicalize().map_err(crate::io_error)?;
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("rundll32.exe");
        command.arg("url.dll,FileProtocolHandler");
        command
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = std::process::Command::new("xdg-open");
    let status = command.arg(&path).status().map_err(|error| Error::Run(format!(
        "report created at {}, but the default browser could not be opened: {error}; open this HTML file in your browser", path.display())))?;
    if !status.success() {
        return Err(Error::Run(format!(
            "report created at {}, but the browser opener failed ({status}); open this HTML file in your browser",
            path.display()
        )));
    }
    Ok(())
}

fn benchmark_identity(path: &Path) -> Result<(String, String, String, String, usize), Error> {
    let text = read_config(path)?;
    let value: toml::Value = toml::from_str(&text).map_err(|error| {
        Error::Input(format!(
            "cannot parse benchmark {}: {error}",
            path.display()
        ))
    })?;
    let table = value
        .as_table()
        .ok_or_else(|| Error::Input(format!("benchmark {} is not a TOML table", path.display())))?;
    let field = |name: &str| {
        table
            .get(name)
            .and_then(toml::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                Error::Input(format!(
                    "benchmark {} is missing string field `{name}`",
                    path.display()
                ))
            })
    };
    let concurrency = match table.get("concurrency") {
        None => 1,
        Some(value) => value
            .as_integer()
            .and_then(|n| usize::try_from(n).ok())
            .filter(|n| (1..=crate::benchmark::MAX_CONCURRENCY).contains(n))
            .ok_or_else(|| {
                Error::Input(format!(
                    "benchmark {} concurrency must be an integer from 1 through {}",
                    path.display(),
                    crate::benchmark::MAX_CONCURRENCY
                ))
            })?,
    };
    Ok((
        field("agent")?,
        field("workload")?,
        field("provider")?,
        field("model")?,
        concurrency,
    ))
}

fn artifact_links(sweep_parent: &Path, measurement_dir: &Path) -> Vec<String> {
    let mut files = Vec::new();
    // Measurement artifacts are the primary evidence for a comparison. Add
    // them before the bounded recursive inventory so a large external
    // workload workspace cannot crowd collector artifacts out of the
    // HTML report's artifact links.
    if let Ok(entries) = fs::read_dir(measurement_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(crate::collectors::prioritizes_artifact)
            {
                files.push(path);
            }
        }
    }
    // Do not inventory external workload workspaces; only measurement output
    // is comparison evidence and should appear in the report.
    collect_artifacts(measurement_dir, 0, &mut files);
    files.sort();
    files.dedup();
    files
        .into_iter()
        .filter(|path| path.file_name().and_then(|name| name.to_str()) != Some("run.json"))
        .filter_map(|path| relative_artifact(sweep_parent, &path))
        .map(|path| format!("../../{}", path.to_string_lossy().replace('\\', "/")))
        .collect()
}

fn relative_artifact(base: &Path, path: &Path) -> Option<PathBuf> {
    // A standalone config and its project-scoped captures may be in sibling
    // directories. Keep those links portable, including absolute capture paths.
    let base = if base.as_os_str().is_empty() {
        Path::new(".")
    } else {
        base
    };
    let base = base.canonicalize().ok()?;
    let path = path.canonicalize().ok()?;
    let base: Vec<_> = base.components().collect();
    let path: Vec<_> = path.components().collect();
    let shared = base.iter().zip(&path).take_while(|(a, b)| a == b).count();
    if shared == 0 {
        return None;
    }
    let mut relative = PathBuf::new();
    for _ in shared..base.len() {
        relative.push("..");
    }
    for component in &path[shared..] {
        relative.push(component.as_os_str());
    }
    Some(relative)
}

fn collect_artifacts(path: &Path, depth: usize, output: &mut Vec<PathBuf>) {
    if depth > 4 || output.len() >= 256 {
        return;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let child = entry.path();
        if child.is_file() {
            output.push(child);
        } else if child.is_dir() {
            collect_artifacts(&child, depth + 1, output);
        }
        if output.len() >= 256 {
            return;
        }
    }
}

/// Render the process-tree accounting captured for every agent execution.
/// The bars are accounting composition, not a chronological thread timeline:
/// a concurrent process tree can execute and sleep simultaneously.
fn write_execution_timeline_svg(path: &Path, benchmarks: &[BenchmarkData]) -> Result<bool, Error> {
    let mut rows = Vec::new();
    for benchmark in benchmarks {
        let Some(component) = benchmark
            .components
            .as_ref()
            .and_then(|v| v.pointer("/components/agent.execute"))
        else {
            continue;
        };
        let state = |name| {
            benchmark
                .metrics
                .get(&format!("perfetto-state-summary.{name}"))
                .copied()
                .unwrap_or(0.0)
        };
        let running = state("running_ms");
        let runnable = state("runnable_ms");
        let sleeping = state("interruptible_sleep_ms");
        let blocked = state("uninterruptible_sleep_ms");
        let io_wait = state("io_wait_ms");
        let other_state = state("other_scheduler_state_ms");
        let wall = component
            .get("wall_ms")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        if running + runnable + sleeping + blocked + io_wait + other_state <= 0.0 && wall <= 0.0 {
            continue;
        }
        let other =
            (wall - running - runnable - sleeping - blocked - io_wait - other_state).max(0.0);
        rows.push((
            benchmark.label.as_str(),
            [
                running,
                runnable,
                sleeping,
                blocked,
                io_wait,
                other_state,
                other,
            ],
        ));
    }
    if rows.is_empty() {
        return Ok(false);
    }
    let height = 100 + rows.len() * 44;
    let colors = [
        "#1976d2", "#c62828", "#78909c", "#6a1b9a", "#ef6c00", "#b0bec5", "#eceff1",
    ];
    let labels = [
        "running",
        "runnable",
        "interruptible sleep",
        "uninterruptible sleep",
        "I/O wait",
        "other scheduler state",
        "unclassified elapsed wall time",
    ];
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"980\" height=\"{height}\" viewBox=\"0 0 980 {height}\"><title>Agent execution time composition</title><rect width=\"100%\" height=\"100%\" fill=\"#fff\"/><text x=\"24\" y=\"30\" font-family=\"sans-serif\" font-size=\"18\" font-weight=\"bold\">Agent execution time composition</text><text x=\"24\" y=\"51\" font-family=\"sans-serif\" font-size=\"12\" fill=\"#52606d\">Process-tree accounting; CPU categories may overlap wall time for concurrent children.</text>"
    );
    for (index, label) in labels.iter().enumerate() {
        let x = 250 + index * 135;
        svg.push_str(&format!("<rect x=\"{x}\" y=\"65\" width=\"10\" height=\"10\" fill=\"{}\"/><text x=\"{}\" y=\"75\" font-family=\"sans-serif\" font-size=\"11\">{}</text>", colors[index], x + 14, escape(label)));
    }
    for (row, (name, values)) in rows.iter().enumerate() {
        svg.push_str(&format!("<g data-benchmark=\"{}\">", escape(name)));
        let y = 94 + row * 44;
        let total: f64 = values.iter().sum();
        svg.push_str(&format!("<text x=\"24\" y=\"{}\" font-family=\"sans-serif\" font-size=\"12\">{}</text><rect x=\"250\" y=\"{}\" width=\"690\" height=\"22\" rx=\"3\" fill=\"#edf1f4\"/>", y + 15, escape(name), y));
        let mut x = 250.0;
        for (i, value) in values.iter().enumerate() {
            if *value > 0.0 {
                let width = 690.0 * value / total;
                svg.push_str(&format!("<rect class=\"bar\" data-label=\"{}\" data-value=\"{value:.2} ms\" x=\"{x:.2}\" y=\"{y}\" width=\"{width:.2}\" height=\"22\" fill=\"{}\"><title>{}: {:.2} ms</title></rect>", escape(labels[i]), colors[i], labels[i], value));
                x += width;
            }
        }
        svg.push_str("</g>");
    }
    svg.push_str("</svg>\n");
    fs::write(path, svg)
        .map_err(|e| Error::Run(format!("cannot write execution timeline SVG: {e}")))?;
    Ok(true)
}

fn write_html_report(
    comparison_dir: &Path,
    benchmarks: &[BenchmarkData],
    graphs: &[String],
    selection_mode: Option<&str>,
    scheduling: Option<crate::sweep::scheduler::Scheduling>,
) -> Result<(), Error> {
    let mut dimensions: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    let mut dispatch_groups = BTreeMap::new();
    for benchmark in benchmarks {
        if let Some(cohort) = &benchmark.execution_cohort {
            let next = dispatch_groups.len() + 1;
            dispatch_groups.entry(cohort.id.as_str()).or_insert(next);
        }
    }
    dimensions.insert(
        "benchmark",
        benchmarks.iter().map(|item| item.label.clone()).collect(),
    );
    dimensions.insert(
        "agent",
        benchmarks.iter().map(|item| item.agent.clone()).collect(),
    );
    dimensions.insert(
        "provider",
        benchmarks
            .iter()
            .map(|item| item.provider.clone())
            .collect(),
    );
    dimensions.insert(
        "model",
        benchmarks.iter().map(|item| item.model.clone()).collect(),
    );
    dimensions.insert(
        "workload",
        benchmarks
            .iter()
            .map(|item| item.workload.clone())
            .collect(),
    );
    for values in dimensions.values_mut() {
        values.sort();
        values.dedup();
    }

    let mut html = String::from(HTML_HEAD);
    html = html.replacen(
        "</style></head>",
        &format!(
            "{}\n{}\n{}\n{}\n</style></head>",
            INTERACTIVE_STYLE, MODAL_STYLE, INLINE_SVG_STYLE, JSON_DETAILS_STYLE
        ),
        1,
    );
    html.push_str("<main class=\"shell\">");
    if let Some(scheduling) = scheduling {
        html.push_str(&format!("<p class=\"scheduling\">Sweep scheduling: up to {} distinct configurations; {} active attempt slots, including replicas. Concurrent tasks share host resources.</p>", scheduling.jobs, scheduling.max_active_attempts));
    }
    html.push_str("<header class=\"hero\"><p class=\"eyebrow\">ASB BENCHMARK REPORT</p><h1>Benchmark comparison</h1><p>Explore recorded benchmark results and measurements. Use the filters to focus the report without rerunning anything.</p></header>");
    let coverage = coverage::evaluation_summary(benchmarks);
    html.push_str(&format!("<section class=\"explain\"><strong>Evaluation coverage</strong><p>Execution mode: {}.</p><p>{}/{} eligible selected attempts evaluated; {} have no completed evaluation. {} attempts are unscored. {}</p><p>Partial results are not a complete-population benchmark score. See population_coverage in comparison.json for dataset selection, unique tasks and complete one-attempt-per-task scores.</p></section>",
        escape(selection_mode.unwrap_or("unknown")),
        coverage.evaluated_attempts, coverage.eligible_selected_attempts, coverage.unevaluated_attempts, coverage.unscored_attempts,
        if selection_mode.is_some() {"Pre-run task selection is preserved and matches the configurations."} else {"No pre-run selection snapshot exists; dataset completeness cannot be established from these legacy records."}));
    html.push_str(&coverage::html_table(benchmarks)?);
    html.push_str(&coverage::metric_html(benchmarks)?);
    html.push_str("<section class=\"explain\"><strong>How to read this report</strong><p>Each row is one selected task attempt for an agent/provider/model/workload combination. Runtime, success rate, token usage, host metadata, component timings, and structured collector measurements are summarized in the table and charts. Network wait remains null unless directly observed. The complete machine-readable evidence inventory is available in comparison.json.</p></section>");
    html.push_str("<section class=\"filters\"><div class=\"section-title\"><h2>Filter results</h2><div><button type=\"button\" data-action=\"all\">Select all</button><button type=\"button\" data-action=\"none\">Clear all</button></div></div><div class=\"filter-grid\">");
    for (group, values) in &dimensions {
        html.push_str(&format!("<fieldset><legend>{}</legend>", title_case(group)));
        for value in values {
            let escaped = escape(value);
            html.push_str(&format!("<label><input type=\"checkbox\" data-group=\"{group}\" value=\"{escaped}\" checked><span>{escaped}</span></label>"));
        }
        html.push_str("</fieldset>");
    }
    html.push_str("</div><p class=\"filter-count\" id=\"filter-count\"></p></section>");
    html.push_str("<section><div class=\"section-title\"><h2>Benchmark results</h2><a class=\"text-link\" href=\"comparison.json\">Download comparison.json</a></div><div class=\"table-wrap\"><table><thead><tr><th>Benchmark</th><th>Agent</th><th>Provider</th><th>Model</th><th>Workload</th><th>Runtime</th><th>Success</th><th>Tokens</th><th>Details</th></tr></thead><tbody id=\"results\">");
    for benchmark in benchmarks {
        let runtime = benchmark
            .metrics
            .get("elapsed_ms")
            .map_or_else(|| "—".into(), |value| format!("{value:.0} ms"));
        let success = benchmark.metrics.get("success_rate").map_or_else(
            || "Not measured".into(),
            |value| format!("{:.1}% ({}/1)", value * 100.0, usize::from(*value == 1.0)),
        );
        let tokens = benchmark
            .metrics
            .get("token_usage.total_tokens")
            .map_or_else(|| "—".into(), |value| format!("{value:.0}"));
        let metrics_details = if benchmark.metrics.is_empty() {
            "<p class=\"muted\">No numeric measurements were written.</p>".to_owned()
        } else {
            let items = benchmark
                .metrics
                .iter()
                .map(|(name, value)| format!("<dt>{}</dt><dd>{value:.4}</dd>", escape(name)))
                .collect::<String>();
            format!("<details><summary>All metrics</summary><dl>{items}</dl></details>")
        };
        let evaluation_details = format!(
            "<p class=\"evaluation\">Workload evaluation: <code>{}</code>. {}</p>",
            escape(&benchmark.workload_evaluation),
            if crate::workloads::resolution_eligible(&benchmark.workload_evaluation) {
                "Task-resolution rate; failures remain in the denominator. Local fixture scores are not upstream suite scores."
            } else {
                "No upstream task resolution was measured. Adapter smoke exercises and unknown graders cannot earn benchmark success credit."
            }
        );
        let host_details = benchmark.host_metadata.as_ref().map_or_else(
            String::new,
            |metadata| {
                let text = serde_json::to_string_pretty(metadata).unwrap_or_else(|_| "{}".into());
                format!(
                    "<details><summary>Host metadata</summary><pre class=\"json-details\">{}</pre></details>",
                    escape(&text)
                )
            },
        );
        let component_details = benchmark.components.as_ref().map_or_else(
            String::new,
            |components| {
                let text =
                    serde_json::to_string_pretty(components).unwrap_or_else(|_| "{}".into());
                format!(
                    "<details><summary>Component timings</summary><pre class=\"json-details\">{}</pre></details>",
                    escape(&text)
                )
            },
        );
        let artifacts = if benchmark.artifacts.is_empty() {
            String::new()
        } else {
            let links = benchmark
                .artifacts
                .iter()
                .map(|artifact| {
                    format!(
                        "<li><a href=\"{}\" download>{}</a></li>",
                        escape(artifact),
                        escape(artifact.rsplit('/').next().unwrap_or(artifact))
                    )
                })
                .collect::<String>();
            let mut previews = String::new();
            for artifact in &benchmark.artifacts {
                if benchmark.result_source == "recovered-official-grade" {
                    // Preserved partial collectors are diagnostic downloads,
                    // never final execution charts for a grade-only closure.
                    continue;
                }
                let name = artifact.rsplit('/').next().unwrap_or(artifact);
                match crate::collectors::display_artifact(name) {
                    Some(crate::collectors::ArtifactDisplay::Svg { class, label }) => {
                        previews.push_str(&inline_svg_artifact(
                            comparison_dir,
                            artifact,
                            class,
                            label,
                        )?);
                    }
                    Some(crate::collectors::ArtifactDisplay::Status {
                        class,
                        label,
                        suffix,
                    }) => {
                        previews.push_str(&format!(
                            "<p class=\"{}\">{}: <a href=\"{}\">{}</a>. {}</p>",
                            escape(class),
                            escape(label),
                            escape(artifact),
                            escape(name),
                            escape(suffix)
                        ));
                    }
                    None => {}
                }
            }
            format!(
                "<details><summary>Run artifacts</summary>{previews}<ul class=\"artifact-list\">{links}</ul></details>"
            )
        };
        html.push_str(&format!("<tr data-dimension-benchmark=\"{}\" data-dimension-agent=\"{}\" data-dimension-provider=\"{}\" data-dimension-model=\"{}\" data-dimension-workload=\"{}\"><td><strong>{}</strong></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td class=\"details\">{}{}{}{}{}{}</td></tr>",
            escape(&benchmark.label), escape(&benchmark.agent), escape(&benchmark.provider), escape(&benchmark.model), escape(&benchmark.workload),
            escape(&benchmark.label), escape(&benchmark.agent), escape(&benchmark.provider), escape(&benchmark.model), escape(&benchmark.workload),
            runtime, success, tokens,
            format_args!("{}{}<p>{}</p>", evaluation_details, if benchmark.reconciled { "<p>Reconciled retained local outcome: current artifact consistency checked; no agent rerun or historical attestation.</p>" } else { "" }, benchmark.execution_cohort.as_ref().map_or_else(
                || "Dispatch context unknown (legacy or missing measurement).".into(),
                |cohort| format!("Dispatch group {}: {} of {} selected replicas admitted; slots {}. Dispatch membership is not proof of simultaneous execution. Resumed measurements may belong to different cohorts.", dispatch_groups[cohort.id.as_str()], cohort.admitted_slots.len(), cohort.selected_replicas, cohort.admitted_slots.iter().map(usize::to_string).collect::<Vec<_>>().join(", ")))),
            component_details,
            host_details,
            metrics_details,
            artifacts,
            benchmark.status.as_ref().map_or(String::new(), |status| format!("<span class=\"status\">{}</span>", escape(status))),
            ));
    }
    html.push_str("</tbody></table></div></section>");
    let mut sections = std::collections::BTreeMap::<(&str, &str), Vec<&String>>::new();
    for graph in graphs {
        if let Some(section) = crate::collectors::graph_section(graph) {
            sections
                .entry((section.title, section.description))
                .or_default()
                .push(graph);
        }
    }
    for ((title, description), section_graphs) in sections {
        html.push_str(&format!("<section><div class=\"section-title\"><h2>{}</h2><p class=\"muted\">{}</p></div><div class=\"charts\">", escape(title), escape(description)));
        for graph in section_graphs {
            html.push_str(&chart_figure(comparison_dir, graph)?);
        }
        html.push_str("</div></section>");
    }
    html.push_str("<section><div class=\"section-title\"><h2>Charts</h2><p class=\"muted\">SVG files are linked below and remain available for download.</p></div><div class=\"charts\">");
    for graph in graphs
        .iter()
        .filter(|graph| crate::collectors::graph_section(graph).is_none())
    {
        html.push_str(&chart_figure(comparison_dir, graph)?);
    }
    html.push_str("</div></section><footer>Generated locally by <code>asb compare</code>. No provider or agent was contacted.</footer></main>");
    html.push_str(INTERACTIVE_SCRIPT);
    html.push_str(HTML_SCRIPT);
    fs::write(comparison_dir.join("report.html"), html)
        .map_err(|error| Error::Run(format!("cannot write HTML report: {error}")))
}

fn chart_figure(comparison_dir: &Path, graph: &str) -> Result<String, Error> {
    let svg = fs::read_to_string(comparison_dir.join(graph))
        .map_err(|error| Error::Run(format!("cannot read generated chart {}: {error}", graph)))?;
    Ok(format!(
        "<figure class=\"interactive-chart\" data-graph=\"{}\"><div class=\"chart-backdrop\" aria-hidden=\"true\"></div><div class=\"chart-toolbar\"><strong>{}</strong><span><button type=\"button\" class=\"chart-reset\">Reset view</button><button type=\"button\" class=\"chart-expand\" aria-label=\"Enlarge chart\">Enlarge</button><button type=\"button\" class=\"chart-close\" aria-label=\"Close enlarged chart\" hidden>Close</button></span></div><div class=\"chart-stage\"><div class=\"chart-tooltip\" role=\"status\" aria-live=\"polite\"></div>{svg}<p class=\"chart-empty\" role=\"status\" hidden>No selected benchmarks have data for this graph.</p></div><figcaption><a href=\"{}\" download>Download SVG (all results)</a></figcaption></figure>",
        escape(graph),
        escape(graph.strip_suffix(".svg").unwrap_or(graph)),
        escape(graph),
    ))
}

fn inline_svg_artifact(
    comparison_dir: &Path,
    artifact: &str,
    class: &str,
    fallback: &str,
) -> Result<String, Error> {
    let path = comparison_dir.join(artifact);
    let metadata = fs::metadata(&path).map_err(|error| {
        Error::Run(format!(
            "cannot read embedded SVG artifact {artifact}: {error}"
        ))
    })?;
    if metadata.len() > MAX_BYTES {
        return Err(Error::Run(format!(
            "embedded SVG artifact {artifact} exceeds 16 MiB"
        )));
    }
    let svg = fs::read_to_string(&path).map_err(|error| {
        Error::Run(format!(
            "cannot read embedded SVG artifact {artifact}: {error}"
        ))
    })?;
    let trimmed = svg.trim_start();
    if !trimmed.starts_with("<svg") && !trimmed.starts_with("<?xml") {
        return Err(Error::Run(format!(
            "embedded SVG artifact {artifact} does not contain an SVG document"
        )));
    }
    Ok(format!(
        "<div class=\"embedded-svg {class}\" aria-label=\"{}\">{svg}</div>",
        escape(fallback)
    ))
}

fn title_case(value: &str) -> String {
    let mut chars = value.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().collect::<String>() + chars.as_str()
    })
}

const HTML_HEAD: &str = r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>ASB sweep report</title><style>
:root{color-scheme:light;--ink:#18212b;--muted:#61707f;--line:#dce4ec;--panel:#fff;--accent:#1769aa;--accent-soft:#e8f2fa;--bg:#f4f7fa}*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--ink);font:14px/1.5 Inter,ui-sans-serif,system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif}.shell{max-width:1500px;margin:auto;padding:36px 28px 56px}.hero{background:linear-gradient(135deg,#12344d,#1769aa);color:#fff;border-radius:18px;padding:42px 46px;box-shadow:0 14px 32px #12344d33}.hero h1{font-size:clamp(2rem,4vw,3.4rem);line-height:1.05;margin:4px 0 14px;letter-spacing:-.04em}.hero p{max-width:760px;margin:0;color:#e5f2fb;font-size:16px}.eyebrow{font-size:11px!important;letter-spacing:.16em;font-weight:700}.explain,.filters,section{background:var(--panel);border:1px solid var(--line);border-radius:14px;margin-top:20px;padding:24px;box-shadow:0 4px 14px #17324d0b}.explain{border-left:4px solid var(--accent)}h2{font-size:19px;margin:0}.section-title{display:flex;align-items:center;justify-content:space-between;gap:16px;margin-bottom:16px}.filter-grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(170px,1fr));gap:14px}fieldset{border:1px solid var(--line);border-radius:10px;padding:10px 12px;min-width:0}legend{font-weight:700;color:var(--accent);padding:0 5px}label{display:flex;gap:8px;align-items:flex-start;margin:6px 0;color:var(--muted);overflow-wrap:anywhere}input{accent-color:var(--accent);margin-top:4px}.filter-count,.muted{color:var(--muted);margin:14px 0 0}.text-link,a{color:var(--accent);text-decoration:none}a:hover{text-decoration:underline}button{background:#fff;border:1px solid #b9c9d8;border-radius:7px;color:var(--accent);cursor:pointer;padding:7px 11px;margin-left:6px}button:hover{background:var(--accent-soft)}.table-wrap{overflow:auto}table{border-collapse:collapse;min-width:100%;white-space:nowrap}th{text-align:left;background:#eef4f8;color:#405466;font-size:12px;letter-spacing:.04em;text-transform:uppercase}th,td{border-bottom:1px solid var(--line);padding:12px 10px;vertical-align:top}.links{white-space:normal;min-width:170px}.links a{display:inline-block;margin:0 8px 5px 0;font-size:12px}.artifact-list{padding-left:18px;margin:8px 0;white-space:normal}.perfetto-metric{display:block;width:min(760px,100%);height:180px;border:0;margin:8px 0}.status{display:block;color:#9a4b00;background:#fff4e5;border-radius:5px;font-size:11px;padding:3px 6px;margin-top:4px;max-width:260px;white-space:normal}.charts{display:grid;grid-template-columns:repeat(auto-fit,minmax(320px,1fr));gap:18px}.charts figure{margin:0;border:1px solid var(--line);border-radius:10px;padding:10px;background:#fbfdff}.charts img{width:100%;height:auto;display:block}.charts figcaption{padding:8px 2px 2px;font-size:12px;overflow-wrap:anywhere}footer{color:var(--muted);text-align:center;padding:28px 0 0}code{background:#e9eef3;border-radius:4px;padding:2px 5px}@media(max-width:700px){.shell{padding:18px 12px}.hero{padding:30px 24px}.section-title{align-items:flex-start;flex-direction:column}.section-title button{margin:0 6px 0 0}}
</style></head><body>"#;

const INTERACTIVE_STYLE: &str = r#".interactive-chart{position:relative;overflow:hidden}.chart-toolbar{display:flex;align-items:center;justify-content:space-between;gap:12px;margin:0 0 8px;color:var(--ink);font-size:12px}.chart-toolbar strong{overflow-wrap:anywhere}.chart-toolbar button{margin:0;font-size:12px}.chart-stage{position:relative;overflow:hidden;border:1px solid var(--line);border-radius:8px;background:linear-gradient(180deg,#fff,#f7fbfe)}.chart-stage svg{display:block;width:100%;height:auto;min-height:260px;cursor:grab;touch-action:none;user-select:none}.chart-stage svg.is-panning{cursor:grabbing}.chart-stage .bar{transition:fill .12s,opacity .12s}.chart-stage .bar.hovered{fill:#d95f02;stroke:#8f3f00;stroke-width:1.5}.chart-stage .series{transition:stroke-width .12s,opacity .12s}.chart-stage .series.hovered{stroke-width:4;filter:drop-shadow(0 2px 2px #18212b44)}.chart-stage .series-point{pointer-events:all;transition:fill .12s,r .12s}.chart-stage .series-point.hovered{fill:#d95f02;stroke:#fff;stroke-width:2;r:7}.chart-tooltip{position:absolute;z-index:2;pointer-events:none;opacity:0;transform:translateY(-3px);padding:7px 10px;border:1px solid #17324d33;border-radius:7px;background:#18212bf2;color:#fff;box-shadow:0 6px 16px #17324d44;font-size:12px;line-height:1.3;white-space:nowrap;transition:opacity .12s}.chart-tooltip.visible{opacity:1}.interactive-chart figcaption{display:flex;justify-content:space-between;gap:8px}.interactive-chart figcaption::after{content:'Hover values · wheel zoom · drag pan';color:var(--muted);font-size:11px}.bpftrace-profile{display:block;width:min(760px,100%);height:220px;border:0;margin:8px 0}.flamegraph{display:block;width:min(1100px,100%);height:420px;border:1px solid var(--line);border-radius:8px;margin:10px 0;background:#fff}.flamegraph-status{color:var(--muted);font-size:12px;margin:8px 0}"#;

const MODAL_STYLE: &str = r#"body.chart-open{overflow:hidden}.chart-backdrop{display:none;position:fixed;inset:0;background:#081522c9;z-index:30}.interactive-chart.expanded{position:fixed;overflow:visible;z-index:31;left:10vw;top:10vh;width:80vw;height:80vh;margin:0;padding:18px;background:var(--panel);border:1px solid #b9c9d8;border-radius:16px;box-shadow:0 24px 80px #08152299}.interactive-chart.expanded .chart-backdrop{display:block;z-index:0}.interactive-chart.expanded .chart-toolbar{position:relative;z-index:32}.interactive-chart.expanded .chart-stage{position:relative;z-index:1;height:calc(80vh - 92px);min-height:0}.interactive-chart.expanded .chart-stage svg{height:100%;min-height:0}.interactive-chart.expanded figcaption{position:relative;z-index:32}@media(max-width:700px){.interactive-chart.expanded{left:3vw;top:3vh;width:94vw;height:94vh;padding:10px}.interactive-chart.expanded .chart-stage{height:calc(94vh - 78px)}}"#;

const INLINE_SVG_STYLE: &str = r#".embedded-svg{display:block;overflow:auto;border:1px solid var(--line);border-radius:8px;background:#fff;margin:8px 0}.embedded-svg svg{display:block;width:100%;height:auto}.embedded-svg.perfetto-metric{width:min(760px,100%);max-height:260px}.embedded-svg.bpftrace-profile{width:min(760px,100%);max-height:300px}.embedded-svg.flamegraph{width:100%;max-height:520px}.embedded-svg.flamegraph svg{min-width:900px}"#;

const JSON_DETAILS_STYLE: &str = ".json-details{max-height:360px;overflow:auto;background:#f4f7fa;border:1px solid var(--line);border-radius:7px;padding:10px;font:12px/1.4 ui-monospace,SFMono-Regular,Consolas,monospace;white-space:pre-wrap}";

const INTERACTIVE_SCRIPT: &str = r#"<script>
document.querySelectorAll('.interactive-chart').forEach(chart=>{
  const stage=chart.querySelector('.chart-stage');
  const svg=stage.querySelector('svg');
  const tooltip=chart.querySelector('.chart-tooltip');
  const reset=chart.querySelector('.chart-reset');
  const expand=chart.querySelector('.chart-expand');
  const close=chart.querySelector('.chart-close');
  const backdrop=chart.querySelector('.chart-backdrop');
  const original=svg.getAttribute('viewBox').split(/\s+/).map(Number);
  let view={x:original[0],y:original[1],w:original[2],h:original[3]};
  let drag=null;
  const interactive=[...svg.querySelectorAll('.bar,.series,.series-point')];
  const clearHover=()=>interactive.forEach(item=>item.classList.remove('hovered'));
  const positionTooltip=(event)=>{
    const bounds=stage.getBoundingClientRect();
    const left=Math.min(Math.max(8,event.clientX-bounds.left+12),Math.max(8,bounds.width-tooltip.offsetWidth-8));
    const top=Math.min(Math.max(8,event.clientY-bounds.top-42),Math.max(8,bounds.height-tooltip.offsetHeight-8));
    tooltip.style.left=`${left}px`;tooltip.style.top=`${top}px`;
  };
  const show=(item,event)=>{
    clearHover();
    item.classList.add('hovered');
    if(item.classList.contains('series-point')){
      const series=svg.querySelector(`.series[data-series-index="${item.dataset.seriesIndex}"]`);
      if(series)series.classList.add('hovered');
    }
    tooltip.textContent=item.dataset.timestamp?`${item.dataset.label}: ${item.dataset.value} @ ${item.dataset.timestamp} ms`:`${item.dataset.label}: ${item.dataset.value}`;
    tooltip.classList.add('visible');
    positionTooltip(event);
  };
  interactive.forEach(item=>item.addEventListener('pointerenter',event=>show(item,event)));
  stage.addEventListener('pointermove',event=>{if(!drag)positionTooltip(event)});
  stage.addEventListener('pointerleave',()=>{clearHover();tooltip.classList.remove('visible')});
  const applyView=()=>svg.setAttribute('viewBox',`${view.x} ${view.y} ${view.w} ${view.h}`);
  const setExpanded=expanded=>{
    chart.classList.toggle('expanded',expanded);
    document.body.classList.toggle('chart-open',expanded);
    expand.hidden=expanded;close.hidden=!expanded;
    if(expanded)close.focus();else expand.focus();
  };
  svg.addEventListener('wheel',event=>{
    event.preventDefault();
    const bounds=svg.getBoundingClientRect();
    const factor=event.deltaY<0?.85:1.15;
    const mx=view.x+(event.clientX-bounds.left)/bounds.width*view.w;
    const my=view.y+(event.clientY-bounds.top)/bounds.height*view.h;
    const w=Math.min(original[2],Math.max(original[2]/8,view.w*factor));
    const h=Math.min(original[3],Math.max(original[3]/8,view.h*factor));
    view.x=Math.max(0,Math.min(original[2]-w,mx-(mx-view.x)*w/view.w));
    view.y=Math.max(0,Math.min(original[3]-h,my-(my-view.y)*h/view.h));
    view.w=w;view.h=h;applyView();
  },{passive:false});
  svg.addEventListener('pointerdown',event=>{
    if(event.target.closest('.bar,.series-point'))return;
    drag={x:event.clientX,y:event.clientY,view:{...view}};svg.classList.add('is-panning');svg.setPointerCapture(event.pointerId);
  });
  svg.addEventListener('pointermove',event=>{
    if(!drag)return;
    const bounds=svg.getBoundingClientRect();
    view.x=Math.max(0,Math.min(original[2]-view.w,drag.view.x-(event.clientX-drag.x)/bounds.width*view.w));
    view.y=Math.max(0,Math.min(original[3]-view.h,drag.view.y-(event.clientY-drag.y)/bounds.height*view.h));
    applyView();
  });
  const stopDrag=event=>{if(!drag)return;drag=null;svg.classList.remove('is-panning');if(svg.hasPointerCapture(event.pointerId))svg.releasePointerCapture(event.pointerId)};
  svg.addEventListener('pointerup',stopDrag);svg.addEventListener('pointercancel',stopDrag);
  reset.addEventListener('click',()=>{view={x:original[0],y:original[1],w:original[2],h:original[3]};applyView()});
  expand.addEventListener('click',()=>setExpanded(true));
  close.addEventListener('click',()=>setExpanded(false));
  backdrop.addEventListener('click',()=>setExpanded(false));
  document.addEventListener('keydown',event=>{if(event.key==='Escape'&&chart.classList.contains('expanded'))setExpanded(false)});
});
</script>"#;

const HTML_SCRIPT: &str = concat!(
    "<script>\n",
    include_str!("compare/report_filters.js"),
    "\n</script></body></html>"
);

fn checked_reference(parent: &Path, reference: &str, sweep: &str) -> Result<PathBuf, Error> {
    let path = Path::new(reference);
    if path.is_absolute()
        || path
            .components()
            .any(|component| component == std::path::Component::ParentDir)
    {
        return Err(Error::Input(format!(
            "sweep {sweep} contains unsafe benchmark path {reference}"
        )));
    }
    if path.extension().and_then(|value| value.to_str()) != Some("toml") {
        return Err(Error::Input(format!(
            "sweep {sweep} entry {reference} is not a .toml benchmark"
        )));
    }
    let resolved = parent.join(path);
    if !resolved.is_file() {
        return Err(Error::Input(format!(
            "sweep {sweep} references missing benchmark {}",
            resolved.display()
        )));
    }
    Ok(resolved)
}

fn read_json(path: &Path) -> Result<Value, Error> {
    let metadata = fs::metadata(path)
        .map_err(|error| Error::Input(format!("cannot read {}: {error}", path.display())))?;
    if metadata.len() > MAX_BYTES {
        return Err(Error::Input(format!(
            "measurement file {} exceeds 16 MiB",
            path.display()
        )));
    }
    let text = fs::read_to_string(path)
        .map_err(|error| Error::Input(format!("cannot read {}: {error}", path.display())))?;
    serde_json::from_str(&text).map_err(|error| {
        Error::Input(format!(
            "invalid measurement JSON {}: {error}",
            path.display()
        ))
    })
}

fn series_values(value: &Value) -> Vec<f64> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_f64)
        .take(2_048)
        .collect()
}

fn write_bar_svg(
    path: &Path,
    title: &str,
    values: &[(String, f64)],
    labels: &BTreeMap<String, String>,
) -> Result<(), Error> {
    let max = values
        .iter()
        .map(|(_, value)| *value)
        .fold(0.0_f64, f64::max)
        .max(1.0);
    let width = 900.0;
    let height = 460.0;
    let left = 70.0;
    let bottom = PLOT_BOTTOM;
    let bar_width = (width - left - 20.0) / values.len().max(1) as f64;
    let mut svg = svg_header(width, height, title);
    append_y_axis(&mut svg, left, PLOT_TOP, bottom, PLOT_RIGHT, max, title);
    for (index, (label, value)) in values.iter().enumerate() {
        svg.push_str(&format!("<g data-benchmark=\"{}\">", escape(label)));
        let display = labels.get(label).unwrap_or(label);
        let x = left + index as f64 * bar_width + 4.0;
        let bar_height = value.max(0.0) / max * (bottom - PLOT_TOP);
        let y = bottom - bar_height;
        let width = (bar_width - 8.0).max(2.0);
        svg.push_str(&format!(
            "<rect class=\"bar\" data-label=\"{}\" data-value=\"{value}\" x=\"{x:.1}\" y=\"{y:.1}\" width=\"{width:.1}\" height=\"{bar_height:.1}\" fill=\"#4472c4\"><title>{} [{}]: {value}</title></rect>",
            escape(label),
            escape(display),
            escape(label),
        ));
        let value_label = format_tick(*value);
        let (label_y, placement, color) = if bar_height >= 24.0 {
            // Keep a small top inset while retaining enough contrast against
            // the bar fill for the value to remain readable.
            (y + 16.0, "inside", "#ffffff")
        } else {
            // Short and zero-height bars cannot contain a label. Place the
            // baseline just above the bar with the same small spacing.
            (y - 5.0, "above", "#18212b")
        };
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{label_y:.1}\" text-anchor=\"middle\" class=\"bar-value\" data-placement=\"{placement}\" font-size=\"11\" font-weight=\"600\" fill=\"{color}\">{value_label}</text>",
            x + width / 2.0
        ));
        svg.push_str(&format!("<text x=\"{:.1}\" y=\"410\" transform=\"rotate(35 {:.1} 410)\" font-size=\"11\">{}</text>", x, x, escape(display)));
        svg.push_str("</g>");
    }
    svg.push_str("</svg>\n");
    fs::write(path, svg)
        .map_err(|error| Error::Run(format!("cannot write {}: {error}", path.display())))
}

fn write_line_svg(path: &Path, title: &str, values: &[(String, TimeSeries)]) -> Result<(), Error> {
    const WIDTH: f64 = 1_200.0;
    const LEFT: f64 = 100.0;
    const RIGHT: f64 = 1_160.0;
    const LEGEND_TOP: f64 = 50.0;
    const LEGEND_COLUMNS: usize = 2;
    const LEGEND_MAX_CHARS: usize = 46;
    const LEGEND_LINE_HEIGHT: f64 = 16.0;
    const LEGEND_ROW_PADDING: f64 = 12.0;
    const PLOT_HEIGHT: f64 = 320.0;
    let max = values
        .iter()
        .flat_map(|(_, points)| points.values.iter().copied())
        .fold(0.0_f64, f64::max)
        .max(1.0);
    let timestamp_bounds = values
        .iter()
        .filter_map(|(_, points)| points.timestamps_ms.as_ref())
        .flat_map(|timestamps| timestamps.iter().copied())
        .fold(None, |bounds: Option<(f64, f64)>, timestamp| {
            Some(match bounds {
                Some((min, max)) => (min.min(timestamp), max.max(timestamp)),
                None => (timestamp, timestamp),
            })
        });
    let legend_columns = values.len().clamp(1, LEGEND_COLUMNS);
    let legend_column_width = (RIGHT - LEFT) / legend_columns as f64;
    let legend_lines = values
        .iter()
        .map(|(label, _)| wrap_label(label, LEGEND_MAX_CHARS))
        .collect::<Vec<_>>();
    let legend_rows = values.len().div_ceil(legend_columns);
    let legend_height = (0..legend_rows)
        .map(|row| {
            (0..legend_columns)
                .filter_map(|column| legend_lines.get(row * legend_columns + column))
                .map(|lines| lines.len())
                .max()
                .unwrap_or(1) as f64
                * LEGEND_LINE_HEIGHT
                + LEGEND_ROW_PADDING
        })
        .sum::<f64>();
    let plot_top = LEGEND_TOP + legend_height + 18.0;
    let bottom = plot_top + PLOT_HEIGHT;
    let height = bottom + 72.0;
    let mut svg = svg_header(WIDTH, height, title);
    svg.push_str(&format!(
        "<rect class=\"legend-panel\" x=\"{LEFT:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{legend_height:.1}\" rx=\"8\" fill=\"#f4f8fb\" stroke=\"#dce4ec\"/>",
        LEGEND_TOP - 12.0,
        RIGHT - LEFT
    ));
    append_y_axis(&mut svg, LEFT, plot_top, bottom, RIGHT, max, title);
    append_series_x_axis(
        &mut svg,
        timestamp_bounds,
        values
            .iter()
            .map(|(_, points)| points.values.len())
            .max()
            .unwrap_or(0),
        LEFT,
        bottom,
        RIGHT,
    );
    for (series_index, (label, points)) in values.iter().enumerate() {
        svg.push_str(&format!("<g data-benchmark=\"{}\">", escape(label)));
        let color = ["#4472c4", "#ed7d31", "#70ad47", "#a5a5a5", "#ffc000"][series_index % 5];
        let coordinates = points
            .values
            .iter()
            .enumerate()
            .map(|(index, value)| {
                let x = line_x(
                    index,
                    points.timestamps_ms.as_deref(),
                    timestamp_bounds,
                    points.values.len(),
                    LEFT,
                    RIGHT,
                );
                let y = bottom - value.max(0.0) / max * (bottom - plot_top);
                format!("{x:.1},{y:.1}")
            })
            .collect::<Vec<_>>()
            .join(" ");
        svg.push_str(&format!(
            "<polyline class=\"series\" data-series-index=\"{series_index}\" data-label=\"{}\" points=\"{coordinates}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"2\"/>",
            escape(label)
        ));
        for (index, value) in points.values.iter().enumerate() {
            let x = line_x(
                index,
                points.timestamps_ms.as_deref(),
                timestamp_bounds,
                points.values.len(),
                LEFT,
                RIGHT,
            );
            let y = bottom - value.max(0.0) / max * (bottom - plot_top);
            let timestamp = points
                .timestamps_ms
                .as_ref()
                .and_then(|timestamps| timestamps.get(index))
                .map_or_else(String::new, |timestamp| {
                    format!(" data-timestamp=\"{timestamp}\"")
                });
            svg.push_str(&format!(
                "<circle class=\"series-point\" data-series-index=\"{series_index}\" data-label=\"{}\" data-value=\"{value}\"{timestamp} cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"6\" fill=\"transparent\" stroke=\"none\"/>",
                escape(label)
            ));
        }
        let column = series_index % legend_columns;
        let row = series_index / legend_columns;
        let legend_x = LEFT + column as f64 * legend_column_width;
        let row_y = LEGEND_TOP
            + (0..row)
                .map(|previous_row| {
                    (0..legend_columns)
                        .filter_map(|previous_column| {
                            legend_lines.get(previous_row * legend_columns + previous_column)
                        })
                        .map(|lines| lines.len())
                        .max()
                        .unwrap_or(1) as f64
                        * LEGEND_LINE_HEIGHT
                        + LEGEND_ROW_PADDING
                })
                .sum::<f64>();
        let lines = &legend_lines[series_index];
        let legend_y = row_y + 6.0;
        svg.push_str(&format!(
            "<rect class=\"legend-swatch\" x=\"{legend_x:.1}\" y=\"{:.1}\" width=\"10\" height=\"10\" rx=\"2\" fill=\"{color}\"/><text class=\"legend-item\" data-series-index=\"{series_index}\" x=\"{:.1}\" y=\"{:.1}\" font-size=\"12\" fill=\"{color}\" aria-label=\"{}\">",
            legend_y - 10.0,
            legend_x + 16.0,
            legend_y,
            escape(label)
        ));
        for (line_index, line) in lines.iter().enumerate() {
            svg.push_str(&format!(
                "<tspan x=\"{:.1}\" dy=\"{}\">{}</tspan>",
                legend_x + 16.0,
                if line_index == 0 { 0 } else { 15 },
                escape(line)
            ));
        }
        svg.push_str("</text></g>");
    }
    svg.push_str("</svg>\n");
    fs::write(path, svg)
        .map_err(|error| Error::Run(format!("cannot write {}: {error}", path.display())))
}

fn line_x(
    index: usize,
    timestamps: Option<&[f64]>,
    bounds: Option<(f64, f64)>,
    length: usize,
    left: f64,
    right: f64,
) -> f64 {
    let fraction = timestamps
        .and_then(|timestamps| timestamps.get(index))
        .zip(bounds)
        .map_or_else(
            || index as f64 / length.saturating_sub(1).max(1) as f64,
            |(timestamp, (min, max))| {
                if max > min {
                    (*timestamp - min) / (max - min)
                } else {
                    index as f64 / length.saturating_sub(1).max(1) as f64
                }
            },
        );
    left + fraction.clamp(0.0, 1.0) * (right - left)
}

fn append_series_x_axis(
    svg: &mut String,
    timestamp_bounds: Option<(f64, f64)>,
    length: usize,
    left: f64,
    bottom: f64,
    right: f64,
) {
    let (label, unit) = if timestamp_bounds.is_some() {
        ("time", "ms")
    } else {
        ("sample", "index")
    };
    for tick in 0..=Y_TICKS {
        let fraction = tick as f64 / Y_TICKS as f64;
        let x = left + fraction * (right - left);
        let value = timestamp_bounds
            .map(|(min, max)| min + fraction * (max - min))
            .unwrap_or_else(|| fraction * length.saturating_sub(1) as f64);
        svg.push_str(&format!(
            "<line class=\"x-axis-tick\" x1=\"{x:.1}\" y1=\"{bottom:.1}\" x2=\"{x:.1}\" y2=\"{:.1}\" stroke=\"#18212b\"/><text class=\"x-axis-value\" x=\"{x:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"11\">{}</text>",
            bottom + 6.0,
            bottom + 22.0,
            format_args!("{} {}", format_tick(value), unit)
        ));
    }
    svg.push_str(&format!(
        "<text class=\"x-axis-label\" x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"12\">{label} ({unit})</text>",
        (left + right) / 2.0,
        bottom + 43.0
    ));
}

fn wrap_label(value: &str, max_chars: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut remaining = value.trim();
    while remaining.chars().count() > max_chars {
        let boundary = remaining
            .char_indices()
            .take_while(|(index, _)| *index <= max_chars)
            .filter(|(_, character)| matches!(character, '_' | '-' | '.' | '/' | ' '))
            .map(|(index, character)| index + character.len_utf8())
            .last()
            .unwrap_or_else(|| {
                remaining
                    .char_indices()
                    .nth(max_chars)
                    .map_or(remaining.len(), |(index, _)| index)
            });
        let split_at = boundary.max(1).min(remaining.len());
        let (line, rest) = remaining.split_at(split_at);
        lines.push(line.trim().to_owned());
        remaining = rest.trim_start();
    }
    if !remaining.is_empty() || lines.is_empty() {
        lines.push(remaining.to_owned());
    }
    lines
}

fn append_y_axis(
    svg: &mut String,
    left: f64,
    top: f64,
    bottom: f64,
    right: f64,
    max: f64,
    title: &str,
) {
    svg.push_str(&format!(
        "<line x1=\"{left}\" y1=\"{top}\" x2=\"{left}\" y2=\"{bottom}\" stroke=\"black\"/><line x1=\"{left}\" y1=\"{bottom}\" x2=\"{right}\" y2=\"{bottom}\" stroke=\"black\"/>"
    ));
    for tick in 0..=Y_TICKS {
        let fraction = tick as f64 / Y_TICKS as f64;
        let y = bottom - fraction * (bottom - top);
        let value = max * fraction;
        svg.push_str(&format!(
            "<line x1=\"{left}\" y1=\"{y:.1}\" x2=\"{right}\" y2=\"{y:.1}\" stroke=\"#dddddd\"/><text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\" font-size=\"11\">{}</text>",
            left - 6.0,
            y + 4.0,
            format_tick(value)
        ));
    }
    let unit = metric_unit(title);
    svg.push_str(&format!(
        "<text x=\"15\" y=\"230\" text-anchor=\"middle\" transform=\"rotate(-90 15 230)\" font-size=\"12\">unit: {}</text>",
        escape(unit)
    ));
}

fn format_tick(value: f64) -> String {
    if value.abs() >= 100.0 || value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.2}")
    }
}

fn metric_unit(title: &str) -> &'static str {
    let name = title.to_ascii_lowercase();
    if name.ends_with("_pct") {
        "%"
    } else if name.ends_with("_ms") || name.contains("elapsed_ms") {
        "ms"
    } else if name.ends_with("_kib_per_s") {
        "KiB/s"
    } else if name.ends_with("_ops_per_s") {
        "ops/s"
    } else if name.ends_with("_per_s") {
        "/s"
    } else if name.ends_with("_kib") {
        "KiB"
    } else if name.ends_with("_mib") {
        "MiB"
    } else if name.ends_with("_depth") {
        "depth"
    } else if name.contains("elapsed_s") || name.ends_with("_seconds") {
        "s"
    } else if name.contains("token") {
        "tokens"
    } else if name == "steps" || name.ends_with("_steps") {
        "steps"
    } else if name.contains("success_rate") || name.ends_with("_rate") {
        "ratio"
    } else if name.contains("count") {
        "count"
    } else {
        "value"
    }
}

fn svg_header(width: f64, height: f64, title: &str) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\"><title>{}</title><text x=\"20\" y=\"30\" font-size=\"18\">{}</text>",
        escape(title),
        escape(title)
    )
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn safe_name(path: &Path) -> String {
    safe_name_str(
        path.file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("benchmark"),
    )
}

fn safe_name_str(value: &str) -> String {
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

#[cfg(test)]
#[path = "../tests/unit/compare_boundaries.rs"]
mod boundary_tests;

#[cfg(test)]
mod tests {
    use super::GraphNames;
    use super::run;
    use serde_json::{Value, json};
    use std::collections::{BTreeMap, BTreeSet};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn dispatch_provenance_is_not_a_numeric_measurement() {
        let mut metrics = std::collections::BTreeMap::new();
        super::flatten_numbers(
            "",
            &json!({"replica_index":2,"execution_cohort":{
            "id":"abcd","selected_replicas":3,"admitted_slots":[2]},"elapsed_ms":42}),
            &mut metrics,
        )
        .unwrap();
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics["elapsed_ms"], 42.0);
    }

    #[test]
    fn agent_process_graphs_include_all_agents_but_not_unavailable_measurements() {
        let root = std::env::temp_dir().join(format!(
            "asb-agent-compare-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("sweep.toml"),
            "configs=['one.toml','two.toml','missing.toml']\n",
        )
        .unwrap();
        for (index, (name, agent, memory, workers)) in [
            ("one", "codex", 10.0, 0),
            ("two", "pi", 25.5, 2),
            ("missing", "opencode", 0.0, 0),
        ]
        .into_iter()
        .enumerate()
        {
            std::fs::write(
                root.join(format!("{name}.toml")),
                format!("agent='{agent}'\nworkload='bug-fix'\nprovider='local'\nmodel='m'\n"),
            )
            .unwrap();
            let dir = root.join(format!("sweep-results/{index:03}-{name}/measurements"));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("run.json"),
                r#"{"status":"completed","elapsed_ms":12}"#,
            )
            .unwrap();
            for (collector, metric, value) in [
                ("agent-memory", "peak_rss_mib", json!(memory)),
                ("agent-workers", "spawned_worker_count", json!(workers)),
            ] {
                let data = if name == "missing" {
                    json!({"status":"unavailable","metrics":{}})
                } else {
                    json!({"status":"observed","metrics":{metric:value}})
                };
                std::fs::write(dir.join(format!("{collector}.json")), data.to_string()).unwrap();
            }
        }
        run(root.join("sweep.toml").to_str().unwrap(), &mut Vec::new()).unwrap();
        let dir = root.join("results/sweep-comparison");
        for (file, unit) in [
            ("metric-agent-memory.peak_rss_mib.svg", "MiB"),
            ("metric-agent-workers.spawned_worker_count.svg", "count"),
        ] {
            let svg = std::fs::read_to_string(dir.join(file)).unwrap();
            assert_eq!(svg.matches("class=\"bar\"").count(), 2, "{svg}");
            assert!(svg.contains("codex [000-one]"));
            assert!(svg.contains("pi [001-two]"));
            assert!(svg.contains(">codex</text>"));
            assert!(svg.contains("data-label=\"000-one\""));
            assert!(svg.contains(&format!("unit: {unit}")));
            assert!(!svg.contains("opencode"));
        }
        let html = std::fs::read_to_string(dir.join("report.html")).unwrap();
        assert!(html.contains("<h2>Agent memory</h2>"));
        assert!(html.contains("<h2>Agent workers</h2>"));
        assert!(html.contains("Sampled peak aggregate RSS"));
        assert!(html.contains("metric-agent-workers.spawned_worker_count.svg"));
        assert!(html.contains("data-value=\"25.5\""));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn compares_measurements_and_writes_svg() {
        let root = std::env::temp_dir().join(format!(
            "asb-compare-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("sweep-results/000-one/measurements")).unwrap();
        std::fs::create_dir_all(root.join("sweep-results/000-one/workspace")).unwrap();
        for index in 0..300 {
            std::fs::write(
                root.join(format!("sweep-results/000-one/workspace/file-{index}.txt")),
                "external workload evidence",
            )
            .unwrap();
        }
        std::fs::write(
            root.join("one.toml"),
            "agent=\"codex\"\nworkload=\"bug-fix\"\nprovider=\"local\"\nmodel=\"m\"\n",
        )
        .unwrap();
        std::fs::write(root.join("sweep.toml"), "configs=[\"one.toml\"]\n").unwrap();
        std::fs::write(
            root.join("sweep-results/000-one/measurements/run.json"),
            r#"{"status":"completed","success":true,"success_rate":1.0,"elapsed_ms":12,"token_usage":{"total_tokens":7}}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("sweep-results/000-one/measurements/system.json"),
            r#"{"schema_version":2,"architecture":"aarch64","cpu_model":"Neoverse-N1","logical_cpus":8,"memory_total_kib":16384,"kernel_settings":{"numa_balancing":"1"}}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("sweep-results/000-one/measurements/components.json"),
            r#"{"schema_version":1,"clock_ticks_per_second":100,"components":{"agent.execute":{"wall_ms":12,"cpu_user_ms":3.5,"cpu_system_ms":1.5,"io_wait_ms":2.0,"scheduler_wait_ms":1.0,"network_wait_ms":null}}}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("sweep-results/000-one/measurements/mpstat.json"),
            r#"{"stdout":"sample 1\nsample 2\n","metrics":{"cpu.user_pct":12.5,"cpu.iowait_pct":3.0}}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("sweep-results/000-one/measurements/perfetto.json"),
            r#"{"metrics":{"perfetto.scheduler_events":24,"perfetto.tracepoints":4},"series":{"perfetto.scheduler_events_per_s":[3,7]},"series_timestamps":{"perfetto.scheduler_events_per_s":[12000,13000]}}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("sweep-results/000-one/measurements/perfetto-metric-scheduler_events.svg"),
            "<svg><title>Perfetto scheduler events</title></svg>",
        )
        .unwrap();
        std::fs::write(
            root.join("sweep-results/000-one/measurements/perfetto-state-timeline.svg"),
            "<svg><title>Exact scheduler state timeline</title></svg>",
        )
        .unwrap();
        std::fs::write(
            root.join("sweep-results/000-one/measurements/bpftrace-sched-latency.json"),
            r#"{"metrics":{"bpftrace.sched-latency.wakeup_latency_events":7}}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("sweep-results/000-one/measurements/bpftrace-sched-latency.svg"),
            "<svg><title>bpftrace sched-latency</title></svg>",
        )
        .unwrap();
        std::fs::write(
            root.join("sweep-results/000-one/measurements/flamegraph.svg"),
            "<svg><title>flamegraph</title></svg>",
        )
        .unwrap();
        std::fs::write(
            root.join("sweep-results/000-one/measurements/flamegraph.json"),
            r#"{"status":"completed","artifact":"flamegraph.svg"}"#,
        )
        .unwrap();
        run(root.join("sweep.toml").to_str().unwrap(), &mut Vec::new()).unwrap();
        assert!(
            root.join("results/sweep-comparison/comparison.json")
                .is_file()
        );
        assert!(
            root.join("results/sweep-comparison/metric-elapsed_ms.svg")
                .is_file()
        );
        assert!(
            root.join("results/sweep-comparison")
                .join(
                    GraphNames::default()
                        .allocate("metric", "component.agent\\.execute.cpu_user_ms")
                )
                .is_file()
        );
        let timeline = root.join("results/sweep-comparison/execution-timeline.svg");
        assert!(timeline.is_file());
        let timeline_svg = std::fs::read_to_string(timeline).unwrap();
        assert!(timeline_svg.contains("Agent execution time composition"));
        assert!(timeline_svg.contains("<g data-benchmark=\"000-one\">"));
        assert!(timeline_svg.contains("I/O wait"));
        assert!(
            root.join("results/sweep-comparison/metric-host.logical_cpus.svg")
                .is_file()
        );
        let elapsed_svg =
            std::fs::read_to_string(root.join("results/sweep-comparison/metric-elapsed_ms.svg"))
                .unwrap();
        assert!(elapsed_svg.contains("text-anchor=\"end\""));
        assert!(elapsed_svg.contains("<g data-benchmark=\"000-one\">"));
        assert!(elapsed_svg.contains("unit: ms"));
        assert!(elapsed_svg.contains(">0</text>"));
        assert!(elapsed_svg.contains(">12</text>"));
        assert!(
            root.join("results/sweep-comparison")
                .join(GraphNames::default().allocate("metric", "mpstat.cpu\\.user_pct"))
                .is_file()
        );
        assert!(
            root.join("results/sweep-comparison")
                .join(
                    GraphNames::default()
                        .allocate("metric", "perfetto.perfetto\\.scheduler_events")
                )
                .is_file()
        );
        assert!(
            root.join("results/sweep-comparison/series-perfetto.scheduler_events_per_s.svg")
                .is_file()
        );
        assert!(
            root.join("results/sweep-comparison")
                .join(GraphNames::default().allocate(
                    "metric",
                    "bpftrace-sched-latency.bpftrace\\.sched-latency\\.wakeup_latency_events"
                ))
                .is_file()
        );
        assert!(
            !root
                .join("results/sweep-comparison/series-mpstat.svg")
                .exists()
        );
        let report =
            std::fs::read_to_string(root.join("results/sweep-comparison/report.html")).unwrap();
        let comparison: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("results/sweep-comparison/comparison.json"))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(comparison["benchmarks"][0]["status"], "completed");
        assert!(report.contains("Filter results"));
        assert!(report.contains("data-group=\"agent\""));
        assert!(report.contains("No selected benchmarks have data for this graph."));
        assert!(report.contains("installReportFilters(document)"));
        let line_svg = std::fs::read_to_string(
            root.join("results/sweep-comparison/series-perfetto.scheduler_events_per_s.svg"),
        )
        .unwrap();
        assert!(line_svg.contains("<g data-benchmark=\"000-one\">"));
        assert!(line_svg.contains("</text></g>"));
        assert!(report.contains(">Details</th>"));
        assert!(report.contains("All metrics"));
        assert!(report.contains("Host metadata"));
        assert!(report.contains("Neoverse-N1"));
        assert!(report.contains("Component timings"));
        assert!(report.contains("network_wait_ms"));
        assert!(comparison["benchmarks"][0]["host_metadata"]["architecture"] == "aarch64");
        assert_eq!(
            comparison["benchmarks"][0]["components"]["components"]["agent.execute"]["cpu_user_ms"],
            3.5
        );
        assert!(!report.contains("run.json"));
        assert!(report.contains("Perfetto metric SVG"));
        assert!(report.contains("perfetto-metric-scheduler_events.svg"));
        assert!(report.contains("perfetto-state-timeline.svg"));
        assert!(report.contains("<div class=\"embedded-svg perfetto-metric\""));
        assert!(report.contains("bpftrace profile SVG"));
        assert!(report.contains("bpftrace-sched-latency.svg"));
        assert!(report.contains(
            "<div class=\"embedded-svg bpftrace-profile\" aria-label=\"bpftrace profile SVG\"><svg>"
        ));
        assert!(report.contains("bpftrace profiles"));
        assert!(report.contains("class=\"embedded-svg flamegraph\""));
        assert!(report.contains("flamegraph.svg"));
        assert!(report.contains(
            "<div class=\"embedded-svg flamegraph\" aria-label=\"Flamegraph SVG\"><svg>"
        ));
        assert!(!report.contains("<object class=\"flamegraph\""));
        assert!(report.contains("Flamegraph collector status"));
        assert!(report.contains("class=\"interactive-chart\""));
        assert!(report.contains("series-perfetto.scheduler_events_per_s.svg"));
        assert!(report.contains("data-timestamp=\"12000\""));
        assert!(report.contains("chart-tooltip"));
        assert!(report.contains("wheel zoom"));
        assert!(report.contains("chart-expand"));
        assert!(report.contains("chart-close"));
        assert!(report.contains("80vw"));
        assert!(report.contains("overflow:visible"));
        assert!(
            report.contains(".interactive-chart.expanded .chart-backdrop{display:block;z-index:0}")
        );
        assert!(
            report.contains(".interactive-chart.expanded .chart-stage{position:relative;z-index:1")
        );
        assert!(report.contains("Select all"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn line_graph_labels_token_axis_unit_and_ticks() {
        let root = std::env::temp_dir().join(format!(
            "asb-compare-svg-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        super::write_line_svg(
            &root.join("tokens.svg"),
            "token_usage.total_tokens",
            &[(
                "agent-with-a-deliberately-long-series-name-for-the-legend".into(),
                super::TimeSeries {
                    values: vec![0.0, 100.0],
                    timestamps_ms: Some(vec![10.0, 20.0]),
                },
            )],
        )
        .unwrap();
        let svg = std::fs::read_to_string(root.join("tokens.svg")).unwrap();
        assert!(svg.contains("unit: tokens"));
        assert!(svg.contains(">0</text>"));
        assert!(svg.contains(">100</text>"));
        assert!(svg.contains("data-timestamp=\"10\""));
        assert!(svg.contains("class=\"x-axis-tick\""));
        assert!(svg.contains("class=\"x-axis-value\""));
        assert!(svg.contains("class=\"x-axis-label\""));
        assert!(svg.contains(">time (ms)</text>"));
        assert!(svg.contains("10 ms"));
        assert!(svg.contains("20 ms"));
        assert!(svg.contains("class=\"legend-item\""));
        assert!(svg.contains("class=\"legend-panel\""));
        assert!(svg.contains("<tspan"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn line_graph_labels_sample_axis_and_gives_long_legends_room() {
        let root = std::env::temp_dir().join(format!(
            "asb-compare-sample-axis-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let series = (0..8)
            .map(|index| {
                (
                    format!(
                        "collector-{}-with-a-long-human-readable-series-name",
                        index + 1
                    ),
                    super::TimeSeries {
                        values: vec![1.0, 2.0, 3.0],
                        timestamps_ms: None,
                    },
                )
            })
            .collect::<Vec<_>>();
        super::write_line_svg(&root.join("samples.svg"), "cpu.user_pct", &series).unwrap();
        let svg = std::fs::read_to_string(root.join("samples.svg")).unwrap();
        assert!(svg.contains(">sample (index)</text>"));
        assert!(svg.contains("0 index"));
        assert!(svg.contains("2 index"));
        assert!(svg.contains("class=\"legend-panel\""));
        assert!(svg.matches("class=\"legend-item\"").count() == 8);
        let height = svg
            .split_once("height=\"")
            .and_then(|(_, value)| value.split_once('"'))
            .and_then(|(value, _)| value.parse::<f64>().ok())
            .unwrap();
        assert!(height > 620.0);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn bar_graph_labels_values_inside_or_above_each_bar() {
        let root = std::env::temp_dir().join(format!(
            "asb-compare-bars-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        super::write_bar_svg(
            &root.join("values.svg"),
            "elapsed_ms",
            &[("tall".into(), 100.0), ("short".into(), 1.0)],
            &Default::default(),
        )
        .unwrap();
        let svg = std::fs::read_to_string(root.join("values.svg")).unwrap();
        assert!(svg.contains("class=\"bar-value\" data-placement=\"inside\""));
        assert!(svg.contains("class=\"bar-value\" data-placement=\"above\""));
        assert!(svg.contains(">100</text>"));
        assert!(svg.contains(">1</text>"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn records_missing_measurements_without_discarding_sweep_results() {
        let root = std::env::temp_dir().join(format!(
            "asb-compare-missing-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("one.toml"),
            "agent=\"codex\"\nworkload=\"bug-fix\"\nprovider=\"local\"\nmodel=\"m\"\n",
        )
        .unwrap();
        std::fs::write(root.join("sweep.toml"), "configs=[\"one.toml\"]\n").unwrap();
        run(root.join("sweep.toml").to_str().unwrap(), &mut Vec::new()).unwrap();
        let summary =
            std::fs::read_to_string(root.join("results/sweep-comparison/comparison.json")).unwrap();
        assert!(summary.contains("missing measurements"));
        assert!(summary.contains("measurement collection may be disabled"));
        let parsed: Value = serde_json::from_str(&summary).unwrap();
        let group = parsed["resolution_groups"]
            .as_object()
            .unwrap()
            .values()
            .next()
            .unwrap();
        assert_eq!(group["task_count"], 1);
        assert_eq!(group["unresolved_count"], 1);
        assert_eq!(group["success_rate"], 0.0);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn official_resolution_uses_typed_baseline_and_evaluator_not_client_exit() {
        let root = std::env::temp_dir().join(format!(
            "asb-official-resolution-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("sweep.toml"),
            "configs=['resolved.toml','unresolved.toml','missing.toml','invalid.toml']\n",
        )
        .unwrap();
        for (index, name, resolved) in [
            (0, "resolved", json!(true)),
            (1, "unresolved", json!(false)),
            (2, "missing", Value::Null),
            (3, "invalid", json!(true)),
        ] {
            std::fs::write(
                root.join(format!("{name}.toml")),
                "agent='pi'\nworkload='swe-bench-lite'\nprovider='local'\nmodel='m'\n",
            )
            .unwrap();
            let baseline = if name == "invalid" {
                json!({"kind":"upstream-task-definition","base_commit":"HEAD"})
            } else {
                json!({"kind":"upstream-task-definition","base_commit":"a".repeat(40)})
            };
            let directory = root.join(format!("sweep-results/{index:03}-{name}/measurements"));
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(
                directory.join("run.json"),
                json!({"schema_version":2,"status":"failed",
                "success":false,"process_success":false,"workload_success":resolved,
                "workload_evaluation":"upstream-harness","workload_baseline":baseline,
                "workload_baseline_passed":false})
                .to_string(),
            )
            .unwrap();
        }
        run(root.join("sweep.toml").to_str().unwrap(), &mut Vec::new()).unwrap();
        let comparison: Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("results/sweep-comparison/comparison.json"))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(comparison["benchmarks"][0]["metrics"]["success_rate"], 1.0);
        assert_eq!(comparison["benchmarks"][1]["metrics"]["success_rate"], 0.0);
        assert_eq!(comparison["benchmarks"][2]["metrics"]["success_rate"], 0.0);
        assert!(
            comparison["benchmarks"][3]["metrics"]
                .get("success_rate")
                .is_none()
        );
        let groups = comparison["resolution_groups"].as_object().unwrap();
        assert_eq!(
            groups
                .values()
                .map(|group| group["task_count"].as_u64().unwrap())
                .sum::<u64>(),
            3
        );
        assert_eq!(
            groups
                .values()
                .map(|group| group["resolved_count"].as_u64().unwrap())
                .sum::<u64>(),
            1
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_smoke_and_unknown_scores_cannot_become_task_resolution() {
        let root = std::env::temp_dir().join(format!(
            "asb-compare-scope-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("sweep.toml"),
            "configs=['smoke.toml','unknown.toml','task.toml']\n",
        )
        .unwrap();
        for (index, name, workload, evaluation) in [
            (0, "smoke", "swe-bench", "adapter-smoke"),
            (1, "unknown", "custom", "unknown"),
            (2, "task", "bug-fix", "bundled-fixture"),
        ] {
            std::fs::write(
                root.join(format!("{name}.toml")),
                format!("agent='cline'\nworkload='{workload}'\nprovider='local'\nmodel='m'\n"),
            )
            .unwrap();
            let dir = root.join(format!("sweep-results/{index:03}-{name}/measurements"));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("run.json"), json!({"schema_version":1,"status":"completed","success":true,"process_success":true,"workload_success":true,"workload_baseline_passed":false,"workload_evaluation":evaluation,"success_rate":1.0,"steps":1,"token_usage":{"total_tokens":8917}}).to_string()).unwrap();
        }
        run(root.join("sweep.toml").to_str().unwrap(), &mut Vec::new()).unwrap();
        let comparison: Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("results/sweep-comparison/comparison.json"))
                .unwrap(),
        )
        .unwrap();
        for index in [0, 1] {
            assert!(
                comparison["benchmarks"][index]["metrics"]
                    .get("success_rate")
                    .is_none()
            );
            assert!(
                comparison["benchmarks"][index]["metrics"]
                    .get("steps")
                    .is_none()
            );
            assert_eq!(
                comparison["benchmarks"][index]["metrics"]["token_usage.total_tokens"],
                8917.0
            );
        }
        assert_eq!(comparison["benchmarks"][2]["metrics"]["success_rate"], 1.0);
        let groups = comparison["resolution_groups"].as_object().unwrap();
        assert_eq!(
            groups
                .values()
                .filter(|value| value["success_rate"].is_null())
                .count(),
            2
        );
        let html =
            std::fs::read_to_string(root.join("results/sweep-comparison/report.html")).unwrap();
        assert!(html.contains("Not measured"));
        assert!(html.contains("100.0% (1/1)"));
        assert!(html.contains("No upstream task resolution was measured"));
        let _ = std::fs::remove_dir_all(root);
    }
    #[test]
    fn report_preserves_colliding_metric_names_paths_and_repeated_config_stems() {
        let root = std::env::temp_dir().join(format!(
            "asb-metric-identities-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("sweep.toml"),
            "configs=['left/same.toml','right/same.toml']",
        )
        .unwrap();
        for (index, side) in ["left", "right"].into_iter().enumerate() {
            std::fs::create_dir_all(root.join(side)).unwrap();
            std::fs::write(
                root.join(side).join("same.toml"),
                "agent='pi'\nworkload='bug-fix'\nprovider='local'\nmodel='m'\n",
            )
            .unwrap();
            let measured = root.join(format!("sweep-results/{index:03}-same/measurements"));
            std::fs::create_dir_all(&measured).unwrap();
            std::fs::write(
                measured.join("run.json"),
                json!({"status":"completed","elapsed_ms":10+index}).to_string(),
            )
            .unwrap();
            std::fs::write(
                measured.join("custom.json"),
                json!({
                    "metrics":{"a/b":113,"a?b":227,"a.b":331,"a":{"b":449},"é":557,"ü":661},
                    "series":{"custom.a/b":[1,2],"custom.a?b":[3,4]}
                })
                .to_string(),
            )
            .unwrap();
        }
        let manifest = root.join("sweep.toml");
        run(manifest.to_str().unwrap(), &mut Vec::new()).unwrap();
        let output = root.join("results/sweep-comparison");
        let summary: Value =
            serde_json::from_slice(&std::fs::read(output.join("comparison.json")).unwrap())
                .unwrap();
        let rows = summary["benchmarks"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        assert_ne!(rows[0]["name"], rows[1]["name"]);
        for row in rows {
            for (key, value) in [
                ("custom.a/b", 113),
                ("custom.a?b", 227),
                ("custom.a\\.b", 331),
                ("custom.a.b", 449),
                ("custom.é", 557),
                ("custom.ü", 661),
            ] {
                assert_eq!(row["metrics"][key].as_f64(), Some(value as f64), "{row}");
            }
        }
        let graphs = summary["graphs"].as_array().unwrap();
        let names: BTreeSet<_> = graphs.iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(names.len(), graphs.len());
        let html = std::fs::read_to_string(output.join("report.html")).unwrap();
        let mut bar_titles = BTreeMap::new();
        let mut series_titles = BTreeMap::new();
        for name in names {
            let svg = std::fs::read_to_string(output.join(name)).unwrap();
            assert!(html.contains(name), "missing graph reference {name}");
            let title = svg
                .split("<title>")
                .nth(1)
                .unwrap()
                .split("</title>")
                .next()
                .unwrap()
                .to_owned();
            if name.starts_with("metric-") {
                assert_eq!(svg.matches("class=\"bar\"").count(), 2);
                assert!(bar_titles.insert(title, svg).is_none());
            } else if name.starts_with("series-") {
                assert!(series_titles.insert(title, svg).is_none());
            }
        }
        for (name, value) in [
            ("custom.a/b", 113),
            ("custom.a?b", 227),
            ("custom.a\\.b", 331),
            ("custom.a.b", 449),
            ("custom.é", 557),
            ("custom.ü", 661),
        ] {
            assert!(bar_titles[name].contains(&format!("data-value=\"{value}\"")));
        }
        assert!(series_titles.contains_key("custom.a/b"));
        assert!(series_titles.contains_key("custom.a?b"));
        let original_names = summary["graphs"].clone();
        run(manifest.to_str().unwrap(), &mut Vec::new()).unwrap();
        let regenerated: Value =
            serde_json::from_slice(&std::fs::read(output.join("comparison.json")).unwrap())
                .unwrap();
        assert_eq!(regenerated["graphs"], original_names);
        // Series keys are a shared collector contract. An ambiguous declaration
        // must fail explicitly regardless of directory iteration order.
        let conflict = root.join("sweep-results/000-same/measurements/other.json");
        std::fs::write(
            &conflict,
            json!({"series":{"custom.a/b":[9,10]}}).to_string(),
        )
        .unwrap();
        let error = run(manifest.to_str().unwrap(), &mut Vec::new())
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("conflicting measurement identity `custom.a/b`"),
            "{error}"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
