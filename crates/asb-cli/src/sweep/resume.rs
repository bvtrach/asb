//! Validated continuation decisions; execution owns summary presentation.
pub(super) use crate::runner::reuse::render_reused;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct State {
    pub groups: BTreeMap<usize, Vec<Value>>,
    pub slots: BTreeMap<usize, BTreeMap<usize, Value>>,
    pub campaign_owners: Vec<crate::output_lock::OutputLock>,
}

impl std::fmt::Debug for State {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ContinuationState")
            .field("completed_groups", &self.groups.len())
            .field("partial_groups", &self.slots.len())
            .field("owned_campaigns", &self.campaign_owners.len())
            .finish()
    }
}

#[cfg(test)]
impl State {
    pub(super) fn has_no_saved_outcomes(&self) -> bool {
        self.groups.is_empty() && self.slots.is_empty()
    }
}
