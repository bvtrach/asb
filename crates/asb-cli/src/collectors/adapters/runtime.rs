use super::super::{RunMetrics, RunObservation, StaticCollector};
pub(super) static ADAPTER: StaticCollector =
    StaticCollector::new("runtime", None, None, false).with_run_observation(observe);

fn observe(observation: &RunObservation, metrics: &mut RunMetrics) {
    metrics.elapsed_ms = observation.elapsed_ms;
}
