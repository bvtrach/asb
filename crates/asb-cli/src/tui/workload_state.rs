//! In-memory workload editing; only an explicit finish requests publication.
use super::{BTreeSet, Pane};
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) enum WorkloadStep {
    #[default]
    Browsing,
    Editing {
        workload: String,
        trials: Pane,
    },
    Saved {
        workload: String,
    },
}

pub(super) enum WorkloadAction {
    Open { workload: String, trials: Pane },
    Save,
    Back,
    Browse,
    Finish,
}

#[derive(Default)]
pub(super) struct WorkloadWizard {
    pub step: WorkloadStep,
    pub selections: BTreeMap<String, BTreeSet<String>>,
}

impl WorkloadWizard {
    pub fn editing(&self) -> bool {
        matches!(self.step, WorkloadStep::Editing { .. })
    }

    /// Returns true only for Finish from Saved. Saving never writes files.
    pub fn reduce(&mut self, action: WorkloadAction) -> Result<bool, &'static str> {
        match action {
            WorkloadAction::Open {
                workload,
                mut trials,
            } => {
                if let Some(saved) = self.selections.get(&workload) {
                    for (choice, selected) in trials.choices.iter().zip(&mut trials.selected) {
                        *selected = saved.contains(&choice.label);
                    }
                }
                self.step = WorkloadStep::Editing { workload, trials };
            }
            WorkloadAction::Save => {
                let WorkloadStep::Editing { workload, trials } = &self.step else {
                    return Err("Open a workload before saving its trials.");
                };
                if !trials.has_selection() {
                    return Err("Select at least one trial before continuing.");
                }
                self.selections.insert(
                    workload.clone(),
                    trials
                        .selected_choices()
                        .map(|choice| choice.label.clone())
                        .collect(),
                );
                self.step = WorkloadStep::Saved {
                    workload: workload.clone(),
                };
            }
            WorkloadAction::Back | WorkloadAction::Browse => {
                self.step = WorkloadStep::Browsing;
            }
            WorkloadAction::Finish => {
                if matches!(self.step, WorkloadStep::Saved { .. }) {
                    return Ok(true);
                }
                return Err("Save a workload's trials before finishing.");
            }
        }
        Ok(false)
    }
}
