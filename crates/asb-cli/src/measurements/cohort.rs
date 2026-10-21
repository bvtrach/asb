//! Dispatch provenance, not a claim that admitted replicas overlapped in time.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionCohort {
    pub id: String,
    pub selected_replicas: usize,
    pub admitted_slots: Vec<usize>,
}

impl ExecutionCohort {
    pub(crate) fn admits(&self, index: usize, total: usize) -> bool {
        self.selected_replicas == total
            && (1..=64).contains(&total)
            && !self.id.is_empty()
            && self.id.len() <= 128
            && self
                .id
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
            && !self.admitted_slots.is_empty()
            && self.admitted_slots.len() <= total
            && self
                .admitted_slots
                .iter()
                .all(|slot| (1..=total).contains(slot))
            && self.admitted_slots.windows(2).all(|pair| pair[0] < pair[1])
            && self.admitted_slots.contains(&index)
    }
}

#[cfg(test)]
mod tests {
    use super::ExecutionCohort;

    #[test]
    fn dispatch_membership_is_bounded_ordered_and_explicit() {
        let mut cohort = ExecutionCohort {
            id: "a-123".into(),
            selected_replicas: 3,
            admitted_slots: vec![2],
        };
        assert!(cohort.admits(2, 3));
        assert!(!cohort.admits(1, 3));
        assert!(!cohort.admits(2, 2));
        for slots in [vec![], vec![0], vec![4], vec![2, 2], vec![3, 2]] {
            cohort.admitted_slots = slots;
            assert!(!cohort.admits(2, 3));
        }
        cohort.admitted_slots = vec![1, 2, 3];
        assert!(cohort.admits(1, 3));
        cohort.id = "../invalid".into();
        assert!(!cohort.admits(1, 3));
    }
}
