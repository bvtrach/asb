//! Read-only whole-population continuation classification. Not launch permission.
use super::*;
use crate::runner::campaign::reading;

type Key = (usize, u64);
const MAX_RECORDS: usize = 65_533;
const MAX_RECORD: u64 = 1024 * 1024;
const MAX_BYTES: u64 = 256 * 1024 * 1024;

/// Locks remain owned until this classification is consumed. Pending slots
/// still need generation publication and exclusive create-new start admission.
pub(in crate::sweep) struct Classified {
    layouts: Vec<Layout>,
    completed: BTreeMap<usize, BTreeMap<u64, Value>>,
    pending: BTreeMap<usize, BTreeSet<u64>>,
    _owners: Vec<crate::output_lock::OutputLock>,
}

#[cfg(test)]
impl Classified {
    pub(in crate::sweep::checkpoint) fn pending_keys(&self) -> BTreeSet<Key> {
        self.pending
            .iter()
            .flat_map(|(configuration, slots)| {
                slots.iter().map(move |slot| (*configuration, *slot))
            })
            .collect()
    }
}

struct Actual {
    path: PathBuf,
    bytes: u64,
    sha256: String,
}

struct Inventory {
    observed: BTreeSet<Key>,
    unsettled: BTreeSet<Key>,
    actual: BTreeMap<Key, Actual>,
}

struct FileDeclaration {
    configuration: usize,
    path: PathBuf,
    bytes: u64,
}

fn recovery(reason: &str) -> Error {
    resume_error(&format!(
        "campaign saved-slot validation requires explicit recovery: {reason}"
    ))
}

fn present_absolute(path: &Path) -> Result<bool, Error> {
    if !path.is_absolute() {
        return Err(recovery("slot-owned observation is not absolute"));
    }
    let root = path
        .ancestors()
        .last()
        .ok_or_else(|| recovery("observation has no root"))?;
    reading::present_relative(
        root,
        path.strip_prefix(root)
            .map_err(|_| recovery("invalid observation root"))?,
    )
}

impl Inventory {
    fn read(layouts: &[Layout], checkpoint: &Path) -> Result<Self, Error> {
        let mut observed = BTreeSet::new();
        let mut unsettled = BTreeSet::new();
        let mut files = Vec::new();
        let mut declared_bytes = 0u64;
        // Admit metadata for every configuration before reading any actual
        // completion or proving a recording payload. Never recursively search.
        for (configuration, layout) in layouts.iter().enumerate() {
            for slot in &layout.plan.slots {
                let key = (configuration, slot.id);
                let mut leaves = vec![
                    slot.workspace.clone(),
                    crate::runner::replica::runtime_path(&slot.workspace),
                    slot.log.clone(),
                    PathBuf::from(format!("batch-{}/starts/{}.json", slot.batch, slot.replica)),
                    PathBuf::from(format!(
                        "batch-{}/allocations/{}.json",
                        slot.batch, slot.replica
                    )),
                    PathBuf::from(format!(
                        "batch-{}/outcomes/{}.json",
                        slot.batch, slot.replica
                    )),
                ];
                let producer = crate::runner::recording::producer(
                    &layout.root,
                    slot.batch,
                    slot.replica,
                    layout.plan.concurrency,
                );
                leaves.push(
                    producer
                        .strip_prefix(&layout.root)
                        .map_err(|_| recovery("producer escapes campaign"))?
                        .to_path_buf(),
                );
                for leaf in leaves {
                    if reading::present_relative(&layout.root, &leaf)? {
                        observed.insert(key);
                    }
                }
                if let Some(directory) = layout.measurement_directory(slot)
                    && present_absolute(&directory)?
                {
                    observed.insert(key);
                }
                if let Some(destination) = &layout.plan.recording
                    && present_absolute(&crate::runner::recording::association_for(
                        destination,
                        slot.id,
                    )?)?
                {
                    observed.insert(key);
                }
                if reading::present_relative(
                    checkpoint,
                    &PathBuf::from(format!(
                        "campaign-completed/{configuration:06}/{:06}.json",
                        slot.id
                    )),
                )? {
                    observed.insert(key);
                }
                if reading::present_relative(
                    checkpoint,
                    &PathBuf::from(format!(
                        "campaign-unsettled/{configuration:06}/{:06}.json",
                        slot.id
                    )),
                )? {
                    observed.insert(key);
                    unsettled.insert(key);
                }
            }
            for batch in 1..=layout.plan.repeat {
                let directory = PathBuf::from(format!("batch-{batch}/.asb-completions"));
                for name in
                    reading::list_relative(&layout.root, &directory, MAX_RECORDS - files.len())?
                {
                    let path = directory.join(name);
                    if path.extension().is_none_or(|extension| extension != "json") {
                        return Err(recovery(
                            "unfinished or unknown actual completion record remains",
                        ));
                    }
                    let bytes =
                        reading::inspect_relative(&layout.root, &path)?.ok_or_else(|| {
                            recovery("actual completion disappeared during inventory")
                        })?;
                    if bytes > MAX_RECORD {
                        return Err(recovery("actual completion exceeds 1 MiB"));
                    }
                    declared_bytes = declared_bytes
                        .checked_add(bytes)
                        .filter(|bytes| *bytes <= MAX_BYTES)
                        .ok_or_else(|| recovery("actual completion inventory exceeds 256 MiB"))?;
                    files.push(FileDeclaration {
                        configuration,
                        path,
                        bytes,
                    });
                }
            }
        }
        let mut actual = BTreeMap::new();
        for file in files {
            let layout = &layouts[file.configuration];
            // Share the same actual-read budget with later strict declaration
            // and proof inspection, including growth after metadata admission.
            let bytes = layout.declaration_bytes(&file.path, MAX_RECORD)?;
            if bytes.len() as u64 != file.bytes {
                return Err(recovery("actual completion length changed after inventory"));
            }
            let value: Value = serde_json::from_slice(&bytes)
                .map_err(|_| recovery("actual completion is malformed"))?;
            let id = value["summary"]["execution"]["slot"]
                .as_u64()
                .ok_or_else(|| recovery("actual completion has no original slot"))?;
            let slot = id
                .checked_sub(1)
                .and_then(|index| usize::try_from(index).ok())
                .and_then(|index| layout.plan.slots.get(index))
                .ok_or_else(|| recovery("actual completion is outside original population"))?;
            if value["summary"]["execution"]
                != serde_json::to_value(layout.plan.execution(slot))
                    .map_err(|_| recovery("cannot encode original execution"))?
                || file.path.parent()
                    != Some(Path::new(&format!("batch-{}/.asb-completions", slot.batch)))
            {
                return Err(recovery(
                    "actual completion differs from original campaign/batch/selection",
                ));
            }
            let key = (file.configuration, id);
            observed.insert(key);
            if actual
                .insert(
                    key,
                    Actual {
                        path: layout.root.join(file.path),
                        bytes: file.bytes,
                        sha256: crate::cassette::digest(&bytes),
                    },
                )
                .is_some()
            {
                return Err(recovery(
                    "multiple actual completions claim one original slot",
                ));
            }
        }
        Ok(Self {
            observed,
            unsettled,
            actual,
        })
    }
}

impl Inputs {
    pub(in crate::sweep) fn classify_campaigns(
        &self,
        parent: &Path,
        results: &Path,
        plan: &SweepPlan,
    ) -> Result<Classified, Error> {
        self.verify_campaign_reservations(results, plan)?;
        let layouts = shared_layouts(results, plan)?;
        let mut owners = Vec::with_capacity(layouts.len());
        for layout in &layouts {
            owners.push(
                crate::output_lock::OutputLock::acquire_existing(
                    &layout.root.join(".owner.lock"),
                    "execution campaign already has an owner",
                )
                .map_err(Error::Run)?,
            );
        }
        let inventory = Inventory::read(&layouts, &self.directory)?;
        let declarations = self.declare_campaign_layouts(
            parent,
            results,
            plan,
            layouts,
            crate::workloads::recovery::verify_retained,
        )?;
        if !declarations.unsettled.is_empty() || !inventory.unsettled.is_empty() {
            return Err(recovery(
                "one or more slots have incomplete or recovery-only outcomes",
            ));
        }
        let records: BTreeMap<Key, &Record> = declarations
            .records
            .iter()
            .map(|record| {
                let slot = &declarations.layouts[record.configuration].plan.slots[record.slot];
                ((record.configuration, slot.id), record)
            })
            .collect();
        if inventory.actual.len() != records.len() {
            return Err(recovery(
                "orphan actual completion or missing outcome reference",
            ));
        }
        for (key, actual) in &inventory.actual {
            let record = records
                .get(key)
                .ok_or_else(|| recovery("actual completion has no matching reference"))?;
            if actual.path != record.path
                || actual.bytes != record.bytes
                || actual.sha256 != record.sha256
            {
                return Err(recovery(
                    "actual completion and immutable reference disagree",
                ));
            }
        }
        let candidate_keys: BTreeSet<_> = declarations
            .candidates
            .iter()
            .map(|candidate| (candidate.configuration, candidate.slot))
            .collect();
        let mut pending: BTreeMap<usize, BTreeSet<u64>> = BTreeMap::new();
        for (configuration, layout) in declarations.layouts.iter().enumerate() {
            for slot in &layout.plan.slots {
                if candidate_keys.contains(&(configuration, slot.id)) {
                    continue;
                }
                if inventory.observed.contains(&(configuration, slot.id)) {
                    return Err(recovery(
                        "started, allocated or otherwise observed slot is not reusable",
                    ));
                }
                pending.entry(configuration).or_default().insert(slot.id);
            }
        }
        let mut remaining = MAX_BYTES;
        let receipts = declarations
            .candidates
            .iter()
            .map(|candidate| self.read_campaign_receipt(candidate, &mut remaining))
            .collect::<Result<Vec<_>, _>>()?;
        // Full-population metadata, actual/reference bijection, untouched
        // observations and receipt identities pass before one grouped proof.
        let inspection = declarations.prove()?;
        let mut completed: BTreeMap<usize, BTreeMap<u64, Value>> = BTreeMap::new();
        for (candidate, receipt) in inspection.candidates.into_iter().zip(receipts) {
            self.verify_campaign_candidate(&candidate, &receipt)?;
            completed
                .entry(candidate.configuration)
                .or_default()
                .insert(candidate.slot, candidate.summary);
        }
        Ok(Classified {
            layouts: inspection.layouts,
            completed,
            pending,
            _owners: owners,
        })
    }

    pub(in crate::sweep) fn resume_campaign_outcomes(
        &self,
        parent: &Path,
        results: &Path,
        plan: &SweepPlan,
    ) -> Result<crate::sweep::resume::State, Error> {
        let classification = self.classify_campaigns(parent, results, plan)?;
        if !classification.pending.is_empty() {
            return Err(resume_error(
                "campaign continuation requires saved-slot validation and generation admission for positively untouched slots",
            ));
        }
        let mut state = crate::sweep::resume::State::default();
        for (configuration, layout) in classification.layouts.iter().enumerate() {
            let slots = classification
                .completed
                .get(&configuration)
                .filter(|slots| slots.len() == layout.plan.slots.len())
                .ok_or_else(|| recovery("classification does not cover original population"))?;
            state
                .groups
                .insert(configuration, slots.values().cloned().collect());
        }
        state.campaign_owners = classification._owners;
        Ok(state)
    }
}
