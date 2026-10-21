//! Immutable identities and the cross-row validation boundary.
use super::*;
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
pub(super) struct RowId {
    pub configuration: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch: Option<u64>,
    pub replica: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum ResultSource {
    Missing,
    Measurements,
    RunLog,
    TaskOutcome,
    ImportedOfficialGrade,
    RecoveredOfficialGrade,
}
impl ResultSource {
    pub(super) fn parse(value: &str) -> Result<Self, Error> {
        match value {
            "missing" => Ok(Self::Missing),
            "measurements" => Ok(Self::Measurements),
            "run-log" => Ok(Self::RunLog),
            "task-outcome" => Ok(Self::TaskOutcome),
            "imported-official-grade" => Ok(Self::ImportedOfficialGrade),
            "recovered-official-grade" => Ok(Self::RecoveredOfficialGrade),
            _ => Err(Error::Input("unknown report result source".into())),
        }
    }
}
pub(super) struct ValidatedReport {
    pub request: source::Request,
    pub rows: Vec<BenchmarkData>,
    pub previews: BTreeMap<String, String>,
}
impl ValidatedReport {
    pub(super) fn validate(loaded: source::LoadedReport) -> Result<Self, Error> {
        let mut cohorts = BTreeMap::new();
        let mut attempts = BTreeSet::new();
        let mut ids = BTreeSet::new();
        for row in &loaded.rows {
            if !ids.insert(row.id) {
                return Err(Error::Input("duplicate report replica row identity".into()));
            }
            if let Some(task) = &row.task
                && !attempts.insert(task.attempt_id.clone())
            {
                return Err(Error::Input(
                    "duplicate actual attempt identity; an attempt cannot be counted twice".into(),
                ));
            }
            if let Some(cohort) = &row.execution_cohort
                && let Some(previous) = cohorts.insert(&cohort.id, cohort)
                && previous != cohort
            {
                return Err(Error::Input(
                    "conflicting execution cohort declarations in sweep measurements".into(),
                ));
            }
        }
        Ok(Self {
            request: loaded.request,
            rows: loaded.rows,
            previews: loaded.previews,
        })
    }
}
