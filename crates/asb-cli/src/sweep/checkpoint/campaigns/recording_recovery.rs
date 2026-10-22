//! Metadata/publication-only recovery. No worker pool, agent, provider or grader.
use super::*;

impl Inputs {
    pub(in crate::sweep) fn recover_recording_publications(
        &mut self,
        parent: &Path,
        results: &Path,
        plan: &SweepPlan,
    ) -> Result<Vec<Value>, Error> {
        let files_before = self
            .snapshotter
            .fingerprint(&self.roots, &self.context, false, &[])?;
        let layouts = shared_layouts(results, plan)?;
        let _owners = layouts
            .iter()
            .map(|layout| {
                crate::output_lock::OutputLock::acquire_existing(
                    &layout.root.join(".owner.lock"),
                    "execution campaign already has an owner",
                )
                .map_err(Error::Run)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let catalog = self
            .read_generation_catalog(results, plan, &layouts)?
            .ok_or_else(|| resume_error("no admitted recording generation to recover"))?;
        let inspection = self
            .declare_campaign_layouts_with_catalog(
                parent,
                results,
                plan,
                layouts,
                crate::workloads::recovery::verify_retained,
                Some(&catalog),
            )?
            .prove()?;
        let mut remaining = 256 * 1024 * 1024;
        // Validate original receipts and all actual source declarations before
        // touching any publication marker. Unknown new slots gain no receipt.
        for candidate in &inspection.candidates {
            if candidate.receipt_binding.is_some() {
                let (receipt, _) = self.read_campaign_receipt(candidate, &mut remaining)?;
                self.verify_campaign_candidate(candidate, &receipt)?;
            }
        }
        let mut prepared = Vec::new();
        for (configuration, descriptor) in catalog.recordings() {
            let journal = crate::cassette::RecordingJournal::recover(
                &descriptor.destination,
                crate::cassette::Limits::default(),
            )
            .map_err(|error| resume_error(&format!("union publication recovery: {error}")))?;
            if !journal.publication_is_frozen() {
                return Err(resume_error(
                    "recording has no existing frozen publication; recovery cannot settle or retry producers",
                ));
            }
            let snapshot = journal
                .snapshot()
                .map_err(|error| resume_error(&error.to_string()))?;
            if snapshot.recording_id != descriptor.physical_id
                || snapshot.provenance.as_ref() != Some(&descriptor.origins)
                || snapshot.concurrency != inspection.layouts[configuration].plan.concurrency as u64
            {
                return Err(resume_error(
                    "recovered union differs from admitted origin/physical identity",
                ));
            }
            if let Some(identity) = inspection.identities.get(&configuration) {
                snapshot
                    .compatibility
                    .require_match(identity)
                    .map_err(|error| resume_error(&error.to_string()))?;
            }
            prepared.push((configuration, descriptor, journal));
        }
        if prepared.is_empty() {
            return Err(resume_error(
                "generation contains no recording union publication",
            ));
        }
        if self
            .snapshotter
            .fingerprint(&self.roots, &self.context, false, &[])?
            != files_before
        {
            return Err(resume_error(
                "inputs changed before recording publication recovery",
            ));
        }
        let mut reports = Vec::new();
        let mut publications = Vec::new();
        for (configuration, descriptor, mut journal) in prepared {
            let publication = journal
                .finalize()
                .map_err(|error| resume_error(&error.to_string()))?;
            let binding = crate::runner::recording::union::Binding {
                reference: catalog.reference(configuration)?,
                origins_sha256: descriptor
                    .origins
                    .commitment()
                    .map_err(|error| resume_error(&error.to_string()))?,
            };
            publications.push(crate::runner::recording::union::prepare_publication(
                &inspection.layouts[configuration].root,
                descriptor,
                &binding,
                &publication,
                &catalog.recording_current_path(&binding.reference)?,
            )?);
            reports.push(json!({"type":"recording-publication-recovery","configuration":configuration,
                "destination":publication.destination,"agent_executions":0,"evaluations":0,"cleanup_complete":publication.cleanup_complete}));
        }
        for publication in &publications {
            publication.preflight()?;
        }
        // Archive confirmation is independently monotonic; all predictable
        // pointer conflicts are rejected before ANY current ledger advances.
        // A crash during this loop is repaired idempotently, not rolled back.
        for publication in publications {
            publication.commit()?;
        }
        if self
            .snapshotter
            .fingerprint(&self.roots, &self.context, false, &[])?
            != files_before
        {
            return Err(resume_error(
                "inputs changed during recording publication recovery",
            ));
        }
        Ok(reports)
    }
}
