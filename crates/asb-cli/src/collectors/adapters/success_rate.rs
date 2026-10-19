use super::super::{CollectorAdapter, RunMetrics, RunObservation};
pub(super) struct SuccessRateCollector;
pub(super) static ADAPTER: SuccessRateCollector = SuccessRateCollector;

impl CollectorAdapter for SuccessRateCollector {
    fn id(&self) -> &'static str {
        "success_rate"
    }
    fn observe_run(&self, observation: &RunObservation, metrics: &mut RunMetrics) {
        metrics.success_rate = if observation.success { 1.0 } else { 0.0 };
    }
}
