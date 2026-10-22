//! Passive generation wire and bounded resolution. Never runtime admission.
use super::{Plan, Reference, Slot, reading};
use crate::{Error, measurements::ExecutionCohort};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub(crate) const SCOPE: &str = "pending-campaign-generation-not-execution-evidence";
pub(crate) const ROOT: &str = "campaign-continuation";
pub(crate) const MAX_MANIFEST: usize = 16 * 1024 * 1024;
pub(crate) const MAX_GENERATION: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Blob {
    pub bytes: u64,
    pub sha256: String,
}

impl Blob {
    pub(crate) fn of(bytes: &[u8]) -> Self {
        Self {
            bytes: bytes.len() as u64,
            sha256: crate::cassette::digest(bytes),
        }
    }

    pub(crate) fn matches(&self, bytes: &[u8]) -> bool {
        self.bytes == bytes.len() as u64 && self.sha256 == crate::cassette::digest(bytes)
    }

    fn bounded(&self, limit: usize) -> bool {
        self.bytes > 0 && self.bytes <= limit as u64 && valid_digest(&self.sha256)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Completed {
    pub slot: u64,
    pub receipt: Blob,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Batch {
    pub batch: u64,
    pub cohort: ExecutionCohort,
}

impl Batch {
    pub(crate) fn directory(&self, generation: &str) -> PathBuf {
        PathBuf::from(".asb-generations")
            .join(generation)
            .join(format!("batch-{}", self.batch))
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Manifest {
    pub schema_version: u32,
    pub scope: String,
    pub generation: String,
    pub run_id: String,
    pub input_digest: String,
    pub configuration: usize,
    pub campaign: String,
    pub original: Reference,
    pub completed: Vec<Completed>,
    pub pending: Vec<Batch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recording: Option<crate::runner::recording::union::Descriptor>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ManifestReference {
    pub configuration: usize,
    pub content: Blob,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Admission {
    pub schema_version: u32,
    pub scope: String,
    pub generation: String,
    pub run_id: String,
    pub input_digest: String,
    pub original_index: Blob,
    pub manifests: Vec<ManifestReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lineage: Option<Lineage>,
}

/// Successor identity binds exact predecessor bytes, not a caller-selected path.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Lineage {
    pub schema_version: u32,
    pub ordinal: usize,
    pub previous_generation: String,
    pub previous_admission: Blob,
}

/// Published last in the predecessor after the successor is durable.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NextReference {
    pub schema_version: u32,
    pub ordinal: usize,
    pub generation: String,
    pub admission: Blob,
    pub previous_admission: Blob,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StartReference {
    pub generation: String,
    pub configuration: usize,
    pub manifest: Blob,
    pub admission: Blob,
}

/// Values supplied by the owning workflow, never copied out of admission.
pub(crate) struct Context<'a> {
    pub run_id: &'a str,
    pub input_digest: &'a str,
    pub original_index: Blob,
}

pub(crate) struct Expected<'a> {
    pub reference: &'a Reference,
    pub plan: &'a Plan,
}

/// Immutable validated metadata, not a deserializable worker capability.
pub(crate) struct Catalog {
    nodes: Vec<Node>,
    indices: std::collections::BTreeMap<String, usize>,
}

/// One immutable admission and its private validated derivatives. Keeping node
/// lookup separate prevents later continuation from rebinding historical starts
/// or recording identities against the newest generation.
struct Node {
    root: PathBuf,
    admission: Admission,
    admission_blob: Blob,
    manifests: Vec<Manifest>,
    // Private derivatives of the validated manifests, not serialized authority.
    origin_commitments: Vec<Option<String>>,
    metadata_bytes: usize,
}

pub(crate) struct Resolved {
    pub prepared: PathBuf,
    pub evidence: Vec<PathBuf>,
}

fn invalid(reason: &str) -> Error {
    Error::Input(format!(
        "campaign generation requires explicit metadata recovery: {reason}"
    ))
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
}

pub(crate) fn manifest_path(index: usize) -> PathBuf {
    PathBuf::from(format!("{index:06}.json"))
}

impl Manifest {
    fn validate(
        &self,
        context: &Context<'_>,
        generation: &str,
        configuration: usize,
        expected: &Expected<'_>,
    ) -> Result<(), Error> {
        let plan = expected.plan;
        let total = plan.validate_layout()?;
        if self.schema_version != 1
            || self.scope != SCOPE
            || self.generation != generation
            || self.run_id != context.run_id
            || self.input_digest != context.input_digest
            || self.configuration != configuration
            || self.campaign != plan.campaign
            || self.original != *expected.reference
            || self.completed.len() > total
            || self.pending.len() > plan.repeat as usize
        {
            return Err(invalid(
                "manifest differs from its original index/input/campaign",
            ));
        }
        let mut slots = BTreeSet::new();
        let mut prior = 0;
        for complete in &self.completed {
            if complete.slot <= prior
                || complete.slot > total as u64
                || !complete.receipt.bounded(1024 * 1024)
            {
                return Err(invalid("completed slot or receipt binding is invalid"));
            }
            prior = complete.slot;
            slots.insert(complete.slot);
        }
        prior = 0;
        for batch in &self.pending {
            let cohort = &batch.cohort;
            if batch.batch <= prior
                || batch.batch > plan.repeat
                || cohort.dispatch.is_none()
                || cohort
                    .admitted_slots
                    .first()
                    .is_none_or(|member| !cohort.admits(*member, plan.concurrency))
            {
                return Err(invalid("pending batch or frozen cohort is invalid"));
            }
            prior = batch.batch;
            for member in &cohort.admitted_slots {
                let slot = (batch.batch - 1) * plan.concurrency as u64 + *member as u64;
                if !slots.insert(slot) {
                    return Err(invalid("completed/pending slots overlap"));
                }
            }
        }
        if slots.len() != total {
            return Err(invalid(
                "manifest does not partition the original population",
            ));
        }
        match (&self.recording, plan.mode, self.pending.is_empty()) {
            (Some(recording), crate::planning::ExecutionMode::Record, false) => recording
                .validate(
                    plan,
                    generation,
                    configuration,
                    self.completed.iter().map(|track| track.slot),
                )?,
            (None, crate::planning::ExecutionMode::Record, false) => {
                return Err(invalid("recording generation has no union descriptor"));
            }
            (None, _, _) => (),
            _ => return Err(invalid("unexpected recording union descriptor")),
        }
        Ok(())
    }
}

impl Node {
    fn has_recording_union(&self, configuration: usize) -> Result<bool, Error> {
        self.manifests
            .get(configuration)
            .map(|manifest| manifest.recording.is_some())
            .ok_or_else(|| invalid("configuration is outside manifest"))
    }
    fn recordings(
        &self,
    ) -> impl Iterator<Item = (usize, &crate::runner::recording::union::Descriptor)> {
        self.manifests
            .iter()
            .enumerate()
            .filter_map(|(index, manifest)| {
                manifest
                    .recording
                    .as_ref()
                    .map(|recording| (index, recording))
            })
    }
    fn completed_receipt(&self, configuration: usize, slot: u64) -> Option<&Blob> {
        let completed = &self.manifests.get(configuration)?.completed;
        completed
            .binary_search_by_key(&slot, |entry| entry.slot)
            .ok()
            .map(|index| &completed[index].receipt)
    }
    /// Validate every metadata envelope before parsing individual manifests.
    /// No provider, private JSONL, recording payload or receipt proof is read.
    fn read(
        checkpoint: &Path,
        relative: &Path,
        context: &Context<'_>,
        expected: &[Expected<'_>],
        remaining: usize,
    ) -> Result<Self, Error> {
        if expected.is_empty()
            || expected.len() > 65_533
            || !valid_digest(context.input_digest)
            || !context.original_index.bounded(MAX_GENERATION)
        {
            return Err(invalid("owning context is invalid"));
        }
        let root = std::path::absolute(checkpoint.join(relative)).map_err(crate::io_error)?;
        let bytes = reading::bounded_relative(
            checkpoint,
            &relative.join("admitted.json"),
            MAX_MANIFEST.min(remaining) as u64,
        )?;
        let admission: Admission =
            serde_json::from_slice(&bytes).map_err(|_| invalid("final admission is malformed"))?;
        if admission.schema_version != 1
            || admission.scope != SCOPE
            || !valid_id(&admission.generation)
            || admission.run_id != context.run_id
            || admission.input_digest != context.input_digest
            || admission.original_index != context.original_index
            || admission.manifests.len() != expected.len()
        {
            return Err(invalid(
                "final admission differs from original inputs/index",
            ));
        }
        let mut budget = bytes.len();
        let mut declared_names = BTreeSet::from([PathBuf::from("admitted.json")]);
        for (index, reference) in admission.manifests.iter().enumerate() {
            if reference.configuration != index || !reference.content.bounded(MAX_MANIFEST) {
                return Err(invalid("global manifest membership is invalid"));
            }
            budget = budget
                .checked_add(reference.content.bytes as usize)
                .filter(|total| *total <= remaining)
                .ok_or_else(|| invalid("cumulative generation metadata exceeds 64 MiB"))?;
            declared_names.insert(manifest_path(index));
        }
        for name in ["next.json", "successors"] {
            if reading::present_relative(&root, Path::new(name))? {
                if name == "successors" && relative != Path::new(ROOT) {
                    return Err(invalid("successor directory outside the fixed base"));
                }
                declared_names.insert(PathBuf::from(name));
            }
        }
        let actual_names: BTreeSet<_> =
            reading::list_relative(checkpoint, relative, expected.len() + 3)?
                .into_iter()
                .map(PathBuf::from)
                .collect();
        if actual_names != declared_names {
            return Err(invalid("partial or foreign generation metadata remains"));
        }
        let mut population = 0usize;
        let mut manifests = Vec::with_capacity(expected.len());
        let mut origin_commitments = Vec::with_capacity(expected.len());
        for (reference, original) in admission.manifests.iter().zip(expected) {
            population = population
                .checked_add(original.plan.slots.len())
                .filter(|count| *count <= 65_533)
                .ok_or_else(|| invalid("generation population exceeds 65533 slots"))?;
            let bytes = reading::bounded_relative(
                &root,
                &manifest_path(reference.configuration),
                reference.content.bytes,
            )?;
            if !reference.content.matches(&bytes) {
                return Err(invalid("generation manifest bytes changed"));
            }
            let manifest: Manifest = serde_json::from_slice(&bytes)
                .map_err(|_| invalid("generation manifest is malformed"))?;
            manifest.validate(
                context,
                &admission.generation,
                reference.configuration,
                original,
            )?;
            origin_commitments.push(
                manifest
                    .recording
                    .as_ref()
                    .map(|descriptor| descriptor.origins.commitment())
                    .transpose()
                    .map_err(|error| invalid(&error.to_string()))?,
            );
            manifests.push(manifest);
        }
        Ok(Self {
            root,
            admission,
            admission_blob: Blob::of(&bytes),
            manifests,
            origin_commitments,
            metadata_bytes: budget,
        })
    }

    fn reference(&self, configuration: usize) -> Result<StartReference, Error> {
        let manifest = self
            .admission
            .manifests
            .get(configuration)
            .ok_or_else(|| invalid("configuration is outside admission"))?;
        Ok(StartReference {
            generation: self.admission.generation.clone(),
            configuration,
            manifest: manifest.content.clone(),
            admission: self.admission_blob.clone(),
        })
    }

    fn recording_descriptor(
        &self,
        configuration: usize,
        reference: &StartReference,
    ) -> Result<&crate::runner::recording::union::Descriptor, Error> {
        if *reference != self.reference(configuration)? {
            return Err(invalid("union publication reference changed"));
        }
        self.manifests[configuration]
            .recording
            .as_ref()
            .ok_or_else(|| invalid("generation has no recording union"))
    }

    /// Resolve recording identity against the already validated owning catalog.
    fn recording(
        &self,
        configuration: usize,
        binding: &crate::runner::recording::union::Binding,
        evidence: &crate::runner::recording::Evidence,
        start: &super::StartIntent,
        plan: &Plan,
        slot: &Slot,
    ) -> Result<(), Error> {
        self.resolve(
            start.generation.as_ref(),
            configuration,
            plan,
            slot,
            start.execution_cohort.as_ref(),
        )?;
        let descriptor = self
            .manifests
            .get(configuration)
            .and_then(|manifest| manifest.recording.as_ref())
            .ok_or_else(|| invalid("recording has no owning union descriptor"))?;
        if start.generation.as_ref() != Some(&binding.reference)
            || start.schema_version != 1
            || start.scope != "execution-start-intent-not-actual-attempt"
            || start.execution != plan.execution(slot)
            || binding.reference != self.reference(configuration)?
            || self
                .origin_commitments
                .get(configuration)
                .and_then(Option::as_ref)
                != Some(&binding.origins_sha256)
            || evidence.track.origin_sha256.as_ref() != Some(&binding.origins_sha256)
            || evidence.track.recording_id != descriptor.physical_id
            || evidence.track.destination != descriptor.destination
            || descriptor.origins.imported(slot.id).is_some()
        {
            return Err(invalid(
                "recording differs from admitted physical identity/import plan",
            ));
        }
        Ok(())
    }

    /// Return derived evidence locations only; this cannot authorize execution.
    fn resolve(
        &self,
        reference: Option<&StartReference>,
        configuration: usize,
        plan: &Plan,
        slot: &Slot,
        cohort: Option<&ExecutionCohort>,
    ) -> Result<Resolved, Error> {
        let manifest = self
            .manifests
            .get(configuration)
            .ok_or_else(|| invalid("configuration is outside manifest"))?;
        if manifest.campaign != plan.campaign
            || slot
                .id
                .checked_sub(1)
                .and_then(|id| usize::try_from(id).ok())
                .and_then(|id| plan.slots.get(id))
                != Some(slot)
        {
            return Err(invalid(
                "resolved slot differs from original campaign layout",
            ));
        }
        let pending = manifest
            .pending
            .binary_search_by_key(&slot.batch, |batch| batch.batch)
            .ok()
            .map(|index| &manifest.pending[index])
            .filter(|batch| {
                batch
                    .cohort
                    .admitted_slots
                    .binary_search(&slot.replica)
                    .is_ok()
            });
        if reference.is_none() {
            if pending.is_some() {
                return Err(invalid(
                    "continued start is missing its generation reference",
                ));
            }
            if self.completed_receipt(configuration, slot.id).is_none() {
                return Err(invalid("historical start is outside completed membership"));
            }
            return Ok(Resolved {
                prepared: PathBuf::from(format!("batch-{}/prepared.json", slot.batch)),
                evidence: Vec::new(),
            });
        }
        let reference = reference.expect("checked above");
        let batch = pending.ok_or_else(|| invalid("start was not admitted as pending"))?;
        if *reference != self.reference(configuration)?
            || manifest.campaign != plan.campaign
            || cohort != Some(&batch.cohort)
            || !batch.cohort.admits(slot.replica, plan.concurrency)
        {
            return Err(invalid("start differs from frozen generation/slot/cohort"));
        }
        Ok(Resolved {
            prepared: batch.directory(&manifest.generation).join("prepared.json"),
            evidence: vec![
                self.root.join(manifest_path(configuration)),
                self.root.join("admitted.json"),
            ],
        })
    }
}

impl Catalog {
    fn head(&self) -> &Node {
        self.nodes
            .last()
            .expect("validated nonempty generation chain")
    }

    fn node_for(&self, reference: &StartReference) -> Result<&Node, Error> {
        let node = self
            .indices
            .get(&reference.generation)
            .and_then(|index| self.nodes.get(*index))
            .ok_or_else(|| invalid("start names an unknown generation"))?;
        if node.reference(reference.configuration)? != *reference {
            return Err(invalid("historical admission/manifest reference changed"));
        }
        Ok(node)
    }

    pub(crate) fn read(
        checkpoint: &Path,
        context: Context<'_>,
        expected: &[Expected<'_>],
    ) -> Result<Self, Error> {
        Self::read_bounded(checkpoint, context, expected, MAX_GENERATION)
    }

    #[cfg(test)]
    pub(crate) fn read_with_metadata_limit(
        checkpoint: &Path,
        context: Context<'_>,
        expected: &[Expected<'_>],
        limit: usize,
    ) -> Result<Self, Error> {
        Self::read_bounded(checkpoint, context, expected, limit.min(MAX_GENERATION))
    }

    fn read_bounded(
        checkpoint: &Path,
        context: Context<'_>,
        expected: &[Expected<'_>],
        limit: usize,
    ) -> Result<Self, Error> {
        let mut nodes: Vec<Node> = Vec::new();
        let mut indices = std::collections::BTreeMap::new();
        let mut remaining = limit;
        let mut next: Option<NextReference> = None;
        let mut relative = PathBuf::from(ROOT);
        loop {
            let node = Node::read(checkpoint, &relative, &context, expected, remaining)?;
            remaining = remaining
                .checked_sub(node.metadata_bytes)
                .ok_or_else(|| invalid("cumulative generation metadata exceeds 64 MiB"))?;
            let ordinal = nodes.len();
            if let Some(previous) = nodes.last() {
                let link = next.as_ref().expect("successor requires validated link");
                if node.admission.lineage
                    != Some(Lineage {
                        schema_version: 1,
                        ordinal,
                        previous_generation: previous.admission.generation.clone(),
                        previous_admission: previous.admission_blob.clone(),
                    })
                    || node.admission_blob != link.admission
                    || node.admission.generation != link.generation
                {
                    return Err(invalid("successor differs from its linked predecessor"));
                }
                validate_transition(previous, &node)?;
            } else if node.admission.lineage.is_some() {
                return Err(invalid("first generation unexpectedly has a predecessor"));
            }
            if indices
                .insert(node.admission.generation.clone(), ordinal)
                .is_some()
            {
                return Err(invalid("generation chain repeats an identity"));
            }
            next = if reading::present_relative(&node.root, Path::new("next.json"))? {
                let bytes = reading::bounded_relative(
                    &node.root,
                    Path::new("next.json"),
                    remaining.min(MAX_MANIFEST) as u64,
                )?;
                remaining = remaining
                    .checked_sub(bytes.len())
                    .ok_or_else(|| invalid("cumulative generation metadata exceeds 64 MiB"))?;
                let link: NextReference = serde_json::from_slice(&bytes)
                    .map_err(|_| invalid("successor link is malformed"))?;
                if link.schema_version != 1
                    || link.ordinal != ordinal + 1
                    || !valid_id(&link.generation)
                    || !link.admission.bounded(MAX_MANIFEST)
                    || link.previous_admission != node.admission_blob
                    || ordinal + 1 >= 65_533
                {
                    return Err(invalid("successor link identity/ordinal is invalid"));
                }
                Some(link)
            } else {
                None
            };
            nodes.push(node);
            if let Some(link) = &next {
                relative = Path::new(ROOT)
                    .join("successors")
                    .join(format!("{:06}", link.ordinal));
            } else {
                break;
            }
        }
        // Enumerate only the fixed topology, never select a generation by name,
        // recency or directory order. An unlinked child is recovery-only state.
        let actual: BTreeSet<_> =
            reading::list_relative(checkpoint, &Path::new(ROOT).join("successors"), nodes.len())?
                .into_iter()
                .collect();
        let declared: BTreeSet<_> = (1..nodes.len())
            .map(|ordinal| std::ffi::OsString::from(format!("{ordinal:06}")))
            .collect();
        if actual != declared {
            return Err(invalid(
                "orphan or foreign successor requires metadata recovery",
            ));
        }
        Ok(Self { nodes, indices })
    }

    pub(crate) fn reference(&self, configuration: usize) -> Result<StartReference, Error> {
        self.head().reference(configuration)
    }

    pub(crate) fn has_recording_union(&self, configuration: usize) -> Result<bool, Error> {
        self.head().has_recording_union(configuration)
    }

    pub(crate) fn recordings(
        &self,
    ) -> impl Iterator<Item = (usize, &crate::runner::recording::union::Descriptor)> {
        self.head().recordings()
    }

    pub(crate) fn completed_receipt(&self, configuration: usize, slot: u64) -> Option<&Blob> {
        self.head().completed_receipt(configuration, slot)
    }

    pub(crate) fn recording_descriptor(
        &self,
        configuration: usize,
        reference: &StartReference,
    ) -> Result<&crate::runner::recording::union::Descriptor, Error> {
        self.node_for(reference)?
            .recording_descriptor(configuration, reference)
    }

    pub(crate) fn recording(
        &self,
        configuration: usize,
        binding: &crate::runner::recording::union::Binding,
        evidence: &crate::runner::recording::Evidence,
        start: &super::StartIntent,
        plan: &Plan,
        slot: &Slot,
    ) -> Result<(), Error> {
        self.node_for(&binding.reference)?.recording(
            configuration,
            binding,
            evidence,
            start,
            plan,
            slot,
        )
    }

    pub(crate) fn resolve(
        &self,
        reference: Option<&StartReference>,
        configuration: usize,
        plan: &Plan,
        slot: &Slot,
        cohort: Option<&ExecutionCohort>,
    ) -> Result<Resolved, Error> {
        let node = match reference {
            Some(reference) => self.node_for(reference)?,
            None => &self.nodes[0],
        };
        node.resolve(reference, configuration, plan, slot, cohort)
    }
}

fn validate_transition(previous: &Node, next: &Node) -> Result<(), Error> {
    let mut progress = false;
    for (before, after) in previous.manifests.iter().zip(&next.manifests) {
        for entry in &before.completed {
            let inherited = after
                .completed
                .binary_search_by_key(&entry.slot, |entry| entry.slot)
                .ok()
                .map(|index| &after.completed[index]);
            if inherited.is_none_or(|inherited| inherited.receipt != entry.receipt) {
                return Err(invalid("successor changed an inherited completion receipt"));
            }
        }
        progress |= after.completed.len() > before.completed.len();
    }
    // Each node independently partitions the same original population. Once
    // inherited receipts are preserved, every newly completed slot was pending
    // in its predecessor, and all residual slots remain predecessor pending.
    if !progress {
        return Err(invalid("successor makes no completed-slot progress"));
    }
    Ok(())
}
