//! Immutable campaign inputs only; not attempt settlement or resume authority.
use super::Root;
use crate::{Error, runner::repeated::InputCost, sweep::plan::SweepPlan};
use std::path::Path;

pub(super) fn input_roots(results: &Path, plan: &SweepPlan) -> Result<Vec<Root>, Error> {
    // Share the exact index/configuration/selection/capacity binding used by
    // comparison. No outcome, growing journal or recording payload is read.
    let layouts = plan.read_campaigns(results)?;
    let mut cost = InputCost::default();
    for layout in &layouts {
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

#[cfg(all(test, unix))]
mod tests;
