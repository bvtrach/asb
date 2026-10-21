//! Fresh sequential batches with a durable intended-slot plan and outcome links.
mod replay;
use super::{RunOptions, batch, replica};
use crate::{Error, benchmark::BenchmarkConfig, cassette::Compatibility, io_error};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const MAX_EXECUTIONS: u64 = 65_533;
const MAX_PLAN_BYTES: usize = 16 * 1024 * 1024;
const MAX_OUTCOME_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ExecutionIdentity {
    #[serde(skip_serializing_if = "Option::is_none")]
    replay_track: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    replay_seed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    replay_algorithm: Option<String>,
    admission: crate::planning::BatchAdmissionPolicy,
    campaign: String,
    batch: u64,
    replica: usize,
    slot: u64,
    total: u64,
}

#[derive(Clone, Serialize)]
pub(crate) struct BatchIdentity {
    #[serde(skip_serializing_if = "Option::is_none")]
    replay: Option<replay::BatchChoices>,
    campaign: String,
    number: u64,
    repeat: u64,
    concurrency: usize,
}

impl BatchIdentity {
    pub(super) fn number(&self) -> u64 {
        self.number
    }
    pub(crate) fn admission(&self) -> crate::planning::BatchAdmissionPolicy {
        crate::planning::BatchAdmissionPolicy::current()
    }
    pub(crate) fn execution(&self, replica: replica::Identity) -> ExecutionIdentity {
        ExecutionIdentity {
            replay_track: self
                .replay
                .as_ref()
                .map(|choices| choices.tracks[replica.index - 1]),
            replay_seed: self.replay.as_ref().and_then(|choices| choices.seed),
            replay_algorithm: self
                .replay
                .as_ref()
                .and_then(|choices| choices.algorithm.clone()),
            admission: self.admission(),
            campaign: self.campaign.clone(),
            batch: self.number,
            replica: replica.index,
            slot: (self.number - 1) * self.concurrency as u64 + replica.index as u64,
            total: self.repeat * self.concurrency as u64,
        }
    }
}

#[derive(Serialize)]
struct Slot {
    #[serde(skip_serializing_if = "Option::is_none")]
    replay_track: Option<u64>,
    id: u64,
    batch: u64,
    replica: usize,
    workspace: PathBuf,
    log: PathBuf,
    measurements: Option<PathBuf>,
    replay_input: Option<usize>,
}

#[derive(Serialize)]
struct ReplayInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    complete: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    track: Option<u64>,
    source: PathBuf,
    private: PathBuf,
    bytes: u64,
    sha256: String,
}

#[derive(Serialize)]
struct Plan<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    replay_selection: Option<crate::cassette::SelectionPlan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    recording: Option<PathBuf>,
    admission: crate::planning::BatchAdmissionPolicy,
    schema_version: u32,
    scope: &'static str,
    campaign: &'a str,
    source: PathBuf,
    configuration: &'a BenchmarkConfig,
    configuration_sha256: String,
    repeat: u64,
    concurrency: usize,
    mode: &'static str,
    respect_record_timings: bool,
    measurement_root: Option<PathBuf>,
    replay_inputs: Vec<ReplayInput>,
    slots: Vec<Slot>,
}

pub(super) fn run(
    config_path: &Path,
    config: BenchmarkConfig,
    repeat: u64,
    options: RunOptions<'_>,
    output: &mut dyn Write,
    warnings: &Mutex<BTreeSet<String>>,
) -> Result<(), Error> {
    // Reject unsupported combinations and excessive populations before any IO
    // publication or dependency/environment preparation.
    let total = repeat
        .checked_mul(config.concurrency as u64)
        .filter(|total| repeat > 0 && *total <= MAX_EXECUTIONS)
        .ok_or_else(|| {
            Error::Input("repeat must be positive and its concurrency product at most 65533".into())
        })?;
    if options.on_completion.is_some()
        || options.replica.index != 1
        || options.replica.total != 1
        || options.reused.is_some()
        || options.preparation_gate.is_some()
        || options.launch_gate.is_some()
        || options.batch.is_some()
        || options.recording.is_some()
    {
        return Err(Error::Input(
            "repeated execution cannot reuse legacy observers, slots or preparation gates".into(),
        ));
    }
    crate::planning::resolve_snapshot(&config, options.measurement_dir.is_some(), options.policy)?;
    if config
        .planning
        .as_ref()
        .is_some_and(|planning| planning.repetition != crate::planning::Repetition::default())
    {
        return Err(Error::Input("--repeat cannot combine with a nondefault planning.repetition; choose automated batches or manually identified configurations".into()));
    }
    let admission = crate::planning::BatchAdmissionPolicy::current();
    let (project_root, _) = crate::project::load(config_path)?;
    let project_root = project_root.canonicalize().map_err(io_error)?;
    let campaign = crate::workloads::TaskIdentity::local("execution-campaign", None).attempt_id;
    let measured = options.measurement_dir.is_some()
        || config
            .measurements
            .as_ref()
            .is_some_and(|value| value.enabled);
    let mut slots = planned_slots(&config, repeat, measured)?;
    let mut replay = replay::Prepared::open(config_path, &config, options, repeat)?;
    let replay_inputs = if let Some(replay) = &replay {
        replay.inputs(&mut slots)?
    } else {
        replay_inputs(config_path, &config, options, &mut slots)?
    };
    let parent = project_root.join("results/asb-executions");
    let root = parent.join(&campaign);
    let recording_destination = (options.mode == Some(crate::tape::Mode::Record))
        .then(|| {
            let destination = options
                .cassette
                .map(PathBuf::from)
                .unwrap_or_else(|| super::default_bundle_cassette(config_path, &config));
            super::recording::preflight(&destination)
        })
        .transpose()?;
    let measurement_root = measured
        .then(|| {
            options
                .measurement_dir
                .map(|path| std::path::absolute(path).map(|path| path.join(&campaign)))
                .unwrap_or_else(|| Ok(root.clone()))
        })
        .transpose()
        .map_err(io_error)?;
    let plan = Plan {
        replay_selection: replay.as_ref().and_then(|replay| replay.selection.clone()),
        recording: recording_destination.clone(),
        admission,
        schema_version: 1,
        scope: "intended-executions-not-attempt-evidence",
        campaign: &campaign,
        source: config_path.canonicalize().map_err(io_error)?,
        configuration: &config,
        configuration_sha256: crate::cassette::digest(
            &serde_json::to_vec(&config).map_err(|error| Error::Input(error.to_string()))?,
        ),
        repeat,
        concurrency: config.concurrency,
        mode: match options.mode {
            Some(crate::tape::Mode::Record) => "record",
            Some(crate::tape::Mode::Replay) => "replay",
            _ => "live",
        },
        respect_record_timings: options.respect_record_timings,
        measurement_root: measurement_root.clone(),
        replay_inputs,
        slots,
    };
    let bytes = encode(&plan)?;
    private_directory(&parent, true)?;
    private_directory(&root, false)?;
    let _owner = crate::output_lock::OutputLock::acquire(
        &root.join(".owner.lock"),
        "execution campaign already has an owner",
    )
    .map_err(Error::Run)?;
    publish_bytes(&root.join("plan.json"), &bytes)?;
    let recording = recording_destination.map(|destination| {
        Arc::new(super::recording::Recorder::new(
            destination,
            root.clone(),
            campaign.clone(),
            repeat,
            config.concurrency,
        ))
    });
    let private_cassette = if let Some(replay) = &mut replay {
        replay.stage(&root, &plan.replay_inputs)?;
        None
    } else {
        stage_replay(&root, &plan, config.concurrency)?
    };
    if let Some(measurement_root) = &measurement_root
        && measurement_root != &root
    {
        let parent = measurement_root
            .parent()
            .ok_or_else(|| Error::Input("invalid measurement root".into()))?;
        private_directory(parent, true)?;
        private_directory(measurement_root, false)?;
    }
    if options.json {
        serde_json::to_writer(
            &mut *output,
            &json!({"type":"execution-plan","path":root.join("plan.json"),"executions":total}),
        )
        .map_err(|error| Error::Io(error.to_string()))?;
        writeln!(output).map_err(io_error)?;
    } else {
        writeln!(
            output,
            "Execution plan: {} ({total} executions)",
            root.join("plan.json").display()
        )
        .map_err(io_error)?;
    }
    if let Some(selection) = &plan.replay_selection {
        if options.json {
            serde_json::to_writer(
                &mut *output,
                &json!({"type":"replay-selection","selection":selection}),
            )
            .map_err(|error| Error::Io(error.to_string()))?;
            writeln!(output).map_err(io_error)?;
        } else {
            writeln!(
                output,
                "Replay seed: {} ({})\nSelected track IDs: {}",
                selection.seed,
                selection.algorithm,
                selection
                    .tracks
                    .iter()
                    .map(u64::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .map_err(io_error)?;
        }
    }
    replay::disclose_incomplete(&plan.replay_inputs, options.json, output)?;
    let mut reference: Option<Compatibility> = replay
        .as_ref()
        .and_then(|replay| replay.compatibility.clone());
    let mut failures = 0u64;
    let mut completed_batches = 0u64;
    let mut stopped = false;
    let mut first_reason = None;
    let mut producers_quiescent = true;
    for number in 1..=repeat {
        let identity = BatchIdentity {
            replay: replay
                .as_ref()
                .map(|replay| replay.batch_choices(&plan, number, config.concurrency)),
            campaign: campaign.clone(),
            number,
            repeat,
            concurrency: config.concurrency,
        };
        let batch_root = root.join(format!("batch-{number}"));
        private_directory(&batch_root, false)?;
        private_directory(&batch_root.join("starts"), false)?;
        private_directory(&batch_root.join("outcomes"), false)?;
        private_directory(&batch_root.join("allocations"), false)?;
        let observer = Arc::new(Observer {
            root: root.clone(),
            directory: batch_root.clone(),
            identity: identity.clone(),
            completed: Mutex::new(BTreeSet::new()),
            cleanup_confirmed: AtomicBool::new(true),
        });
        let prepared = batch_root.join("prepared.json");
        let batch_recorder = recording.clone();
        let gate = batch::Gate::new(config.concurrency, reference.clone(), admission.material_timeout())?.observe(Arc::new(move |compatibility| {
            publish(&prepared, &json!({"schema_version":1,"scope":"verified-prepared-material","compatibility":compatibility}))?;
            if let Some(recorder) = &batch_recorder { recorder.prepare(compatibility,number)?; }
            Ok(())
        }));
        let mut selected = config.clone();
        selected
            .planning
            .get_or_insert_with(Default::default)
            .repetition = crate::planning::Repetition {
            index: number as usize,
            total: repeat as usize,
        };
        let mut policy =
            crate::planning::resolve_for_run(&selected, options.measurement_dir.is_some())?;
        if let Some(saved) = options.policy {
            policy.task_requirements = saved.task_requirements.clone();
        }
        let launch_gate = batch::Gate::deferred(config.concurrency, admission.launch_timeout())?;
        selected.workspace = path_text(&batch_root.join("workspace"))?;
        selected.log_file = path_text(&batch_root.join("run.jsonl"))?;
        let measures = measurement_root
            .as_ref()
            .map(|root| root.join(format!("batch-{number}/measurements")));
        if let Some(measurements) = selected.measurements.as_mut() {
            measurements.directory = path_text(
                measures
                    .as_deref()
                    .unwrap_or(&batch_root.join("measurements")),
            )?;
        }
        let override_measurements = options
            .measurement_dir
            .map(|_| path_text(measures.as_deref().expect("explicit override is measured")))
            .transpose()?;
        let batch_cassette = if recording.is_some() {
            Some(path_text(&super::recording::base(&root, number))?)
        } else if replay.is_some() {
            Some(replay::stage_batch(
                &root,
                &plan,
                number,
                config.concurrency,
            )?)
        } else {
            private_cassette.clone()
        };
        let result = super::run_loaded(
            config_path,
            selected,
            RunOptions {
                repeat: None,
                policy: Some(&policy),
                batch: Some(&identity),
                preparation_gate: Some(&gate),
                launch_gate: Some(&launch_gate),
                verify_initial_material: true,
                measurement_dir: override_measurements.as_deref(),
                cassette: batch_cassette.as_deref(),
                recording: recording.as_deref(),
                on_completion: Some(observer.as_ref()),
                ..options
            },
            output,
            warnings,
        );
        let prepared = gate.confirmed_identity();
        let completed = observer
            .completed
            .lock()
            .map_err(|_| Error::Run("execution completion state was interrupted".into()))?
            .len();
        let batch_failed = result.is_err();
        producers_quiescent &=
            completed == config.concurrency && observer.cleanup_confirmed.load(Ordering::Acquire);
        // A published outcome for every admitted slot distinguishes ordinary
        // completed failures from setup, output or evidence-publication failure.
        stopped = must_stop(
            &result,
            completed,
            config.concurrency,
            prepared.is_ok(),
            observer.cleanup_confirmed.load(Ordering::Acquire),
        );
        let reason = result
            .as_ref()
            .err()
            .map(|error| error.to_string().chars().take(4096).collect::<String>())
            .or_else(|| {
                stopped.then(|| {
                    "execution evidence or cleanup was not confirmed; inspect retained outcomes"
                        .into()
                })
            });
        if first_reason.is_none() {
            first_reason = reason.clone();
        }
        publish(
            &batch_root.join("finished.json"),
            &json!({"schema_version":1,"completed_executions":completed,"failed":batch_failed||stopped,"stopped":stopped,"failure_reason":reason}),
        )?;
        if batch_failed || stopped {
            failures += 1;
        }
        if stopped {
            break;
        }
        reference = Some(prepared?);
        completed_batches += 1;
    }
    let publication = recording
        .as_ref()
        .map(|recorder| recorder.finalize(producers_quiescent));
    let publication_status = match &publication {
        None => serde_json::Value::Null,
        Some(Ok(Some(published))) => {
            json!({"status":"published","format":"asb-zip-v1","path":published.destination,"tracks":published.manifest.tracks.len(),"cleanup_complete":published.cleanup_complete,"warnings":published.warnings})
        }
        Some(Ok(None)) => json!({"status":"not-created","reason":"no verified recording material"}),
        Some(Err(error)) => json!({"status":"failed","reason":error.to_string()}),
    };
    publish(
        &root.join("finished.json"),
        &json!({"schema_version":1,"completed_batches":completed_batches,"failed_batches":failures,"stopped":stopped,"cassette_publication":publication_status}),
    )?;
    if publication.is_some() {
        if options.json {
            serde_json::to_writer(
                &mut *output,
                &json!({"type":"cassette-publication","publication":publication_status}),
            )
            .map_err(|error| Error::Io(error.to_string()))?;
            writeln!(output).map_err(io_error)?;
        } else {
            match &publication {
                Some(Ok(Some(published))) => {
                    writeln!(
                        output,
                        "Cassette: {} (ZIP v1, {} tracks, published)",
                        published.destination.display(),
                        published.manifest.tracks.len()
                    )
                    .map_err(io_error)?;
                    for warning in &published.warnings {
                        writeln!(output, "Warning: {warning}").map_err(io_error)?;
                    }
                }
                Some(Ok(None)) => writeln!(
                    output,
                    "Recording was not created: no compatible workload material was prepared."
                )
                .map_err(io_error)?,
                Some(Err(_)) => writeln!(
                    output,
                    "Recording publication is not confirmed; partial evidence was retained."
                )
                .map_err(io_error)?,
                None => (),
            }
        }
    }
    if let Some(Err(error)) = publication {
        return Err(Error::Io(format!(
            "{}; {error}",
            first_reason
                .as_deref()
                .unwrap_or("recording publication failed")
        )));
    }
    if failures > 0 || stopped {
        Err(Error::Run(format!(
            "repeated execution had {failures} failed batch(es){}; {}; inspect {}",
            if stopped {
                " and stopped further admission"
            } else {
                ""
            },
            first_reason
                .as_deref()
                .unwrap_or("incomplete execution evidence"),
            root.join("plan.json").display()
        )))
    } else {
        Ok(())
    }
}

fn must_stop(
    result: &Result<(), Error>,
    completed: usize,
    selected: usize,
    prepared: bool,
    cleanup: bool,
) -> bool {
    let ordinary_failure =
        matches!(result, Err(Error::Run(_))) && completed == selected && prepared && cleanup;
    (result.is_err() && !ordinary_failure) || !prepared || !cleanup || completed != selected
}

fn planned_slots(
    config: &BenchmarkConfig,
    repeat: u64,
    measured: bool,
) -> Result<Vec<Slot>, Error> {
    let mut slots = Vec::new();
    for batch in 1..=repeat {
        let mut selected = config.clone();
        selected.workspace = format!("batch-{batch}/workspace");
        selected.log_file = format!("batch-{batch}/run.jsonl");
        for replica in 1..=config.concurrency {
            let selected = replica::configuration(
                &selected,
                replica::Identity {
                    index: replica,
                    total: config.concurrency,
                },
            )?;
            let measures = PathBuf::from(format!("batch-{batch}/measurements"));
            slots.push(Slot {
                replay_track: None,
                id: (batch - 1) * config.concurrency as u64 + replica as u64,
                batch,
                replica,
                workspace: selected.workspace.into(),
                log: selected.log_file.into(),
                measurements: measured.then(|| {
                    if config.concurrency == 1 {
                        measures
                    } else {
                        super::parallel_path(measures.to_str().expect("generated UTF-8"), replica)
                            .into()
                    }
                }),
                replay_input: None,
            });
        }
    }
    Ok(slots)
}

fn replay_inputs(
    config_path: &Path,
    config: &BenchmarkConfig,
    options: RunOptions<'_>,
    slots: &mut [Slot],
) -> Result<Vec<ReplayInput>, Error> {
    if options.mode != Some(crate::tape::Mode::Replay) {
        return Ok(Vec::new());
    }
    let base = options
        .cassette
        .map(PathBuf::from)
        .unwrap_or_else(|| super::default_cassette(config_path, config));
    let mut seen = BTreeMap::new();
    let mut inputs = Vec::new();
    let mut total = 0u64;
    let mut replicas = Vec::new();
    for replica in 1..=config.concurrency {
        let source = if config.concurrency == 1 {
            base.clone()
        } else {
            super::replica_cassette(crate::tape::Mode::Replay, &base, replica)
        };
        let source = source.canonicalize().map_err(|error| {
            Error::Input(format!(
                "cannot resolve replay input {}: {error}",
                source.display()
            ))
        })?;
        let id = if let Some(id) = seen.get(&source) {
            *id
        } else {
            let (bytes, sha256) = transfer_input(&source, &mut std::io::sink())?;
            total = total
                .checked_add(bytes)
                .filter(|total| *total <= 2 * 1024 * 1024 * 1024)
                .ok_or_else(|| Error::Input("replay inputs exceed 2 GiB aggregate bound".into()))?;
            let id = inputs.len();
            inputs.push(ReplayInput {
                complete: None,
                track: None,
                source: source.clone(),
                private: PathBuf::from(format!("inputs/recording-{}.jsonl", id + 1)),
                bytes,
                sha256,
            });
            seen.insert(source, id);
            id
        };
        replicas.push(id);
    }
    for slot in slots {
        slot.replay_input = Some(replicas[slot.replica - 1]);
    }
    Ok(inputs)
}

fn transfer_input(path: &Path, output: &mut dyn Write) -> Result<(u64, String), Error> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let mut file = options.open(path).map_err(io_error)?;
    let metadata = file.metadata().map_err(io_error)?;
    if !metadata.is_file() || metadata.len() > 64 * 1024 * 1024 {
        return Err(Error::Input(
            "legacy replay input must be a regular file at most 64 MiB".into(),
        ));
    }
    let mut hash = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(io_error)?;
        if count == 0 {
            break;
        }
        bytes += count as u64;
        if bytes > metadata.len() {
            return Err(Error::Input(
                "legacy replay input changed while reading".into(),
            ));
        }
        hash.update(&buffer[..count]);
        output.write_all(&buffer[..count]).map_err(io_error)?;
    }
    if bytes != metadata.len() {
        return Err(Error::Input(
            "legacy replay input changed while reading".into(),
        ));
    }
    Ok((bytes, format!("{:x}", hash.finalize())))
}

fn stage_replay(root: &Path, plan: &Plan<'_>, concurrency: usize) -> Result<Option<String>, Error> {
    if plan.replay_inputs.is_empty() {
        return Ok(None);
    }
    private_directory(&root.join("inputs"), false)?;
    for input in &plan.replay_inputs {
        let destination = root.join(&input.private);
        let pending = destination.with_extension("partial");
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&pending).map_err(io_error)?;
        let observed = transfer_input(&input.source, &mut file)?;
        if observed != (input.bytes, input.sha256.clone()) {
            return Err(Error::Input(
                "replay input changed after execution planning; no agent was started".into(),
            ));
        }
        file.sync_all()
            .and_then(|()| fs::hard_link(&pending, &destination))
            .map_err(io_error)?;
        fs::File::open(root.join("inputs"))
            .and_then(|file| file.sync_all())
            .map_err(io_error)?;
        fs::remove_file(pending).map_err(io_error)?;
    }
    let base = root.join("inputs/cassette.jsonl");
    for slot in plan.slots.iter().take(concurrency) {
        let source = &plan.replay_inputs[slot.replay_input.expect("replay slot")];
        let destination = if concurrency == 1 {
            base.clone()
        } else {
            super::replica_cassette(crate::tape::Mode::Record, &base, slot.replica)
        };
        fs::hard_link(root.join(&source.private), destination).map_err(io_error)?;
    }
    fs::File::open(root.join("inputs"))
        .and_then(|file| file.sync_all())
        .map_err(io_error)?;
    path_text(&base).map(Some)
}

struct Observer {
    root: PathBuf,
    directory: PathBuf,
    identity: BatchIdentity,
    completed: Mutex<BTreeSet<usize>>,
    cleanup_confirmed: AtomicBool,
}

impl replica::CompletionObserver for Observer {
    fn allocated(
        &self,
        replica: replica::Identity,
        reference: &crate::workloads::recovery::AttemptReference,
    ) -> Result<(), Error> {
        publish(
            &self
                .directory
                .join(format!("allocations/{}.json", replica.index)),
            &json!({"schema_version":1,"scope":"allocated-workload-reference-not-outcome","execution":self.identity.execution(replica),"reference":reference}),
        )
    }
    fn starting(
        &self,
        replica: replica::Identity,
        _: Option<&crate::measurements::ExecutionCohort>,
    ) -> Result<(), Error> {
        publish(
            &self
                .directory
                .join(format!("starts/{}.json", replica.index)),
            &json!({"schema_version":1,"scope":"execution-start-intent-not-actual-attempt","execution":self.identity.execution(replica)}),
        )
    }
    fn completed(&self, replica: replica::Identity, record: &Path) -> Result<(), Error> {
        let mut completed = self
            .completed
            .lock()
            .map_err(|_| Error::Run("execution completion state was interrupted".into()))?;
        let relative = record
            .strip_prefix(&self.root)
            .map_err(|_| Error::Input("execution outcome is outside campaign".into()))?;
        let mut file = fs::File::open(record)
            .map_err(io_error)?
            .take(MAX_OUTCOME_BYTES + 1);
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).map_err(io_error)?;
        if bytes.len() as u64 > MAX_OUTCOME_BYTES {
            return Err(Error::Input("execution outcome exceeds bounds".into()));
        }
        let value: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|_| Error::Input("invalid execution outcome".into()))?;
        let expected = self.identity.execution(replica);
        if value["summary"]["execution"]
            != serde_json::to_value(&expected).map_err(|error| Error::Input(error.to_string()))?
        {
            return Err(Error::Input(
                "execution outcome identity differs from planned slot".into(),
            ));
        }
        if !crate::workloads::ResourceDisposition::matches_record(
            value.get("workload_resource_disposition"),
            value["workload_cleanup_confirmed"] == true,
            &value["summary"],
        ) {
            return Err(Error::Input(
                "execution outcome resource disposition contradicts settlement evidence".into(),
            ));
        }
        publish(
            &self
                .directory
                .join(format!("outcomes/{}.json", replica.index)),
            &json!({"schema_version":1,"execution":expected,"path":relative,"bytes":bytes.len(),"sha256":crate::cassette::digest(&bytes),"task":value["summary"]["task"]}),
        )?;
        if value["workload_cleanup_confirmed"] != true || value["proxy_cleanup_confirmed"] != true {
            self.cleanup_confirmed.store(false, Ordering::Release);
        }
        completed.insert(replica.index);
        Ok(())
    }
}

fn path_text(path: &Path) -> Result<String, Error> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| Error::Input("execution paths must be UTF-8".into()))
}

fn private_directory(path: &Path, recursive: bool) -> Result<(), Error> {
    if recursive {
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_dir() => return Ok(()),
            Ok(_) => {
                return Err(Error::Input(
                    "execution parent must be a real directory".into(),
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let parent = path
                    .parent()
                    .ok_or_else(|| Error::Input("invalid execution parent".into()))?;
                private_directory(parent, true)?;
            }
            Err(error) => return Err(io_error(error)),
        }
    }
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path).map_err(io_error)?;
    fs::File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(io_error)?;
    if let Some(parent) = path.parent() {
        fs::File::open(parent)
            .and_then(|file| file.sync_all())
            .map_err(io_error)?;
    }
    Ok(())
}

fn encode(value: &impl Serialize) -> Result<Vec<u8>, Error> {
    let bytes = serde_json::to_vec(value).map_err(|error| Error::Input(error.to_string()))?;
    if bytes.len() > MAX_PLAN_BYTES {
        return Err(Error::Input("execution metadata exceeds 16 MiB".into()));
    }
    Ok(bytes)
}

fn publish(path: &Path, value: &impl Serialize) -> Result<(), Error> {
    publish_bytes(path, &encode(value)?)
}

pub(super) fn publish_bytes(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let directory = path
        .parent()
        .ok_or_else(|| Error::Input("execution metadata needs a parent".into()))?;
    let temporary = directory.join(format!(
        ".{}.pending",
        crate::workloads::TaskIdentity::local("execution-metadata", None).attempt_id
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary).map_err(io_error)?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .and_then(|()| fs::hard_link(&temporary, path))
        .map_err(io_error)?;
    fs::File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(io_error)?;
    fs::remove_file(temporary).map_err(io_error)?;
    fs::File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(io_error)
}

#[cfg(test)]
mod tests {
    use super::super::replica::CompletionObserver;
    use super::*;

    fn outcome_observer() -> (PathBuf, Observer, replica::Identity) {
        let root = std::env::temp_dir()
            .join(crate::workloads::TaskIdentity::local("repeat-settlement", None).attempt_id);
        private_directory(&root, false).unwrap();
        private_directory(&root.join("outcomes"), false).unwrap();
        let observer = Observer {
            root: root.clone(),
            directory: root.clone(),
            identity: BatchIdentity {
                replay: None,
                campaign: "fixture".into(),
                number: 1,
                repeat: 2,
                concurrency: 1,
            },
            completed: Mutex::new(BTreeSet::new()),
            cleanup_confirmed: AtomicBool::new(true),
        };
        (root, observer, replica::Identity::default())
    }

    #[test]
    fn grading_and_settlement_dispositions_are_independent_admission_evidence() {
        for (disposition, confirmed, summary_disposition, accepted, continue_batch) in [
            ("not-required", true, "not-required", true, true),
            ("removed", true, "removed", true, true),
            ("not-created", true, "not-created", true, true),
            ("stopped-preserved", true, "stopped-preserved", true, true),
            (
                "stopped-preserved",
                false,
                "stopped-preserved",
                false,
                false,
            ),
            ("stopped-preserved", true, "removed", false, false),
            ("unknown", true, "unknown", false, false),
        ] {
            let (root, observer, replica) = outcome_observer();
            let value = json!({"summary":{"execution":observer.identity.execution(replica),
                "task":{"attempt_id":"failed-attempt"},"workload_success":null,
                "workload_failure":{"kind":"grader_error"},"workload_resource_disposition":summary_disposition},
                "workload_resource_disposition":disposition,"workload_cleanup_confirmed":confirmed,"proxy_cleanup_confirmed":true});
            publish(&root.join("record.json"), &value).unwrap();
            assert_eq!(
                observer
                    .completed(replica, &root.join("record.json"))
                    .is_ok(),
                accepted,
                "{disposition}"
            );
            assert_eq!(
                !must_stop(
                    &Err(Error::Run("grader failure".into())),
                    observer.completed.lock().unwrap().len(),
                    1,
                    true,
                    observer.cleanup_confirmed.load(Ordering::Acquire)
                ),
                continue_batch
            );
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn unconfirmed_cleanup_preserves_outcome_but_stops_new_admission() {
        for field in ["workload_cleanup_confirmed", "proxy_cleanup_confirmed"] {
            let root = std::env::temp_dir()
                .join(crate::workloads::TaskIdentity::local("repeat-cleanup", None).attempt_id);
            private_directory(&root, false).unwrap();
            private_directory(&root.join("outcomes"), false).unwrap();
            let observer = Observer {
                root: root.clone(),
                directory: root.clone(),
                identity: BatchIdentity {
                    replay: None,
                    campaign: "fixture".into(),
                    number: 1,
                    repeat: 2,
                    concurrency: 1,
                },
                completed: Mutex::new(BTreeSet::new()),
                cleanup_confirmed: AtomicBool::new(true),
            };
            let replica = replica::Identity::default();
            let mut outcome = json!({"summary":{"execution":observer.identity.execution(replica),"task":{"attempt_id":"graded-fixture"}},"workload_cleanup_confirmed":true,"proxy_cleanup_confirmed":true});
            outcome[field] = json!(false);
            publish(&root.join("record.json"), &outcome).unwrap();
            observer
                .completed(replica, &root.join("record.json"))
                .unwrap();
            assert_eq!(observer.completed.lock().unwrap().len(), 1);
            assert!(root.join("outcomes/1.json").exists());
            assert!(must_stop(
                &Err(Error::Run("cleanup failure".into())),
                1,
                1,
                true,
                observer.cleanup_confirmed.load(Ordering::Acquire)
            ));
            assert!(must_stop(
                &Err(Error::Io("output failure".into())),
                1,
                1,
                true,
                true
            ));
            assert!(!must_stop(
                &Err(Error::Run("ordinary graded failure".into())),
                1,
                1,
                true,
                true
            ));
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn metadata_publication_never_replaces_and_legacy_input_size_is_bounded() {
        let root = std::env::temp_dir()
            .join(crate::workloads::TaskIdentity::local("repeat-metadata", None).attempt_id);
        private_directory(&root, false).unwrap();
        let destination = root.join("plan.json");
        publish(&destination, &json!({"original":true})).unwrap();
        let bytes = fs::read(&destination).unwrap();
        assert!(publish(&destination, &json!({"original":false})).is_err());
        assert_eq!(fs::read(&destination).unwrap(), bytes);
        let large = root.join("large.jsonl");
        fs::File::create(&large)
            .unwrap()
            .set_len(64 * 1024 * 1024 + 1)
            .unwrap();
        assert!(transfer_input(&large, &mut std::io::sink()).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
