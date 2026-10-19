use super::super::{RunMetrics, RunObservation, StaticCollector};
pub(super) static ADAPTER: StaticCollector =
    StaticCollector::new("success_rate", None, None, false).with_run_observation(observe);

fn observe(observation: &RunObservation, metrics: &mut RunMetrics) {
    metrics.success_rate = if observation.success { 1.0 } else { 0.0 };
}
