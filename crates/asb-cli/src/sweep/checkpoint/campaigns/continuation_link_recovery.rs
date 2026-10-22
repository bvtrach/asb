//! Owned, metadata-only repair of a fully staged but unlinked successor.
//! No admission capabilities escape; normal resume is a separate operation.
use super::*;
use crate::runner::campaign::generation::{HeadSnapshot, UnlinkedChild};

struct Repair<'a> {
    inputs: &'a mut Inputs,
    classified: Classified,
    candidate: UnlinkedChild,
    snapshot: HeadSnapshot,
    files_before: Fingerprint,
}

impl Inputs {
    pub(in crate::sweep) fn recover_continuation_link(
        &mut self,
        parent: &Path,
        results: &Path,
        sweep: &SweepPlan,
    ) -> Result<Value, Error> {
        self.recover_continuation_link_after_link(parent, results, sweep, |_| Ok(()))
    }

    // Private callback qualifies the hard-link/directory-sync crash boundary;
    // there is deliberately no environment or installed CLI fault surface.
    pub(in crate::sweep::checkpoint) fn recover_continuation_link_after_link(
        &mut self,
        parent: &Path,
        results: &Path,
        sweep: &SweepPlan,
        mut after_link: impl FnMut(&Path) -> Result<(), Error>,
    ) -> Result<Value, Error> {
        let files_before = self
            .snapshotter
            .fingerprint(&self.roots, &self.context, false, &[])?;
        let owned = self.own_campaigns(parent, results, sweep)?;
        let inspection = self
            .read_generation_with(results, sweep, &owned.layouts, Catalog::read_for_recovery)?
            .ok_or_else(|| recovery("no admitted continuation history"))?;
        let classified = self.classify_owned(
            parent,
            results,
            sweep,
            owned,
            Some(inspection.authoritative),
        )?;
        let Some(candidate) = inspection.candidate else {
            // Idempotence is permitted only for the exact unused current head.
            let catalog = classified.catalog.as_ref().expect("inspected catalog");
            if catalog.new_completion_count(&classified.receipt_bindings)? != 0 {
                return Err(recovery(
                    "current head has executed; no unused link repair exists",
                ));
            }
            let snapshot = catalog.head_snapshot()?;
            let report = repair_report(&snapshot, false);
            let mut repair = RepairHead {
                inputs: self,
                classified,
                snapshot,
                files_before,
            };
            repair.validate()?;
            let _writers = repair.writers()?;
            repair
                .classified
                .catalog
                .as_mut()
                .expect("inspected catalog")
                .confirm_reactivation(|_| Ok(()))?;
            repair.validate()?;
            return Ok(report);
        };
        let snapshot = candidate.snapshot()?;
        let mut repair = Repair {
            inputs: self,
            classified,
            candidate,
            snapshot,
            files_before,
        };
        repair.validate()?;
        // Journal owners are retained across final validation and link commit.
        // Recovery never starts a journal, imports again or fabricates a baseline.
        let _writers = repair.writers()?;
        repair
            .classified
            .catalog
            .as_mut()
            .expect("authoritative predecessor")
            .confirm_before_successor(&repair.candidate, |_| Ok(()))?;
        repair.candidate.confirm(|_| Ok(()))?;
        repair.validate()?;
        let catalog = repair
            .classified
            .catalog
            .as_ref()
            .expect("authoritative predecessor");
        let bytes = repair.candidate.link_bytes(catalog)?;
        crate::runner::repeated::publish_bytes_after_link(
            &catalog.head_root().join("next.json"),
            &bytes,
            || after_link(catalog.head_root()),
        )?;
        // Reopen through the normal strict reader, then durably confirm even
        // if an earlier link attempt had succeeded before its directory sync.
        let mut linked = repair
            .inputs
            .read_generation_catalog(results, sweep, &repair.classified.layouts)?
            .ok_or_else(|| recovery("linked continuation disappeared"))?;
        if linked.head_snapshot()?.bytes != repair.snapshot.bytes {
            return Err(recovery("repaired head differs from retained candidate"));
        }
        linked.confirm_reactivation(|_| Ok(()))?;
        if repair.inputs.snapshotter.fingerprint(
            &repair.inputs.roots,
            &repair.inputs.context,
            false,
            &[],
        )? != repair.files_before
        {
            return Err(recovery(
                "retained inputs changed during continuation link repair",
            ));
        }
        Ok(repair_report(&repair.snapshot, true))
    }
}

fn repair_report(snapshot: &HeadSnapshot, linked: bool) -> Value {
    json!({"type":"continuation-link-recovery","generation":snapshot.admission.generation,
        "linked":linked,"agent_executions":0,"evaluations":0})
}

/// Both repair cases consume the same strict classified population. Rechecking
/// does not reacquire campaign locks or resolve starts through the candidate.
fn validate_population(classified: &Classified, snapshot: &HeadSnapshot) -> Result<(), Error> {
    for (configuration, (layout, (manifest, _))) in classified
        .layouts
        .iter()
        .zip(&snapshot.manifests)
        .enumerate()
    {
        let runtime = PathBuf::from(".asb-generations").join(&snapshot.admission.generation);
        reading::list_relative(&layout.root, &runtime, 0)?;
        let expected: BTreeMap<_, _> = manifest
            .completed
            .iter()
            .map(|entry| {
                (
                    (configuration, entry.slot),
                    (entry.receipt.bytes, entry.receipt.sha256.clone()),
                )
            })
            .collect();
        let actual: BTreeMap<_, _> = classified
            .receipt_bindings
            .range((configuration, 0)..=(configuration, u64::MAX))
            .map(|(key, value)| (*key, value.clone()))
            .collect();
        let pending: BTreeSet<_> = manifest
            .pending
            .iter()
            .flat_map(|batch| {
                batch.cohort.admitted_slots.iter().map(move |replica| {
                    (batch.batch - 1) * layout.plan.concurrency as u64 + *replica as u64
                })
            })
            .collect();
        if expected != actual
            || pending
                != classified
                    .pending
                    .get(&configuration)
                    .cloned()
                    .unwrap_or_default()
        {
            return Err(recovery(
                "staged continuation differs from classified original population",
            ));
        }
        let empty = BTreeMap::new();
        let completed = classified.completed.get(&configuration).unwrap_or(&empty);
        if let Some(descriptor) = &manifest.recording {
            let derived = crate::runner::recording::union::Descriptor::plan(
                &layout.plan,
                &snapshot.admission.generation,
                configuration,
                completed,
            )?;
            if *descriptor != derived {
                return Err(recovery(
                    "staged recording imports differ from original completion evidence",
                ));
            }
        }
    }
    Ok(())
}

fn recover_writers(
    classified: &Classified,
    snapshot: &HeadSnapshot,
) -> Result<Vec<crate::cassette::RecordingJournal>, Error> {
    let mut writers = Vec::new();
    let catalog = classified
        .catalog
        .as_ref()
        .expect("classified authoritative history");
    for (configuration, (layout, (manifest, _))) in classified
        .layouts
        .iter()
        .zip(&snapshot.manifests)
        .enumerate()
    {
        if catalog.has_recording_union(configuration)? {
            crate::runner::recording::union::current(&layout.root, catalog, configuration)?;
        }
        if let Some(descriptor) = &manifest.recording
            && let Some(writer) = descriptor.recover_unused(
                classified.identities.get(&configuration),
                layout.plan.concurrency,
            )?
        {
            writers.push(writer);
        }
    }
    Ok(writers)
}

impl Repair<'_> {
    fn validate(&mut self) -> Result<(), Error> {
        self.candidate.recheck()?;
        let catalog = self
            .classified
            .catalog
            .as_ref()
            .expect("authoritative predecessor");
        catalog.recheck_for_publication(Some(self.candidate.root()))?;
        reclassify(self.inputs, &mut self.classified, &self.files_before)?;
        validate_population(&self.classified, &self.snapshot)
    }

    fn writers(&self) -> Result<Vec<crate::cassette::RecordingJournal>, Error> {
        recover_writers(&self.classified, &self.snapshot)
    }
}

struct RepairHead<'a> {
    inputs: &'a mut Inputs,
    classified: Classified,
    snapshot: HeadSnapshot,
    files_before: Fingerprint,
}

impl RepairHead<'_> {
    fn validate(&mut self) -> Result<(), Error> {
        reclassify(self.inputs, &mut self.classified, &self.files_before)?;
        validate_population(&self.classified, &self.snapshot)
    }

    fn writers(&self) -> Result<Vec<crate::cassette::RecordingJournal>, Error> {
        // For an already linked unused head, confirm only earlier applicable
        // unions; its own journal is deliberately not a finalized publication.
        let catalog = self
            .classified
            .catalog
            .as_ref()
            .expect("authoritative head");
        let mut writers = Vec::new();
        for (configuration, layout) in self.classified.layouts.iter().enumerate() {
            if let Some(reference) = catalog.recording_before_head(configuration)? {
                crate::runner::recording::union::confirmed(&layout.root, catalog, &reference)?;
            }
            if let Some(descriptor) = &self.snapshot.manifests[configuration].0.recording
                && let Some(writer) = descriptor.recover_unused(
                    self.classified.identities.get(&configuration),
                    layout.plan.concurrency,
                )?
            {
                writers.push(writer);
            }
        }
        Ok(writers)
    }
}

fn reclassify(
    inputs: &mut Inputs,
    classified: &mut Classified,
    files_before: &Fingerprint,
) -> Result<(), Error> {
    if inputs
        .snapshotter
        .fingerprint(&inputs.roots, &inputs.context, false, &[])?
        != *files_before
    {
        return Err(recovery(
            "retained inputs changed before continuation link repair",
        ));
    }
    let origin = &classified.origin;
    let results = origin
        .checkpoint
        .parent()
        .expect("checkpoint results parent");
    let bytes = reading::bounded(
        &results.join("plan.json"),
        crate::sweep::plan::MAX_PLAN_BYTES as u64,
    )?;
    if bytes.len() as u64 != origin.index_bytes
        || crate::cassette::digest(&bytes) != origin.index_sha256
    {
        return Err(recovery("original index changed before metadata repair"));
    }
    let sweep: SweepPlan =
        serde_json::from_slice(&bytes).map_err(|_| recovery("invalid original index"))?;
    let owned = OwnedCampaigns {
        origin: Origin::read(inputs, &origin.invocation_parent, results, &sweep)?,
        layouts: shared_layouts(results, &sweep)?,
        owners: std::mem::take(&mut classified._owners),
    };
    let next = inputs.classify_owned(
        &origin.invocation_parent,
        results,
        &sweep,
        owned,
        classified.catalog.take(),
    )?;
    if next.origin != classified.origin
        || next.receipt_bindings != classified.receipt_bindings
        || next.pending != classified.pending
    {
        return Err(recovery(
            "original population changed before metadata repair",
        ));
    }
    *classified = next;
    Ok(())
}
