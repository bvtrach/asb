//! Pure aggregation and graph identities. No report files are read or written.
use super::*;
pub(super) enum GraphKind {
    Metric(String),
    Series(String),
    Composition,
}
pub(super) struct GraphSpec {
    pub filename: String,
    pub kind: GraphKind,
    pub rows: Vec<usize>,
}
pub(super) struct PreparedReport {
    pub data: ValidatedReport,
    pub graphs: Vec<GraphSpec>,
    pub evaluation: coverage::EvaluationCoverage,
    pub populations: BTreeMap<String, coverage::PopulationCoverage>,
    pub resolution: BTreeMap<String, crate::workloads::ResolutionSummary>,
    pub previews: BTreeMap<String, String>,
}
impl PreparedReport {
    pub(super) fn prepare(data: ValidatedReport) -> Result<Self, Error> {
        let evaluation = coverage::evaluation_summary(&data.rows);
        let populations = coverage::populations(&data.rows)?;
        let mut previews = BTreeMap::new();
        for row in &data.rows {
            if row.result_source == ResultSource::RecoveredOfficialGrade {
                continue;
            }
            for artifact in &row.artifacts {
                let name = artifact.rsplit('/').next().unwrap_or(artifact);
                if matches!(
                    crate::collectors::display_artifact(name),
                    Some(crate::collectors::ArtifactDisplay::Svg { .. })
                ) && !previews.contains_key(artifact)
                {
                    previews.insert(
                        artifact.clone(),
                        source::read_svg(
                            &source::artifact_path(&data.request.comparison_dir, artifact)?,
                            artifact,
                        )?,
                    );
                }
            }
        }
        let mut metrics: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let mut series: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let mut resolution: BTreeMap<String, crate::workloads::ResolutionSummary> = BTreeMap::new();
        for (index, row) in data.rows.iter().enumerate() {
            for name in row.metrics.keys() {
                metrics.entry(name.clone()).or_default().push(index);
            }
            for name in row.series.keys() {
                series.entry(name.clone()).or_default().push(index);
            }
            let key = serde_json::to_string(&(
                &row.agent,
                &row.provider,
                &row.model,
                &row.workload,
                coverage::group_kind(row),
                row.task
                    .as_ref()
                    .and_then(|task| task.dataset.as_ref())
                    .or_else(|| {
                        row.selection
                            .as_ref()
                            .and_then(|selection| selection.dataset.as_ref())
                    }),
            ))
            .map_err(|error| Error::Run(error.to_string()))?;
            resolution.entry(key).or_default().observe(
                coverage::group_kind(row),
                row.metrics
                    .get("success_rate")
                    .is_some_and(|value| *value == 1.0),
            );
        }
        let mut names = GraphNames::default();
        let mut graphs = Vec::new();
        for (name, rows) in metrics {
            graphs.push(GraphSpec {
                filename: names.allocate("metric", &name),
                kind: GraphKind::Metric(name),
                rows,
            });
        }
        for (name, rows) in series {
            graphs.push(GraphSpec {
                filename: names.allocate("series", &name),
                kind: GraphKind::Series(name),
                rows,
            });
        }
        if !composition_rows(&data.rows).is_empty() {
            graphs.push(GraphSpec {
                filename: "execution-timeline.svg".into(),
                kind: GraphKind::Composition,
                rows: Vec::new(),
            });
        }
        Ok(Self {
            data,
            graphs,
            evaluation,
            populations,
            resolution,
            previews,
        })
    }
    pub(super) fn graph_names(&self) -> Vec<String> {
        self.graphs
            .iter()
            .map(|graph| graph.filename.clone())
            .collect()
    }
}
