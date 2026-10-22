//! Immutable campaign inputs only; not attempt settlement or resume authority.
use super::Root;
use crate::{Error, runner::repeated::InputCost, sweep::plan::SweepPlan};
use std::path::Path;

mod reservations;
mod settled;
pub(in crate::sweep) use settled::continuation::{
    AdmittedCampaign, CampaignContinuation, Guard, PreparedContinuation,
};

pub(super) fn input_roots(results: &Path, plan: &SweepPlan) -> Result<Vec<Root>, Error> {
    // Share the exact index/configuration/selection/capacity binding used by
    // comparison. No outcome, growing journal or recording payload is read.
    let layouts = plan.read_campaigns(results)?;
    admit_cost(&layouts)?;
    let mut roots = Vec::new();
    for (index, layout) in layouts.iter().enumerate() {
        roots.push(Root::path(
            format!("campaign-{index}-plan"),
            layout.root.join("plan.json"),
        ));
        roots.push(Root::path(
            format!("campaign-{index}-configuration"),
            layout.plan.source.clone(),
        ));
        for (input_index, input) in layout.plan.replay_inputs.iter().enumerate() {
            roots.push(Root::path(
                format!("campaign-{index}-replay-{input_index}"),
                layout.root.join(&input.private),
            ));
        }
    }
    Ok(roots)
}

fn admit_cost(layouts: &[crate::runner::campaign::reading::Layout]) -> Result<(), Error> {
    let mut cost = InputCost::default();
    for layout in layouts {
        cost = cost.checked_add(InputCost {
            inputs: layout.plan.replay_inputs.len(),
            bytes: layout
                .plan
                .replay_inputs
                .iter()
                .map(|input| input.bytes)
                .sum(),
            ..Default::default()
        })?;
    }
    Ok(())
}

/// Original private payload declarations must agree before a modern input seal
/// can authorize further continuation checks. No directory or receipt is made.
pub(super) fn verify_private_inputs(results: &Path, plan: &SweepPlan) -> Result<(), Error> {
    let layouts = plan.read_campaigns(results)?;
    admit_cost(&layouts)?;
    for layout in &layouts {
        layout.inspect_private_replay()?;
    }
    for layout in &layouts {
        layout.verify_private_replay()?;
    }
    Ok(())
}

#[cfg(all(test, unix))]
pub(in crate::sweep::checkpoint) mod tests;
