use super::super::{CollectorAdapter, RunMetrics, RunObservation};
pub(super) struct RuntimeCollector;
pub(super) static ADAPTER: RuntimeCollector = RuntimeCollector;

impl CollectorAdapter for RuntimeCollector {
    fn id(&self) -> &'static str {
        "runtime"
    }
    fn observe_run(&self, observation: &RunObservation, metrics: &mut RunMetrics) {
        metrics.elapsed_ms = observation.elapsed_ms;
    }
}
