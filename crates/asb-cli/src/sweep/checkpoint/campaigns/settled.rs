//! Strict immutable per-slot settlement, separate from reporting fallbacks.
use super::super::{Fingerprint, Inputs, Root, completed, resume_error};
use crate::runner::campaign::{ExecutionIdentity, Slot, StartIntent, reading::Layout};
use crate::sweep::plan::{PlannedBenchmark, SweepPlan};
use crate::workloads::{contracts::EnvironmentKind, recovery::AttemptReference};
use crate::{Error, io_error};
use completed::{OutcomeBatch, OutcomeContext, OutcomeEvidence};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const METADATA: u64 = 64 * 1024;
const SCOPE: &str = "settled-campaign-slot-declared-evidence";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Prepared {
    schema_version: u32,
    scope: String,
    compatibility: crate::cassette::Compatibility,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Allocation {
    schema_version: u32,
    scope: String,
    execution: ExecutionIdentity,
    reference: AttemptReference,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema_version: u32,
    scope: String,
    configuration: usize,
    execution: ExecutionIdentity,
    input_digest: String,
    roots: Vec<Root>,
    fingerprint: Fingerprint,
}

struct Candidate {
    configuration: usize,
    slot: u64,
    execution: ExecutionIdentity,
    roots: Vec<Root>,
}

struct Record {
    configuration: usize,
    slot: usize,
    path: PathBuf,
    value: Value,
}

struct SlotOwner<'a> {
    layout: &'a Layout,
    slot: &'a Slot,
    entry: &'a PlannedBenchmark,
    invocation: &'a Path,
}

impl Inputs {
    /// Only called after execution workers and recording producers have joined.
    /// A missing receipt never changes an independently computed fresh grade.
    pub(in crate::sweep) fn seal_campaigns(
        &self,
        parent: &Path,
        results: &Path,
        plan: &SweepPlan,
    ) -> Result<(), Error> {
        self.seal_campaigns_with(
            parent,
            results,
            plan,
            crate::workloads::recovery::verify_retained,
        )
    }

    #[cfg(test)]
    pub(in crate::sweep::checkpoint) fn seal_campaigns_fixture(
        &self,
        parent: &Path,
        results: &Path,
        plan: &SweepPlan,
        verify: impl Fn(&crate::workloads::recovery::RetainedOutcomeContext<'_>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.seal_campaigns_with(parent, results, plan, verify)
    }

    fn seal_campaigns_with(
        &self,
        parent: &Path,
        results: &Path,
        plan: &SweepPlan,
        verify: impl Fn(&crate::workloads::recovery::RetainedOutcomeContext<'_>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.verify_campaign_reservations(results, plan)?;
        let mut layouts = plan.read_campaigns(results)?;
        let budget = std::rc::Rc::new(std::cell::Cell::new(256 * 1024 * 1024));
        for layout in &mut layouts {
            layout.share_observation_budget(budget.clone());
        }
        preflight(&layouts)?;
        let mut records = Vec::new();
        for (configuration, layout) in layouts.iter().enumerate() {
            for (index, slot) in layout.plan.slots.iter().enumerate() {
                if let Some(outcome) = layout.outcome(slot)? {
                    records.push(Record {
                        configuration,
                        slot: index,
                        path: outcome.path,
                        value: outcome.value,
                    });
                }
            }
        }
        // All declarations pass the common strict policy before one grouped
        // recording proof. No report-only fallback can enter this batch.
        let mut declarations = OutcomeBatch::declare(records.iter().map(|record| {
            (
                &record.value,
                OutcomeContext::from(&plan.entries[record.configuration]),
            )
        }))?;
        let mut prepared = BTreeMap::new();
        let mut identities = BTreeMap::new();
        let mut attempts = BTreeSet::new();
        let mut candidates = Vec::new();
        let mut eligible = BTreeSet::new();
        for (index, evidence) in declarations.declarations() {
            let record = &records[index];
            let layout = &layouts[record.configuration];
            let slot = &layout.plan.slots[record.slot];
            if (layout.plan.mode == crate::planning::ExecutionMode::Record)
                != evidence.recording.is_some()
            {
                continue;
            }
            if !attempts.insert(evidence.task.attempt_id.clone()) {
                return Err(resume_error("campaign outcomes contain duplicate attempts"));
            }
            let prepared_key = (record.configuration, slot.batch);
            if let std::collections::btree_map::Entry::Vacant(entry) = prepared.entry(prepared_key)
            {
                let value: Prepared = serde_json::from_value(layout.declaration(
                    &PathBuf::from(format!("batch-{}/prepared.json", slot.batch)),
                    METADATA,
                )?)
                .map_err(|_| resume_error("prepared campaign identity is malformed"))?;
                value
                    .compatibility
                    .validate()
                    .map_err(|error| resume_error(&error.to_string()))?;
                if value.schema_version != 1 || value.scope != "verified-prepared-material" {
                    return Err(resume_error(
                        "prepared campaign identity has an unknown protocol",
                    ));
                }
                if let Some(expected) = layout
                    .plan
                    .replay_compatibility
                    .as_ref()
                    .or_else(|| identities.get(&record.configuration))
                {
                    expected
                        .require_match(&value.compatibility)
                        .map_err(|error| resume_error(&error.to_string()))?;
                }
                identities
                    .entry(record.configuration)
                    .or_insert_with(|| value.compatibility.clone());
                entry.insert(value.compatibility);
            }
            let compatibility = &prepared[&prepared_key];
            let roots = slot_roots(
                SlotOwner {
                    layout,
                    slot,
                    entry: &plan.entries[record.configuration],
                    invocation: &parent.join(&plan.entries[record.configuration].reference),
                },
                &record.path,
                evidence,
                compatibility,
                &verify,
            )?;
            candidates.push(Candidate {
                configuration: record.configuration,
                slot: slot.id,
                execution: layout.plan.execution(slot),
                roots,
            });
            eligible.insert(index);
        }
        declarations.retain_candidates(&eligible);
        let unsettled = declarations.unsettled().to_vec();
        let _validated = declarations.verify()?;
        // Finish every read-only validation before the first receipt write.
        for candidate in candidates {
            let context = json!({"protocol":"asb-campaign-completion-v1","scope":SCOPE,
                "input_digest":self.initial.digest,"configuration":candidate.configuration,
                "execution":candidate.execution});
            let fingerprint =
                self.snapshotter
                    .fingerprint(&candidate.roots, &context, false, &[])?;
            let path = self
                .directory
                .join("campaign-completed")
                .join(format!("{:06}", candidate.configuration))
                .join(format!("{:06}.json", candidate.slot));
            completed::publish(
                &path,
                &Receipt {
                    schema_version: 1,
                    scope: SCOPE.into(),
                    configuration: candidate.configuration,
                    execution: candidate.execution,
                    input_digest: self.initial.digest.clone(),
                    roots: candidate.roots,
                    fingerprint,
                },
            )?;
        }
        for index in &unsettled {
            let record = &records[*index];
            let layout = &layouts[record.configuration];
            let slot = &layout.plan.slots[record.slot];
            completed::publish(
                &self
                    .directory
                    .join("campaign-unsettled")
                    .join(format!("{:06}/{:06}.json", record.configuration, slot.id)),
                &json!({"schema_version":1,"scope":"unsealed-slot-recovery-required-not-retry-permission",
                    "configuration":record.configuration,"execution":layout.plan.execution(slot),
                    "input_digest":self.initial.digest,"outcome":record.path}),
            )?;
        }
        if unsettled.is_empty() {
            Ok(())
        } else {
            Err(resume_error(
                "some campaign slots require recovery; completed peers were sealed without retry",
            ))
        }
    }
}

fn preflight(layouts: &[Layout]) -> Result<(), Error> {
    let mut total = 0u64;
    for layout in layouts {
        total = total
            .checked_add(layout.preflight()?)
            .ok_or_else(|| resume_error("settlement size overflow"))?;
        let mut leaves = BTreeSet::new();
        for slot in &layout.plan.slots {
            leaves.insert(format!("batch-{}/starts/{}.json", slot.batch, slot.replica));
            leaves.insert(format!(
                "batch-{}/allocations/{}.json",
                slot.batch, slot.replica
            ));
            leaves.insert(format!("batch-{}/prepared.json", slot.batch));
        }
        for leaf in leaves {
            match std::fs::symlink_metadata(layout.root.join(leaf)) {
                Ok(metadata) if metadata.is_file() && metadata.len() <= METADATA => {
                    total = total
                        .checked_add(metadata.len())
                        .ok_or_else(|| resume_error("settlement size overflow"))?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(io_error(error)),
                _ => {
                    return Err(resume_error(
                        "settlement declaration is not a bounded regular file",
                    ));
                }
            }
        }
        if total > 256 * 1024 * 1024 {
            return Err(resume_error(
                "aggregate settlement declarations exceed 256 MiB",
            ));
        }
    }
    Ok(())
}

fn slot_roots(
    owner: SlotOwner<'_>,
    path: &Path,
    evidence: OutcomeEvidence<'_>,
    compatibility: &crate::cassette::Compatibility,
    verify: &impl Fn(&crate::workloads::recovery::RetainedOutcomeContext<'_>) -> Result<(), Error>,
) -> Result<Vec<Root>, Error> {
    let SlotOwner {
        layout,
        slot,
        entry,
        invocation,
    } = owner;
    let structured = crate::workloads::outcome::BenchmarkOutcome::read(evidence.summary)
        .map_err(|_| resume_error("malformed outcome protocol"))?;
    let policy = structured
        .as_ref()
        .map(|outcome| &outcome.policy)
        .cloned()
        .unwrap_or_default();
    if compatibility.agent != entry.configuration.agent
        || compatibility.provider != entry.configuration.provider
        || compatibility.model != entry.configuration.model
        || compatibility.workload != evidence.task.workload
        || compatibility.task != evidence.task.task_id
        || compatibility.dataset_sha256
            != crate::runner::dataset_fingerprint(evidence.task.dataset.as_ref())?
        || compatibility.protocol_sha256
            != Some(crate::runner::replay_protocol_fingerprint(
                entry.selection.module.as_ref(),
                &policy,
            )?)
    {
        return Err(resume_error(
            "prepared material identity differs from actual configuration/task/protocol",
        ));
    }
    let execution = layout.plan.execution(slot);
    let replica = crate::runner::replica::Identity {
        index: slot.replica,
        total: layout.plan.concurrency,
    };
    let start_path = PathBuf::from(format!("batch-{}/starts/{}.json", slot.batch, slot.replica));
    let start: StartIntent = serde_json::from_value(layout.declaration(&start_path, METADATA)?)
        .map_err(|_| resume_error("campaign start is malformed"))?;
    let cohort = start
        .execution_cohort
        .as_ref()
        .ok_or_else(|| resume_error("campaign start has unknown dispatch provenance"))?;
    if start.schema_version != 1
        || start.scope != "execution-start-intent-not-actual-attempt"
        || start.execution != execution
        || !cohort.admits(slot.replica, layout.plan.concurrency)
        || evidence.schema_version != 2
        || evidence.replica != Some(replica)
        || evidence.summary["execution"]
            != serde_json::to_value(&execution).map_err(|error| resume_error(&error.to_string()))?
        || evidence.summary["execution_cohort"]
            != serde_json::to_value(cohort).map_err(|error| resume_error(&error.to_string()))?
    {
        return Err(resume_error(
            "campaign outcome differs from its original slot/start/cohort",
        ));
    }
    let mut roots = vec![
        Root::path("outcome", path.to_path_buf()),
        Root::path(
            "association",
            layout.root.join(format!(
                "batch-{}/outcomes/{}.json",
                slot.batch, slot.replica
            )),
        ),
        Root::path("start", layout.root.join(start_path)),
        Root::path(
            "prepared",
            layout
                .root
                .join(format!("batch-{}/prepared.json", slot.batch)),
        ),
    ];
    let workspace = layout.root.join(&slot.workspace);
    if !std::fs::symlink_metadata(&workspace).is_ok_and(|metadata| metadata.is_dir()) {
        return Err(resume_error("campaign workspace is missing or aliased"));
    }
    let runtime = crate::runner::replica::runtime_path(&workspace);
    let mut outputs = BTreeSet::from([workspace, layout.root.join(&slot.log)]);
    if std::fs::symlink_metadata(&runtime).is_ok() {
        outputs.insert(runtime);
    }
    if let Some(measurements) = layout.measurement_directory(slot) {
        outputs.insert(measurements);
        let measured = layout
            .measurement(slot)?
            .ok_or_else(|| resume_error("campaign measurement is missing"))?;
        if !completed::measurement_matches(&measured, evidence.summary) {
            return Err(resume_error(
                "campaign measurements differ from the graded slot",
            ));
        }
    }
    let actual: BTreeSet<_> = evidence.outputs.iter().cloned().collect();
    if actual != outputs || actual.len() != evidence.outputs.len() {
        return Err(resume_error(
            "campaign output roots differ from the saved slot layout",
        ));
    }
    for (index, output) in outputs.into_iter().enumerate() {
        roots.push(Root::path(format!("output-{index}"), output));
    }
    let module = entry
        .selection
        .module
        .as_ref()
        .ok_or_else(|| resume_error("campaign allocation capability is unknown"))?;
    let allocation_path = PathBuf::from(format!(
        "batch-{}/allocations/{}.json",
        slot.batch, slot.replica
    ));
    match std::fs::symlink_metadata(layout.root.join(&allocation_path)) {
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound
                && module.protocol.environment == EnvironmentKind::Native => {}
        Err(error) => return Err(io_error(error)),
        Ok(_) => {
            let allocation: Allocation =
                serde_json::from_value(layout.declaration(&allocation_path, METADATA)?)
                    .map_err(|_| resume_error("campaign allocation is malformed"))?;
            let (project_root, project_config) = crate::project::load(invocation)?;
            allocation.reference.validate_location(&project_root)?;
            if allocation.schema_version != 1
                || allocation.scope != "allocated-workload-reference-not-outcome"
                || allocation.execution != execution
                || !allocation.reference.matches_selection(&entry.selection)
                || !evidence.workload.contains(&allocation.reference.artifacts)
            {
                return Err(resume_error(
                    "campaign allocation differs from original execution/selection/evidence",
                ));
            }
            let remembered = slot
                .workspace
                .join(".asb-attempts")
                .join(format!("{}.json", allocation.reference.preparation_id));
            let remembered: AttemptReference =
                serde_json::from_value(layout.declaration(&remembered, METADATA)?)
                    .map_err(|_| resume_error("malformed workspace allocation linkage"))?;
            if remembered != allocation.reference {
                return Err(resume_error(
                    "campaign workspace lost its original allocation linkage",
                ));
            }
            verify(&crate::workloads::recovery::RetainedOutcomeContext {
                recovery: crate::workloads::recovery::RecoveryContext {
                    project_root: &project_root,
                    project_config: &project_config,
                    reference: &allocation.reference,
                },
                summary: evidence.summary,
            })?;
            roots.push(Root::path("allocation", layout.root.join(allocation_path)));
        }
    }
    if let Some(recording) = evidence.recording {
        if recording.campaign_root != layout.root
            || Some(&recording.track.destination) != layout.plan.recording.as_ref()
        {
            return Err(resume_error(
                "recording proof differs from campaign destination",
            ));
        }
        recording
            .track
            .compatibility
            .require_match(compatibility)
            .map_err(|error| resume_error(&error.to_string()))?;
        roots.push(Root::path(
            "recording-association",
            recording.association_path()?,
        ));
    }
    let extra: BTreeSet<_> = evidence
        .workload
        .iter()
        .chain(evidence.proxy)
        .cloned()
        .collect();
    for (index, path) in extra.into_iter().enumerate() {
        roots.push(Root::path(format!("evidence-{index}"), path));
    }
    Ok(roots)
}
