//! Shared report/replay metadata context. Never an executable resume capability.
use super::*;
use crate::runner::campaign::{generation, reading, reading::Layout};

impl SweepPlan {
    pub(crate) fn generation_catalog(
        &self,
        results: &Path,
        layouts: &[Layout],
    ) -> Result<Option<generation::Catalog>, Error> {
        let checkpoint = results.join(".checkpoint");
        if !reading::present_relative(results, &Path::new(".checkpoint").join(generation::ROOT))? {
            return Ok(None);
        }
        let seal: serde_json::Value = serde_json::from_slice(&reading::bounded_relative(
            &checkpoint,
            Path::new("inputs.json"),
            16 * 1024 * 1024,
        )?)
        .map_err(|_| Error::Input("invalid original campaign input seal".into()))?;
        let digest = seal["initial"]["digest"]
            .as_str()
            .ok_or_else(|| Error::Input("original campaign input identity missing".into()))?;
        let bytes = reading::bounded(&results.join("plan.json"), MAX_PLAN_BYTES as u64)?;
        let saved: SweepPlan = serde_json::from_slice(&bytes)
            .map_err(|_| Error::Input("invalid retained sweep index".into()))?;
        if serde_json::to_value(saved).map_err(|error| Error::Input(error.to_string()))?
            != serde_json::to_value(self).map_err(|error| Error::Input(error.to_string()))?
        {
            return Err(Error::Input(
                "retained sweep index changed during metadata loading".into(),
            ));
        }
        let expected = self
            .entries
            .iter()
            .zip(layouts)
            .map(|(entry, layout)| {
                Ok(generation::Expected {
                    reference: entry.campaign.as_ref().ok_or_else(|| {
                        Error::Input("missing original campaign reference".into())
                    })?,
                    plan: &layout.plan,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        generation::Catalog::read(
            &checkpoint,
            generation::Context {
                run_id: &self.run_id,
                input_digest: digest,
                original_index: generation::Blob::of(&bytes),
            },
            &expected,
        )
        .map(Some)
    }
}
