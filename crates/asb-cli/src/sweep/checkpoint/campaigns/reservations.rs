//! Compact immutable slot reservations, finalized only after the input seal.
//! A reservation is intent, not evidence that any attempt completed or stopped.
use super::super::{Fingerprint, Inputs, Root, completed, resume_error};
use crate::{Error, runner::campaign::Reference, sweep::plan::SweepPlan};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const SCOPE: &str = "campaign-slot-reservations-not-attempt-evidence";
const MAX_BYTES: usize = 64 * 1024 * 1024;

#[derive(Serialize)]
struct Reservation<'a> {
    schema_version: u32,
    scope: &'static str,
    run_id: &'a str,
    input_digest: &'a str,
    configuration: usize,
    campaign: &'a str,
    reference: &'a Reference,
    repeat: u64,
    concurrency: usize,
    slots: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Admission {
    schema_version: u32,
    scope: String,
    run_id: String,
    input_digest: String,
    manifests: usize,
    fingerprint: Fingerprint,
}

struct Manifest {
    path: PathBuf,
    value: Value,
}

impl Inputs {
    fn reservation_manifests(
        &self,
        results: &Path,
        plan: &SweepPlan,
    ) -> Result<Vec<Manifest>, Error> {
        let layouts = plan.read_campaigns(results)?;
        super::admit_cost(&layouts)?;
        let mut total = 0usize;
        let mut manifests = Vec::with_capacity(layouts.len());
        for (index, (entry, layout)) in plan.entries.iter().zip(&layouts).enumerate() {
            let reservation = Reservation {
                schema_version: 1,
                scope: SCOPE,
                run_id: &plan.run_id,
                input_digest: &self.initial.digest,
                configuration: index,
                campaign: &layout.plan.campaign,
                reference: entry
                    .campaign
                    .as_ref()
                    .expect("validated campaign reference"),
                repeat: layout.plan.repeat,
                concurrency: layout.plan.concurrency,
                slots: layout.plan.slots.len() as u64,
            };
            let value = serde_json::to_value(&reservation)
                .map_err(|_| resume_error("cannot encode campaign reservations"))?;
            let bytes = serde_json::to_vec_pretty(&value)
                .map_err(|_| resume_error("cannot encode campaign reservations"))?;
            total = total
                .checked_add(bytes.len())
                .filter(|total| *total <= MAX_BYTES)
                .ok_or_else(|| resume_error("campaign reservation metadata exceeds 64 MiB"))?;
            if bytes.len() > 1024 * 1024 {
                return Err(resume_error(
                    "individual campaign reservation exceeds 1 MiB",
                ));
            }
            manifests.push(Manifest {
                path: self
                    .directory
                    .join("campaign-reservations")
                    .join(format!("{index:06}.json")),
                value,
            });
        }
        Ok(manifests)
    }

    fn reservation_context(&self, plan: &SweepPlan) -> Value {
        // The original input digest already binds exact selection-plan bytes.
        // The expected membership is recomputed from that validated index, not
        // accepted from a marker or inferred from a directory listing.
        json!({"protocol":"asb-campaign-admission-v1","scope":SCOPE,
            "run_id":plan.run_id,"input_digest":self.initial.digest,
            "configurations":plan.entries.len()})
    }

    fn admission_path(&self) -> PathBuf {
        self.directory.join("campaign-admission.json")
    }

    pub(in crate::sweep::checkpoint) fn reserve_campaigns(
        &self,
        results: &Path,
        plan: &SweepPlan,
    ) -> Result<(), Error> {
        let manifests = self.reservation_manifests(results, plan)?;
        for manifest in &manifests {
            // Reuse the bounded create-new, synced metadata publisher.
            completed::publish(&manifest.path, &manifest.value)?;
        }
        // Verify the exact bytes actually published before creating authority.
        self.check_reservation_manifests(&manifests)?;
        let roots = manifest_roots(&manifests);
        let fingerprint =
            self.snapshotter
                .fingerprint(&roots, &self.reservation_context(plan), false, &[])?;
        completed::publish(
            &self.admission_path(),
            &Admission {
                schema_version: 1,
                scope: SCOPE.into(),
                run_id: plan.run_id.clone(),
                input_digest: self.initial.digest.clone(),
                manifests: manifests.len(),
                fingerprint,
            },
        )
    }

    fn check_reservation_manifests(&self, manifests: &[Manifest]) -> Result<(), Error> {
        let directory = self.directory.join("campaign-reservations");
        if !std::fs::symlink_metadata(&directory).is_ok_and(|metadata| metadata.is_dir()) {
            return Err(resume_error(
                "campaign reservation directory is missing or aliased",
            ));
        }
        for manifest in manifests {
            let expected = serde_json::to_vec_pretty(&manifest.value)
                .map_err(|_| resume_error("cannot encode campaign reservations"))?;
            let bytes =
                crate::runner::campaign::reading::bounded(&manifest.path, expected.len() as u64)?;
            if bytes != expected {
                return Err(resume_error(
                    "campaign reservation differs from its original input/slot declaration",
                ));
            }
        }
        Ok(())
    }

    pub(in crate::sweep::checkpoint) fn verify_campaign_reservations(
        &self,
        results: &Path,
        plan: &SweepPlan,
    ) -> Result<(), Error> {
        let manifests = self.reservation_manifests(results, plan)?;
        let bytes = crate::runner::campaign::reading::bounded(&self.admission_path(), 4096)?;
        let admission: Admission = serde_json::from_slice(&bytes)
            .map_err(|_| resume_error("campaign admission marker is malformed"))?;
        if admission.schema_version != 1
            || admission.scope != SCOPE
            || admission.run_id != plan.run_id
            || admission.input_digest != self.initial.digest
            || admission.manifests != manifests.len()
            || !admission.fingerprint.valid()
        {
            return Err(resume_error(
                "campaign admission marker differs from original reservations",
            ));
        }
        self.check_reservation_manifests(&manifests)?;
        let actual = self.snapshotter.fingerprint(
            &manifest_roots(&manifests),
            &self.reservation_context(plan),
            false,
            &[],
        )?;
        if actual != admission.fingerprint {
            return Err(resume_error("campaign reservation evidence changed"));
        }
        Ok(())
    }
}

fn manifest_roots(manifests: &[Manifest]) -> Vec<Root> {
    manifests
        .iter()
        .enumerate()
        .map(|(index, manifest)| {
            Root::path(
                format!("campaign-reservation-{index}"),
                manifest.path.clone(),
            )
        })
        .collect()
}
