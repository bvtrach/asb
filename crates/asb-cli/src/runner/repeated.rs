//! Fresh sequential batches with a durable intended-slot plan and outcome links.
mod legacy;
mod replay;
pub(crate) use super::campaign::ExecutionIdentity;
use super::campaign::{MAX_EXECUTIONS, Plan, ReplayInput, Slot, planned_slots};
use super::{RunOptions, batch, replica};
use crate::{Error, benchmark::BenchmarkConfig, cassette::Compatibility, io_error};
use serde::Serialize;
use serde_json::json;
use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const MAX_PLAN_BYTES: usize = 16 * 1024 * 1024;
const MAX_OUTCOME_BYTES: u64 = 1024 * 1024;

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

pub(super) fn run(
    config_path: &Path,
    config: BenchmarkConfig,
    repeat: u64,
    options: RunOptions<'_>,
    output: &mut dyn Write,
    warnings: &Mutex<BTreeSet<String>>,
) -> Result<(), Error> {
    activate(
        Prepared::prepare(config_path, config, repeat, options, None)?,
        output,
        warnings,
    )
}

/// Own frozen planning facts and opened sources, never an active process/lock.
struct Prepared {
    config_path: PathBuf,
    options: ActivationOptions,
    parent: PathBuf,
    root: PathBuf,
    plan: Plan,
    bytes: Vec<u8>,
    replay: Option<PreparedReplay>,
}

enum PreparedReplay {
    Selected(Box<replay::Prepared>),
    Legacy(legacy::Prepared),
}

/// Only presentation and saved policy survive preparation as execution options.
/// Routing/counts/timings come from the owned plan/source, not parallel copies.
struct ActivationOptions {
    json: bool,
    show_output: bool,
    measurement_override: bool,
    policy: Option<crate::planning::ResolvedPlan>,
}

impl Prepared {
    fn prepare(
        config_path: &Path,
        config: BenchmarkConfig,
        repeat: u64,
        options: RunOptions<'_>,
        campaign_parent: Option<&Path>,
    ) -> Result<Self, Error> {
        // Reject unsupported combinations and excessive populations before any IO
        // publication or dependency/environment preparation.
        repeat
            .checked_mul(config.concurrency as u64)
            .filter(|total| repeat > 0 && *total <= MAX_EXECUTIONS)
            .ok_or_else(|| {
                Error::Input(
                    "repeat must be positive and its concurrency product at most 65533".into(),
                )
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
                "repeated execution cannot reuse legacy observers, slots or preparation gates"
                    .into(),
            ));
        }
        crate::planning::resolve_snapshot(
            &config,
            options.measurement_dir.is_some(),
            options.policy,
        )?;
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
        let (replay, replay_inputs) =
            match replay::Prepared::open(config_path, &config, options, repeat)? {
                Some(selected) => {
                    let inputs = selected.inputs(&mut slots)?;
                    (Some(PreparedReplay::Selected(Box::new(selected))), inputs)
                }
                None if options.mode == Some(crate::tape::Mode::Replay) => {
                    let (legacy, inputs) =
                        legacy::Prepared::open(config_path, &config, options, &mut slots)?;
                    (Some(PreparedReplay::Legacy(legacy)), inputs)
                }
                None => (None, Vec::new()),
            };
        let parent = campaign_parent
            .map(|path| std::path::absolute(path).map_err(io_error))
            .transpose()?
            .unwrap_or_else(|| project_root.join("results/asb-executions"));
        let root = parent.join(&campaign);
        let recording_destination = (options.mode == Some(crate::tape::Mode::Record))
            .then(|| {
                let destination = options.cassette.map(PathBuf::from).unwrap_or_else(|| {
                    if campaign_parent.is_some() {
                        root.join("recording.asb.zip")
                    } else {
                        super::default_bundle_cassette(config_path, &config)
                    }
                });
                super::recording::inspect_destination(&destination)
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
            replay_selection: replay.as_ref().and_then(|replay| match replay {
                PreparedReplay::Selected(selected) => selected.selection.clone(),
                PreparedReplay::Legacy(_) => None,
            }),
            recording: recording_destination.clone(),
            admission,
            schema_version: 1,
            scope: "intended-executions-not-attempt-evidence".into(),
            campaign: campaign.clone(),
            source: config_path.canonicalize().map_err(io_error)?,
            configuration_sha256: crate::cassette::digest(
                &serde_json::to_vec(&config).map_err(|error| Error::Input(error.to_string()))?,
            ),
            repeat,
            concurrency: config.concurrency,
            configuration: config,
            mode: match options.mode {
                Some(crate::tape::Mode::Record) => crate::planning::ExecutionMode::Record,
                Some(crate::tape::Mode::Replay) => crate::planning::ExecutionMode::Replay,
                _ => crate::planning::ExecutionMode::Live,
            },
            respect_record_timings: options.respect_record_timings,
            measurement_root: measurement_root.clone(),
            replay_inputs,
            slots,
        };
        plan.validate_layout()?;
        let bytes = encode(&plan)?;
        Ok(Self {
            // Anchor the invocation without following its final symlink: its
            // project authority can differ from the canonical source location.
            config_path: std::path::absolute(config_path).map_err(io_error)?,
            options: ActivationOptions {
                json: options.json,
                show_output: options.show_output,
                measurement_override: options.measurement_dir.is_some(),
                policy: options.policy.cloned(),
            },
            parent,
            root,
            plan,
            bytes,
            replay,
        })
    }
}

fn activate(
    prepared: Prepared,
    output: &mut dyn Write,
    warnings: &Mutex<BTreeSet<String>>,
) -> Result<(), Error> {
    execute(publish_campaign(prepared)?, output, warnings)
}

/// Published layout/input ownership is not evidence of an attempted execution.
struct PublishedCampaign {
    config_path: PathBuf,
    options: ActivationOptions,
    root: PathBuf,
    plan: Plan,
    private_cassette: Option<String>,
    replay: Option<Box<replay::Prepared>>,
    _owner: crate::output_lock::OutputLock,
}

fn publish_campaign(prepared: Prepared) -> Result<PublishedCampaign, Error> {
    let Prepared {
        config_path,
        options,
        parent,
        root,
        plan,
        bytes,
        replay,
    } = prepared;
    plan.validate_layout()?;
    // Inspection never reserved the output. Recheck before creating a campaign
    // or starting any dependency; the journal remains the final no-clobber owner.
    if let Some(destination) = &plan.recording {
        let observed = super::recording::inspect_destination(destination)?;
        if observed != *destination {
            return Err(Error::Input(
                "recording destination changed after planning".into(),
            ));
        }
    }
    private_directory(&parent, true)?;
    private_directory(&root, false)?;
    let _owner = crate::output_lock::OutputLock::acquire(
        &root.join(".owner.lock"),
        "execution campaign already has an owner",
    )
    .map_err(Error::Run)?;
    // Root-contained cassettes must not create/adopt the root ahead of its
    // exclusive owner. Inspection was not a reservation; recheck after owning it.
    if let Some(destination) = &plan.recording {
        let observed = super::recording::preflight(destination)?;
        if observed != *destination {
            return Err(Error::Input(
                "recording destination changed during publication".into(),
            ));
        }
    }
    publish_bytes(&root.join("plan.json"), &bytes)?;
    let (private_cassette, replay) = match replay {
        Some(PreparedReplay::Selected(mut selected)) => {
            selected.stage(&root, &plan.replay_inputs)?;
            (None, Some(selected))
        }
        Some(PreparedReplay::Legacy(legacy)) => (legacy.stage(&root, &plan)?, None),
        None => (None, None),
    };
    if let Some(measurement_root) = &plan.measurement_root
        && measurement_root != &root
    {
        let parent = measurement_root
            .parent()
            .ok_or_else(|| Error::Input("invalid measurement root".into()))?;
        private_directory(parent, true)?;
        private_directory(measurement_root, false)?;
    }
    Ok(PublishedCampaign {
        config_path,
        options,
        root,
        plan,
        private_cassette,
        replay,
        _owner,
    })
}

fn execute(
    published: PublishedCampaign,
    output: &mut dyn Write,
    warnings: &Mutex<BTreeSet<String>>,
) -> Result<(), Error> {
    let PublishedCampaign {
        config_path,
        options,
        root,
        plan,
        private_cassette,
        replay,
        _owner,
    } = published;
    let config = &plan.configuration;
    let repeat = plan.repeat;
    let total = plan.slots.len();
    let admission = plan.admission;
    let campaign = plan.campaign.clone();
    let measurement_root = plan.measurement_root.clone();
    let mode = match plan.mode {
        crate::planning::ExecutionMode::Live => None,
        crate::planning::ExecutionMode::Record => Some(crate::tape::Mode::Record),
        crate::planning::ExecutionMode::Replay => Some(crate::tape::Mode::Replay),
        crate::planning::ExecutionMode::Unknown => {
            return Err(Error::Input(
                "unknown campaign execution mode cannot be activated".into(),
            ));
        }
    };
    let recording = plan.recording.as_ref().map(|destination| {
        Arc::new(super::recording::Recorder::new(
            destination.clone(),
            root.clone(),
            campaign.clone(),
            repeat,
            config.concurrency,
        ))
    });
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
        let mut policy = crate::planning::resolve_for_run(&selected, options.measurement_override)?;
        if let Some(saved) = &options.policy {
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
            .measurement_override
            .then(|| path_text(measures.as_deref().expect("explicit override is measured")))
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
            &config_path,
            selected,
            RunOptions {
                json: options.json,
                show_output: options.show_output,
                mode,
                respect_record_timings: plan.respect_record_timings,
                policy: Some(&policy),
                batch: Some(&identity),
                preparation_gate: Some(&gate),
                launch_gate: Some(&launch_gate),
                verify_initial_material: true,
                measurement_dir: override_measurements.as_deref(),
                cassette: batch_cassette.as_deref(),
                recording: recording.as_deref(),
                on_completion: Some(observer.as_ref()),
                ..Default::default()
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
        Some(Ok(None)) => {
            json!({"status":"not-created","reason":"no verified recording material"})
        }
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
            &super::campaign::OutcomeReference {
                schema_version: 1,
                execution: expected,
                path: relative.to_path_buf(),
                bytes: bytes.len() as u64,
                sha256: crate::cassette::digest(&bytes),
                task: value["summary"]["task"].clone(),
            },
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

    fn planning_fixture() -> (PathBuf, BenchmarkConfig, PathBuf) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/campaign-planning-fixtures")
            .join(crate::workloads::TaskIdentity::local("planning", None).attempt_id);
        fs::create_dir_all(root.join(".asb")).unwrap();
        let root = root.canonicalize().unwrap();
        fs::write(
            root.join(".asb/project.toml"),
            toml::to_string(&crate::project::ProjectConfig::default()).unwrap(),
        )
        .unwrap();
        let config: BenchmarkConfig = toml::from_str(
            "agent='pi'\nprovider='openrouter'\nmodel='fixture'\nworkload='bug-fix'\nconcurrency=2",
        )
        .unwrap();
        let config_path = root.join("benchmark.toml");
        fs::write(&config_path, toml::to_string(&config).unwrap()).unwrap();
        (root, config, config_path)
    }

    #[test]
    fn published_internal_recording_owns_root_without_starting_a_journal_or_client() {
        let (root, config, path) = planning_fixture();
        let parent = root.join("isolated/campaigns");
        let prepared = Prepared::prepare(
            &path,
            config.clone(),
            2,
            RunOptions {
                mode: Some(crate::tape::Mode::Record),
                ..Default::default()
            },
            Some(&parent),
        )
        .unwrap();
        assert_eq!(
            prepared.plan.recording.as_ref().unwrap(),
            &prepared.root.join("recording.asb.zip")
        );
        assert!(!parent.exists());
        let published = publish_campaign(prepared).unwrap();
        let campaign = published.root.clone();
        let lock = campaign.join(".owner.lock");
        assert!(campaign.join("plan.json").is_file());
        assert!(!campaign.join("recording.asb.zip").exists());
        assert!(!campaign.join(".recording.asb.zip.partial").exists());
        assert!(!campaign.join("batch-1").exists());
        assert!(crate::output_lock::OutputLock::acquire(&lock, "busy").is_err());
        drop(published);
        let owner = crate::output_lock::OutputLock::acquire(&lock, "busy").unwrap();
        assert!(campaign.join("plan.json").is_file());
        drop(owner);

        struct BrokenOutput(PathBuf);
        impl Write for BrokenOutput {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                assert!(crate::output_lock::OutputLock::acquire(&self.0, "busy").is_err());
                Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "publication-only fixture output closed",
                ))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let prepared = Prepared::prepare(
            &path,
            config,
            1,
            RunOptions {
                mode: Some(crate::tape::Mode::Record),
                ..Default::default()
            },
            Some(&parent),
        )
        .unwrap();
        let campaign = prepared.root.clone();
        let published = publish_campaign(prepared).unwrap();
        let error = execute(
            published,
            &mut BrokenOutput(campaign.join(".owner.lock")),
            &Mutex::new(BTreeSet::new()),
        )
        .unwrap_err();
        assert!(error.to_string().contains("fixture output closed"));
        assert!(!campaign.join("batch-1").exists());
        assert!(!campaign.join(".recording.asb.zip.partial").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn later_publication_failure_retains_earlier_plan_and_cannot_execute_any_campaign() {
        let (root, config, path) = planning_fixture();
        let parent = root.join("isolated/campaigns");
        let first = Prepared::prepare(
            &path,
            config.clone(),
            1,
            RunOptions {
                mode: Some(crate::tape::Mode::Record),
                ..Default::default()
            },
            Some(&parent),
        )
        .unwrap();
        let first_root = first.root.clone();
        let second = Prepared::prepare(
            &path,
            config.clone(),
            1,
            RunOptions {
                mode: Some(crate::tape::Mode::Record),
                ..Default::default()
            },
            Some(&parent),
        )
        .unwrap();
        fs::create_dir_all(&second.root).unwrap();
        let foreign = second.plan.recording.as_ref().unwrap().clone();
        fs::write(&foreign, b"foreign cassette").unwrap();
        let second_root = second.root.clone();
        let executions = std::cell::Cell::new(0);
        let result = (|| -> Result<(), Error> {
            let all = [first, second]
                .into_iter()
                .map(publish_campaign)
                .collect::<Result<Vec<_>, _>>()?;
            for published in all {
                executions.set(executions.get() + 1);
                execute(published, &mut Vec::new(), &Mutex::new(BTreeSet::new()))?;
            }
            Ok(())
        })();
        assert!(result.unwrap_err().to_string().contains("already exists"));
        assert_eq!(executions.get(), 0);
        assert!(first_root.join("plan.json").is_file());
        assert!(!first_root.join("batch-1").exists());
        assert!(!first_root.join(".recording.asb.zip.partial").exists());
        assert!(!second_root.join("plan.json").exists());
        assert_eq!(fs::read(foreign).unwrap(), b"foreign cassette");
        let owner =
            crate::output_lock::OutputLock::acquire(&first_root.join(".owner.lock"), "busy")
                .unwrap();
        drop(owner);

        let prepared =
            Prepared::prepare(&path, config, 1, RunOptions::default(), Some(&parent)).unwrap();
        let foreign_root = prepared.root.clone();
        fs::create_dir_all(&foreign_root).unwrap();
        fs::write(foreign_root.join("sentinel"), b"untouched").unwrap();
        assert!(publish_campaign(prepared).is_err());
        assert_eq!(
            fs::read(foreign_root.join("sentinel")).unwrap(),
            b"untouched"
        );
        assert!(!foreign_root.join("plan.json").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn campaign_planning_has_no_output_side_effects_and_activation_rechecks_destination() {
        let (root, config, config_path) = planning_fixture();
        let parent = root.join("isolated/sweep-configuration/campaigns");
        let destination = root.join("recordings/new/trials.asb.zip");
        let (prepared, saved_policy) = {
            let cassette = destination.to_string_lossy().into_owned();
            let measurements = root
                .join("external-measures")
                .to_string_lossy()
                .into_owned();
            let policy = crate::planning::resolve_for_run(&config, true).unwrap();
            let prepared = Prepared::prepare(
                &config_path,
                config.clone(),
                3,
                RunOptions {
                    mode: Some(crate::tape::Mode::Record),
                    cassette: Some(&cassette),
                    measurement_dir: Some(&measurements),
                    policy: Some(&policy),
                    ..Default::default()
                },
                Some(&parent),
            )
            .unwrap();
            (prepared, policy)
        }; // Temporary option strings are gone; policy is no longer borrowed.
        assert_eq!(prepared.options.policy.as_ref(), Some(&saved_policy));
        assert!(prepared.options.measurement_override);
        assert_eq!(
            prepared.plan.measurement_root.as_ref().unwrap(),
            &root.join("external-measures").join(&prepared.plan.campaign)
        );
        assert!(!root.join("external-measures").exists());
        assert_eq!(prepared.plan.validate_layout().unwrap(), 6);
        assert_eq!(prepared.root.parent().unwrap(), parent);
        assert_eq!(prepared.config_path, config_path);
        assert!(prepared.config_path.is_absolute());
        assert_eq!(prepared.plan.configuration, config);
        assert_eq!(prepared.plan.recording.as_ref().unwrap(), &destination);
        assert!(!parent.exists());
        assert!(!root.join("recordings").exists());
        assert!(!root.join("results").exists());
        #[cfg(unix)]
        {
            let other = root.join("other-project");
            fs::create_dir_all(other.join(".asb")).unwrap();
            fs::copy(
                root.join(".asb/project.toml"),
                other.join(".asb/project.toml"),
            )
            .unwrap();
            let other_config = other.join("benchmark.toml");
            fs::copy(&config_path, &other_config).unwrap();
            let alias = root.join("linked-benchmark.toml");
            std::os::unix::fs::symlink(&other_config, &alias).unwrap();
            let linked =
                Prepared::prepare(&alias, config.clone(), 1, RunOptions::default(), None).unwrap();
            assert_eq!(linked.config_path, alias);
            assert_eq!(linked.plan.source, other_config);
            assert_eq!(linked.parent, root.join("results/asb-executions"));
        }
        let planned_root = prepared.root.clone();
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(&destination, b"foreign cassette created after planning").unwrap();
        let error = activate(prepared, &mut Vec::new(), &Mutex::new(BTreeSet::new())).unwrap_err();
        assert!(error.to_string().contains("already exists"));
        assert!(!planned_root.exists());
        assert!(!parent.exists());
        assert_eq!(
            fs::read(&destination).unwrap(),
            b"foreign cassette created after planning"
        );
        let live = Prepared::prepare(&config_path, config, 2, RunOptions::default(), None).unwrap();
        assert_eq!(live.plan.validate_layout().unwrap(), 4);
        assert_eq!(live.parent, root.join("results/asb-executions"));
        assert!(!live.root.exists());
        drop(live);
        assert!(!root.join("results").exists());
        fs::remove_dir_all(root).unwrap();
    }

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
        assert!(legacy::transfer_input(&large, &mut std::io::sink()).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
