//! Consuming, all-configuration generation publication and owned activation.

use super::*;
use crate::measurements::ExecutionCohort;
use crate::runner::campaign::Plan;
use crate::runner::campaign::generation::{
    Admission, Batch, Blob, Completed, MAX_GENERATION, MAX_MANIFEST, Manifest, ManifestReference,
    ROOT, SCOPE, manifest_path,
};
use crate::runner::repeated::publish_bytes;
use serde::Serialize;
use std::fs;

/// Not Clone/Deserialize. Original observation budgets never enter workers.
struct Campaign {
    compatibility: Option<crate::cassette::Compatibility>,
    invocation: PathBuf,
    root: PathBuf,
    plan: Plan,
    completed: BTreeMap<u64, Value>,
    manifest: Manifest,
    bytes: Vec<u8>,
}

/// Parent retains every campaign lock, including already completed campaigns,
/// through worker settlement and final report publication.
pub(in crate::sweep) struct Guard {
    inputs: Inputs,
    owners: Vec<crate::output_lock::OutputLock>,
}

/// Planning owns the original input owner and cannot mint a second generation.
pub(in crate::sweep) struct Prepared {
    guard: Guard,
    parent: PathBuf,
    origin: Origin,
    campaigns: Vec<Campaign>,
    admission: Admission,
    admission_bytes: Vec<u8>,
}

/// Only a fully published generation can produce worker capabilities. This is
/// not deserializable; a future recovery path must positively revalidate state.
pub(in crate::sweep) struct AdmittedCampaign {
    compatibility: Option<crate::cassette::Compatibility>,
    invocation: PathBuf,
    root: PathBuf,
    plan: Plan,
    completed: BTreeMap<u64, Value>,
    manifest: Manifest,
    manifest_reference: Blob,
    admission_reference: Blob,
}

impl Guard {
    pub(in crate::sweep) fn inputs(&self) -> &Inputs {
        &self.inputs
    }

    pub(in crate::sweep) fn verify_stable(&mut self) -> Result<(), Error> {
        if self.inputs.final_fingerprint()? != self.inputs.initial {
            return Err(recovery("declared inputs changed during continuation"));
        }
        // Keep all campaign owners, including completed-only configurations,
        // structurally live through comparison rather than transferring inputs.
        debug_assert!(!self.owners.is_empty());
        Ok(())
    }
}

impl AdmittedCampaign {
    pub(in crate::sweep) fn settlement_root(&self) -> PathBuf {
        self.root
            .join(".asb-generations")
            .join(&self.manifest.generation)
    }

    pub(in crate::sweep) fn execute(
        self,
        json: bool,
        show_output: bool,
        policy: Option<&crate::planning::ResolvedPlan>,
        output: &mut dyn std::io::Write,
        warnings: &std::sync::Mutex<BTreeSet<String>>,
    ) -> Result<(), Error> {
        let reference = crate::runner::campaign::generation::StartReference {
            generation: self.manifest.generation.clone(),
            configuration: self.manifest.configuration,
            manifest: self.manifest_reference,
            admission: self.admission_reference,
        };
        crate::runner::repeated::continue_campaign(
            crate::runner::repeated::Continuation {
                invocation: self.invocation,
                root: self.root,
                plan: self.plan,
                completed: self.completed,
                pending: self.manifest.pending,
                compatibility: self.compatibility,
                reference,
            },
            json,
            show_output,
            policy,
            output,
            warnings,
        )
    }
}

fn encode(value: &impl Serialize) -> Result<Vec<u8>, Error> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|_| recovery("cannot encode continuation generation"))?;
    if bytes.len() > MAX_MANIFEST {
        return Err(recovery("continuation manifest exceeds 16 MiB"));
    }
    Ok(bytes)
}

fn root(inputs: &Inputs) -> PathBuf {
    inputs.directory.join(ROOT)
}

impl Prepared {
    pub(super) fn plan(
        inputs: Inputs,
        classified: Classified,
        parent: &Path,
        results: &Path,
        sweep: &SweepPlan,
    ) -> Result<Self, Error> {
        if classified.origin != Origin::read(&inputs, parent, results, sweep)? {
            return Err(recovery(
                "classification belongs to another input/index owner",
            ));
        }
        // A single derived root makes a prior publication crash visible. Never
        // pick a new random directory and silently abandon earlier authority.
        if reading::present_relative(&inputs.directory, Path::new(ROOT))? {
            return Err(recovery(
                "a prior continuation generation requires metadata recovery",
            ));
        }
        if classified.pending.is_empty() {
            return Err(recovery(
                "completed-only reuse needs no continuation generation",
            ));
        }
        if classified
            .layouts
            .iter()
            .any(|layout| layout.plan.mode == crate::planning::ExecutionMode::Record)
        {
            return Err(recovery(
                "recording continuation requires union publication recovery",
            ));
        }
        let Classified {
            mut identities,
            origin,
            layouts,
            mut completed,
            pending,
            receipt_bindings,
            _owners,
        } = classified;
        let generation =
            crate::workloads::TaskIdentity::local("continuation-generation", None).attempt_id;
        let mut campaigns = Vec::with_capacity(layouts.len());
        let mut references = Vec::with_capacity(layouts.len());
        let mut total_bytes = 0usize;
        let mut total_slots = 0usize;
        for (configuration, layout) in layouts.into_iter().enumerate() {
            let retained = completed.remove(&configuration).unwrap_or_default();
            let untouched = pending.get(&configuration).cloned().unwrap_or_default();
            total_slots = total_slots
                .checked_add(layout.plan.slots.len())
                .filter(|count| *count <= MAX_RECORDS)
                .ok_or_else(|| recovery("continuation population exceeds 65533 slots"))?;
            let mut batches: BTreeMap<u64, Vec<usize>> = BTreeMap::new();
            let mut receipts = Vec::with_capacity(retained.len());
            for slot in &layout.plan.slots {
                match (
                    retained.contains_key(&slot.id),
                    untouched.contains(&slot.id),
                ) {
                    (true, false) => {
                        let (bytes, sha256) = receipt_bindings
                            .get(&(configuration, slot.id))
                            .ok_or_else(|| {
                                recovery("completed slot has no verified receipt binding")
                            })?;
                        receipts.push(Completed {
                            slot: slot.id,
                            receipt: Blob {
                                bytes: *bytes,
                                sha256: sha256.clone(),
                            },
                        });
                    }
                    (false, true) => batches.entry(slot.batch).or_default().push(slot.replica),
                    _ => {
                        return Err(recovery(
                            "classification does not partition the original population",
                        ));
                    }
                }
            }
            if retained.len() + untouched.len() != layout.plan.slots.len() {
                return Err(recovery("classification contains foreign original slots"));
            }
            let pending = batches
                .into_iter()
                .map(|(batch, members)| {
                    let mut cohort = ExecutionCohort {
                        id: crate::workloads::TaskIdentity::local("execution-cohort", None)
                            .attempt_id,
                        selected_replicas: layout.plan.concurrency,
                        admitted_slots: members,
                        dispatch: None,
                    };
                    cohort.dispatch = Some(
                        cohort.dispatch_for(cohort.admitted_slots[0], layout.plan.concurrency)?,
                    );
                    Ok(Batch { batch, cohort })
                })
                .collect::<Result<Vec<_>, Error>>()?;
            let manifest = Manifest {
                schema_version: 1,
                scope: SCOPE.into(),
                generation: generation.clone(),
                run_id: origin.run_id.clone(),
                input_digest: origin.input_digest.clone(),
                configuration,
                campaign: layout.plan.campaign.clone(),
                original: sweep.entries[configuration]
                    .campaign
                    .as_ref()
                    .ok_or_else(|| recovery("original campaign reference is missing"))?
                    .clone(),
                completed: receipts,
                pending,
            };
            let bytes = encode(&manifest)?;
            total_bytes = total_bytes
                .checked_add(bytes.len())
                .filter(|count| *count <= MAX_GENERATION)
                .ok_or_else(|| recovery("continuation generation exceeds 64 MiB"))?;
            references.push(ManifestReference {
                configuration,
                content: Blob::of(&bytes),
            });
            // Drop the Layout here, retaining only its owned Send plan and root.
            campaigns.push(Campaign {
                compatibility: identities.remove(&configuration),
                invocation: origin
                    .invocation_parent
                    .join(&sweep.entries[configuration].reference),
                root: layout.root,
                plan: layout.plan,
                completed: retained,
                manifest,
                bytes,
            });
        }
        let admission = Admission {
            schema_version: 1,
            scope: SCOPE.into(),
            generation,
            run_id: origin.run_id.clone(),
            input_digest: origin.input_digest.clone(),
            original_index: Blob {
                bytes: origin.index_bytes,
                sha256: origin.index_sha256.clone(),
            },
            manifests: references,
        };
        let admission_bytes = encode(&admission)?;
        total_bytes
            .checked_add(admission_bytes.len())
            .filter(|count| *count <= MAX_GENERATION)
            .ok_or_else(|| recovery("continuation generation exceeds 64 MiB"))?;
        Ok(Self {
            guard: Guard {
                inputs,
                owners: _owners,
            },
            parent: origin.invocation_parent.clone(),
            origin,
            campaigns,
            admission,
            admission_bytes,
        })
    }

    fn recheck(&mut self, results: &Path, sweep: &SweepPlan) -> Result<(), Error> {
        let inputs = &mut self.guard.inputs;
        if self.origin != Origin::read(inputs, &self.parent, results, sweep)?
            || inputs.final_fingerprint()? != inputs.initial
        {
            return Err(recovery(
                "original continuation inputs/index changed before admission",
            ));
        }
        // Revalidate the whole population while ALL original owners remain held.
        // This does not reacquire locks and reads no public replay archive.
        let layouts = shared_layouts(results, sweep)?;
        let inventory = Inventory::read(&layouts, &inputs.directory)?;
        // This generation has not published its final marker yet. Only the
        // original completed starts may be checked during this owned phase.
        let declarations = inputs.declare_campaign_layouts_with_catalog(
            &self.parent,
            results,
            sweep,
            layouts,
            crate::workloads::recovery::verify_retained,
            None,
        )?;
        if !inventory.unsettled.is_empty() || !declarations.unsettled.is_empty() {
            return Err(recovery(
                "unsettled evidence appeared before generation admission",
            ));
        }
        for campaign in &self.campaigns {
            let index = campaign.manifest.configuration;
            for batch in &campaign.manifest.pending {
                for replica in &batch.cohort.admitted_slots {
                    let slot =
                        (batch.batch - 1) * campaign.plan.concurrency as u64 + *replica as u64;
                    if inventory.observed.contains(&(index, slot)) {
                        return Err(recovery(
                            "a pending original slot changed before generation admission",
                        ));
                    }
                }
            }
            for receipt in &campaign.manifest.completed {
                let bytes = reading::bounded(
                    &inputs.campaign_completion_path(index, receipt.slot),
                    MAX_RECORD,
                )?;
                if !receipt.receipt.matches(&bytes) {
                    return Err(recovery(
                        "a completed receipt changed before generation admission",
                    ));
                }
            }
        }
        let expected: BTreeSet<_> = self
            .campaigns
            .iter()
            .flat_map(|campaign| {
                campaign
                    .manifest
                    .completed
                    .iter()
                    .map(|receipt| (campaign.manifest.configuration, receipt.slot))
            })
            .collect();
        let actual: BTreeSet<_> = declarations
            .candidates
            .iter()
            .map(|candidate| (candidate.configuration, candidate.slot))
            .collect();
        if actual != expected || inventory.actual.len() != declarations.records.len() {
            return Err(recovery(
                "completed population changed before generation admission",
            ));
        }
        for record in &declarations.records {
            let slot = &declarations.layouts[record.configuration].plan.slots[record.slot];
            let actual = inventory
                .actual
                .get(&(record.configuration, slot.id))
                .ok_or_else(|| recovery("actual completion lost its immutable reference"))?;
            if actual.path != record.path
                || actual.bytes != record.bytes
                || actual.sha256 != record.sha256
            {
                return Err(recovery(
                    "actual completion/reference changed before generation admission",
                ));
            }
        }
        // The original receipt's roots/fingerprint must still agree with all
        // completed evidence, not merely an unchanged receipt filename.
        let mut remaining = MAX_BYTES;
        let receipts = declarations
            .candidates
            .iter()
            .map(|candidate| inputs.read_campaign_receipt(candidate, &mut remaining))
            .collect::<Result<Vec<_>, _>>()?;
        let inspection = declarations.prove()?;
        for (candidate, (receipt, _)) in inspection.candidates.iter().zip(&receipts) {
            inputs.verify_campaign_candidate(candidate, receipt)?;
        }
        Ok(())
    }

    pub(in crate::sweep) fn publish(
        self,
        results: &Path,
        sweep: &SweepPlan,
    ) -> Result<(Guard, Vec<AdmittedCampaign>), Error> {
        self.publish_with(results, sweep, |_| Ok(()))
    }

    fn publish_with(
        mut self,
        results: &Path,
        sweep: &SweepPlan,
        mut after_file: impl FnMut(usize) -> Result<(), Error>,
    ) -> Result<(Guard, Vec<AdmittedCampaign>), Error> {
        self.recheck(results, sweep)?;
        let directory = root(&self.guard.inputs);
        // No recursive mkdir/adoption: existing partial state is recovery-only.
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&directory).map_err(io_error)?;
        sync_directory(&self.guard.inputs.directory)?;
        for (index, campaign) in self.campaigns.iter().enumerate() {
            publish_bytes(&directory.join(manifest_path(index)), &campaign.bytes)?;
            after_file(index)?;
        }
        // Read back the exact bounded snapshots, not merely their names/count.
        for (reference, campaign) in self.admission.manifests.iter().zip(&self.campaigns) {
            let bytes = reading::bounded_relative(
                &directory,
                &manifest_path(reference.configuration),
                MAX_MANIFEST as u64,
            )?;
            if !reference.content.matches(&bytes) || bytes != campaign.bytes {
                return Err(recovery(
                    "published generation manifest changed before final admission",
                ));
            }
        }
        self.recheck(results, sweep)?;
        publish_bytes(&directory.join("admitted.json"), &self.admission_bytes)?;
        // Even a post-link sync failure above returns NO executable capability.
        // Explicit recovery must distinguish published metadata from starts.
        let entries = self
            .campaigns
            .into_iter()
            .map(|campaign| AdmittedCampaign {
                compatibility: campaign.compatibility,
                invocation: campaign.invocation,
                root: campaign.root,
                plan: campaign.plan,
                completed: campaign.completed,
                manifest_reference: Blob::of(&campaign.bytes),
                manifest: campaign.manifest,
                admission_reference: Blob::of(&self.admission_bytes),
            })
            .collect();
        Ok((self.guard, entries))
    }
}

fn sync_directory(path: &Path) -> Result<(), Error> {
    #[cfg(unix)]
    fs::File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(io_error)?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[cfg(all(test, unix))]
#[path = "generation_tests.rs"]
mod tests;
