//! Registry for selectable benchmark collectors.
//!
//! Collector execution remains specialized in `measurements`, while selection,
//! configuration discovery and tool identity come from one extensible catalog.

pub(crate) trait CollectorAdapter: Sync {
    fn id(&self) -> &'static str;
    fn environment_flag(&self) -> Option<&'static str>;
    fn tool_id(&self) -> Option<&'static str>;
    fn requires_pid(&self) -> bool;
}

pub(crate) struct StaticCollector {
    id: &'static str,
    environment_flag: Option<&'static str>,
    tool_id: Option<&'static str>,
    requires_pid: bool,
}
impl StaticCollector {
    const fn new(
        id: &'static str,
        environment_flag: Option<&'static str>,
        tool_id: Option<&'static str>,
        requires_pid: bool,
    ) -> Self {
        Self {
            id,
            environment_flag,
            tool_id,
            requires_pid,
        }
    }
}
impl CollectorAdapter for StaticCollector {
    fn id(&self) -> &'static str {
        self.id
    }
    fn environment_flag(&self) -> Option<&'static str> {
        self.environment_flag
    }
    fn tool_id(&self) -> Option<&'static str> {
        self.tool_id
    }
    fn requires_pid(&self) -> bool {
        self.requires_pid
    }
}

static SYSTEM: StaticCollector = StaticCollector::new("system", None, None, false);
static RUNTIME: StaticCollector = StaticCollector::new("runtime", None, None, false);
static MPSTAT: StaticCollector =
    StaticCollector::new("mpstat", Some("ASB_MEASURE_MPSTAT"), Some("mpstat"), true);
static IOSTAT: StaticCollector =
    StaticCollector::new("iostat", Some("ASB_MEASURE_IOSTAT"), Some("iostat"), true);
static PERF: StaticCollector =
    StaticCollector::new("perf", Some("ASB_MEASURE_PERF"), Some("perf"), true);
static PERFETTO: StaticCollector = StaticCollector::new(
    "perfetto",
    Some("ASB_MEASURE_PERFETTO"),
    Some("perfetto"),
    true,
);
static FLAMEGRAPH: StaticCollector =
    StaticCollector::new("flamegraph", None, Some("flamegraph"), true);
static BPFTRACE: StaticCollector = StaticCollector::new(
    "bpftrace",
    Some("ASB_MEASURE_BPFTRACE"),
    Some("bpftrace"),
    true,
);
static TOKEN_USAGE: StaticCollector = StaticCollector::new("token_usage", None, None, false);
static STEPS: StaticCollector = StaticCollector::new("steps", None, None, false);
static SUCCESS_RATE: StaticCollector = StaticCollector::new("success_rate", None, None, false);

static REGISTRY: &[&dyn CollectorAdapter] = &[
    &SYSTEM,
    &RUNTIME,
    &MPSTAT,
    &IOSTAT,
    &PERF,
    &PERFETTO,
    &FLAMEGRAPH,
    &BPFTRACE,
    &TOKEN_USAGE,
    &STEPS,
    &SUCCESS_RATE,
];
pub(crate) fn all() -> &'static [&'static dyn CollectorAdapter] {
    REGISTRY
}
pub(crate) fn find(id: &str) -> Option<&'static dyn CollectorAdapter> {
    REGISTRY
        .iter()
        .copied()
        .find(|collector| collector.id() == id)
}
#[cfg(test)]
pub(crate) fn names() -> impl Iterator<Item = &'static str> {
    REGISTRY.iter().map(|collector| collector.id())
}
pub(crate) fn selected_or_environment(id: &str, selected: &[String]) -> bool {
    find(id).is_some_and(|collector| {
        selected.iter().any(|value| value == id)
            || collector
                .environment_flag()
                .is_some_and(super::measurements::env_true)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registry_has_unique_names_and_declares_tool_backed_collectors() {
        let ids = names().collect::<std::collections::BTreeSet<_>>();
        assert_eq!(ids.len(), all().len());
        for id in [
            "mpstat",
            "iostat",
            "perf",
            "perfetto",
            "flamegraph",
            "bpftrace",
        ] {
            assert!(
                find(id).and_then(|collector| collector.tool_id()).is_some(),
                "{id}"
            );
        }
        assert!(find("perf").unwrap().requires_pid());
    }
}
