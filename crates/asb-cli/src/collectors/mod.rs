//! Registry for selectable benchmark collectors.
//!
//! Runtime behavior, presentation and tool identity are dispatched through an
//! extensible catalog of collector-specific adapters.

pub(crate) mod adapters;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub(crate) struct ProcessContext<'a> {
    pub name: &'a str,
    pub status: &'a std::process::ExitStatus,
    pub stdout: &'a str,
    pub stderr: &'a str,
}

pub(crate) struct ProcessData {
    pub status: String,
    pub metrics: BTreeMap<String, f64>,
    pub series: BTreeMap<String, Vec<f64>>,
    pub timestamps: BTreeMap<String, Vec<f64>>,
    pub artifact: Option<String>,
}

impl ProcessData {
    pub fn from_output(context: &ProcessContext<'_>) -> Self {
        let completed = context.status.success()
            || (context.status.code().is_none() && !context.stdout.is_empty());
        Self {
            status: if completed { "completed" } else { "failed" }.into(),
            metrics: BTreeMap::new(),
            series: BTreeMap::new(),
            timestamps: BTreeMap::new(),
            artifact: None,
        }
    }
}

pub(crate) struct PreflightContext<'a> {
    pub measurement_dir: &'a Path,
    pub selected: &'a [String],
    pub tool_dir: Option<&'a Path>,
    pub sudo_collectors: &'a mut BTreeSet<String>,
}

pub(crate) struct UsageContext<'a> {
    pub provider: &'a str,
    pub agent: &'a str,
    pub stdout: &'a str,
    pub stderr: &'a str,
    pub cassette: Option<&'a Path>,
}

pub(crate) struct RunObservation {
    pub elapsed_ms: u128,
    pub success: bool,
}

#[derive(Default)]
pub(crate) struct RunMetrics {
    pub elapsed_ms: u128,
    pub success_rate: f64,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum SharedArtifact {
    StackSamples,
}

pub(crate) enum ArtifactDisplay {
    Svg {
        class: &'static str,
        label: &'static str,
    },
    Status {
        class: &'static str,
        label: &'static str,
        suffix: &'static str,
    },
}

pub(crate) struct GraphSection {
    pub title: &'static str,
    pub description: &'static str,
}

/// Non-secret benchmark lifecycle events available to every collector.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(tag = "kind", content = "phase", rename_all = "snake_case")]
pub(crate) enum LifecycleEvent {
    RunStarted,
    RunFinishing,
    RunFailed,
    PhaseBegin(LifecyclePhase),
    PhaseEnd(LifecyclePhase),
    PhaseFailed(LifecyclePhase),
    MeasurementsReady,
    AgentSpawn,
    AgentExecuteBegin,
    AgentStarted,
    AgentSpawnFailed,
    AgentAttachFailed,
    AgentWaitFailed,
    AgentExecuteEnd,
    AgentExited,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LifecyclePhase {
    ProviderDiscovery,
    ProviderSetup,
    WorkloadPreparation,
    ProxyPreparation,
    AgentPreparation,
    WorkloadEvaluation,
}

impl LifecyclePhase {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::ProviderDiscovery => "provider.discovery",
            Self::ProviderSetup => "provider.setup",
            Self::WorkloadPreparation => "workload.prepare",
            Self::ProxyPreparation => "proxy.prepare",
            Self::AgentPreparation => "agent.prepare",
            Self::WorkloadEvaluation => "workload.evaluate",
        }
    }
}

pub(crate) trait CollectorAdapter: Sync {
    fn id(&self) -> &'static str;
    fn environment_flag(&self) -> Option<&'static str>;
    fn tool_id(&self) -> Option<&'static str>;
    fn requires_pid(&self) -> bool;
    fn usage(&self, _context: &UsageContext<'_>) -> Option<crate::measurements::TokenUsage> {
        None
    }
    fn usage_file(&self, _path: &Path) -> Option<crate::measurements::TokenUsage> {
        None
    }
    fn steps(&self, _agent: &str, _stdout: &str, _stderr: &str) -> Option<u64> {
        None
    }
    fn observe_run(&self, _observation: &RunObservation, _metrics: &mut RunMetrics) {}
    fn configure(
        &self,
        _session: &mut crate::measurements::Session,
        _selected: &[String],
    ) -> Result<(), String> {
        Ok(())
    }
    fn preflight(&self, _context: &mut PreflightContext<'_>) -> Result<(), String> {
        Ok(())
    }
    fn prioritizes_artifact(&self, _name: &str) -> bool {
        false
    }
    fn display_artifact(&self, _name: &str) -> Option<ArtifactDisplay> {
        None
    }
    fn graph_section(&self, _name: &str) -> Option<GraphSection> {
        None
    }
    fn graceful_shutdown(&self, _name: &str) -> bool {
        false
    }
    fn process(
        &self,
        _session: &mut crate::measurements::Session,
        context: &ProcessContext<'_>,
    ) -> Result<ProcessData, String> {
        Ok(ProcessData::from_output(context))
    }
    fn finish(&self, _session: &mut crate::measurements::Session) -> Result<(), String> {
        Ok(())
    }
    /// Start collectors that need to observe events before the child exists.
    fn start(&self, _session: &mut crate::measurements::Session) -> Result<(), String> {
        Ok(())
    }
    fn lifecycle_event(
        &self,
        _session: &mut crate::measurements::Session,
        _event: LifecycleEvent,
    ) -> Result<(), String> {
        Ok(())
    }
    /// Attach this collector to an already spawned benchmark process.
    fn attach_pid(
        &self,
        _session: &mut crate::measurements::Session,
        _pid: u32,
    ) -> Result<(), String> {
        Ok(())
    }
}

type SessionHook = fn(&mut crate::measurements::Session) -> Result<(), String>;
type AttachHook = fn(&mut crate::measurements::Session, u32) -> Result<(), String>;
type ConfigureHook = fn(&mut crate::measurements::Session, &[String]) -> Result<(), String>;
type PreflightHook = fn(&mut PreflightContext<'_>) -> Result<(), String>;
type ProcessHook =
    fn(&mut crate::measurements::Session, &ProcessContext<'_>) -> Result<ProcessData, String>;
type EventHook = fn(&mut crate::measurements::Session, LifecycleEvent) -> Result<(), String>;

pub(crate) struct StaticCollector {
    id: &'static str,
    environment_flag: Option<&'static str>,
    tool_id: Option<&'static str>,
    requires_pid: bool,
    usage: Option<fn(&UsageContext<'_>) -> crate::measurements::TokenUsage>,
    usage_file: Option<fn(&Path) -> crate::measurements::TokenUsage>,
    steps: Option<fn(&str, &str, &str) -> u64>,
    observe_run: Option<fn(&RunObservation, &mut RunMetrics)>,
    attach_pid: Option<AttachHook>,
    start: Option<SessionHook>,
    configure: Option<ConfigureHook>,
    preflight: Option<PreflightHook>,
    prioritizes_artifact: Option<fn(&str) -> bool>,
    display_artifact: Option<fn(&str) -> Option<ArtifactDisplay>>,
    graph_section: Option<fn(&str) -> Option<GraphSection>>,
    graceful_shutdown: Option<fn(&str) -> bool>,
    process: Option<ProcessHook>,
    finish: Option<SessionHook>,
    lifecycle_event: Option<EventHook>,
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
            usage: None,
            usage_file: None,
            steps: None,
            observe_run: None,
            attach_pid: None,
            start: None,
            configure: None,
            preflight: None,
            prioritizes_artifact: None,
            display_artifact: None,
            graph_section: None,
            graceful_shutdown: None,
            process: None,
            finish: None,
            lifecycle_event: None,
        }
    }
    const fn with_usage(
        mut self,
        usage: fn(&UsageContext<'_>) -> crate::measurements::TokenUsage,
        usage_file: fn(&Path) -> crate::measurements::TokenUsage,
    ) -> Self {
        self.usage = Some(usage);
        self.usage_file = Some(usage_file);
        self
    }
    const fn with_steps(mut self, steps: fn(&str, &str, &str) -> u64) -> Self {
        self.steps = Some(steps);
        self
    }
    const fn with_run_observation(mut self, hook: fn(&RunObservation, &mut RunMetrics)) -> Self {
        self.observe_run = Some(hook);
        self
    }
    const fn with_attach_pid(
        mut self,
        hook: fn(&mut crate::measurements::Session, u32) -> Result<(), String>,
    ) -> Self {
        self.attach_pid = Some(hook);
        self
    }
    const fn with_start(
        mut self,
        hook: fn(&mut crate::measurements::Session) -> Result<(), String>,
    ) -> Self {
        self.start = Some(hook);
        self
    }
    const fn with_configure(
        mut self,
        hook: fn(&mut crate::measurements::Session, &[String]) -> Result<(), String>,
    ) -> Self {
        self.configure = Some(hook);
        self
    }
    const fn with_preflight(
        mut self,
        hook: fn(&mut PreflightContext<'_>) -> Result<(), String>,
    ) -> Self {
        self.preflight = Some(hook);
        self
    }
    const fn with_artifacts(
        mut self,
        priority: fn(&str) -> bool,
        display: fn(&str) -> Option<ArtifactDisplay>,
    ) -> Self {
        self.prioritizes_artifact = Some(priority);
        self.display_artifact = Some(display);
        self
    }
    const fn with_processing(
        mut self,
        graceful: fn(&str) -> bool,
        process: fn(
            &mut crate::measurements::Session,
            &ProcessContext<'_>,
        ) -> Result<ProcessData, String>,
    ) -> Self {
        self.graceful_shutdown = Some(graceful);
        self.process = Some(process);
        self
    }
    const fn with_graph_section(mut self, hook: fn(&str) -> Option<GraphSection>) -> Self {
        self.graph_section = Some(hook);
        self
    }
    const fn with_finish(
        mut self,
        hook: fn(&mut crate::measurements::Session) -> Result<(), String>,
    ) -> Self {
        self.finish = Some(hook);
        self
    }
    const fn with_lifecycle_event(
        mut self,
        hook: fn(&mut crate::measurements::Session, LifecycleEvent) -> Result<(), String>,
    ) -> Self {
        self.lifecycle_event = Some(hook);
        self
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
    fn usage(&self, context: &UsageContext<'_>) -> Option<crate::measurements::TokenUsage> {
        self.usage.map(|hook| hook(context))
    }
    fn usage_file(&self, path: &Path) -> Option<crate::measurements::TokenUsage> {
        self.usage_file.map(|hook| hook(path))
    }
    fn steps(&self, agent: &str, stdout: &str, stderr: &str) -> Option<u64> {
        self.steps.map(|hook| hook(agent, stdout, stderr))
    }
    fn observe_run(&self, observation: &RunObservation, metrics: &mut RunMetrics) {
        if let Some(hook) = self.observe_run {
            hook(observation, metrics);
        }
    }
    fn configure(
        &self,
        session: &mut crate::measurements::Session,
        selected: &[String],
    ) -> Result<(), String> {
        self.configure
            .map_or(Ok(()), |hook| hook(session, selected))
    }
    fn preflight(&self, context: &mut PreflightContext<'_>) -> Result<(), String> {
        self.preflight.map_or(Ok(()), |hook| hook(context))
    }
    fn prioritizes_artifact(&self, name: &str) -> bool {
        self.prioritizes_artifact.is_some_and(|hook| hook(name))
    }
    fn display_artifact(&self, name: &str) -> Option<ArtifactDisplay> {
        self.display_artifact.and_then(|hook| hook(name))
    }
    fn graph_section(&self, name: &str) -> Option<GraphSection> {
        self.graph_section.and_then(|hook| hook(name))
    }
    fn graceful_shutdown(&self, name: &str) -> bool {
        self.graceful_shutdown.is_some_and(|hook| hook(name))
    }
    fn process(
        &self,
        session: &mut crate::measurements::Session,
        context: &ProcessContext<'_>,
    ) -> Result<ProcessData, String> {
        self.process.map_or_else(
            || Ok(ProcessData::from_output(context)),
            |hook| hook(session, context),
        )
    }
    fn finish(&self, session: &mut crate::measurements::Session) -> Result<(), String> {
        self.finish.map_or(Ok(()), |hook| hook(session))
    }
    fn start(&self, session: &mut crate::measurements::Session) -> Result<(), String> {
        self.start.map_or(Ok(()), |hook| hook(session))
    }
    fn lifecycle_event(
        &self,
        session: &mut crate::measurements::Session,
        event: LifecycleEvent,
    ) -> Result<(), String> {
        self.lifecycle_event
            .map_or(Ok(()), |hook| hook(session, event))
    }
    fn attach_pid(
        &self,
        session: &mut crate::measurements::Session,
        pid: u32,
    ) -> Result<(), String> {
        self.attach_pid.map_or(Ok(()), |hook| hook(session, pid))
    }
}

pub(crate) fn all() -> &'static [&'static dyn CollectorAdapter] {
    adapters::REGISTRY
}
pub(crate) fn run_metrics(observation: &RunObservation) -> RunMetrics {
    let mut metrics = RunMetrics::default();
    for adapter in all() {
        adapter.observe_run(observation, &mut metrics);
    }
    metrics
}
pub(crate) fn find(id: &str) -> Option<&'static dyn CollectorAdapter> {
    all().iter().copied().find(|collector| collector.id() == id)
}
pub(crate) fn prioritizes_artifact(name: &str) -> bool {
    all()
        .iter()
        .any(|adapter| adapter.prioritizes_artifact(name))
}
pub(crate) fn display_artifact(name: &str) -> Option<ArtifactDisplay> {
    all()
        .iter()
        .find_map(|adapter| adapter.display_artifact(name))
}
pub(crate) fn graph_section(name: &str) -> Option<GraphSection> {
    all().iter().find_map(|adapter| adapter.graph_section(name))
}
#[cfg(test)]
pub(crate) fn names() -> impl Iterator<Item = &'static str> {
    all().iter().map(|collector| collector.id())
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

    #[test]
    fn built_in_run_metrics_are_computed_by_registered_adapters() {
        let success = run_metrics(&RunObservation {
            elapsed_ms: 124,
            success: true,
        });
        assert_eq!(success.elapsed_ms, 124);
        assert_eq!(success.success_rate, 1.0);
        let failure = run_metrics(&RunObservation {
            elapsed_ms: 9,
            success: false,
        });
        assert_eq!(failure.elapsed_ms, 9);
        assert_eq!(failure.success_rate, 0.0);
    }

    #[test]
    fn generic_runtime_does_not_dispatch_on_collector_ids() {
        fn direct_dispatch(source: &str, name: &str) -> bool {
            let code: String = source
                .split("#[cfg(test)]")
                .next()
                .unwrap_or(source)
                .chars()
                .filter(|character| !character.is_whitespace())
                .collect();
            let literal = format!("\"{name}\"");
            [
                "collector",
                "collector.name",
                "collector.id()",
                "collector_id",
            ]
            .iter()
            .any(|value| {
                code.contains(&format!("{value}=={literal}"))
                    || code.contains(&format!("{literal}=={value}"))
            }) || [
                "matchcollector{",
                "matchcollector.name.as_str(){",
                "matchcollector_id{",
            ]
            .iter()
            .filter_map(|value| code.split_once(value).map(|(_, tail)| tail))
            .any(|tail| {
                tail.split('}')
                    .next()
                    .unwrap_or("")
                    .contains(&format!("{literal}=>"))
            }) || code.contains(&format!(".starts_with(\"metric-{name}-\")"))
                || code.contains(&format!(".starts_with(\"series-{name}-\")"))
        }
        assert!(direct_dispatch(
            "if collector == \"perfetto\" {}",
            "perfetto"
        ));
        let source_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        for filename in [
            "measurements.rs",
            "runner.rs",
            "install.rs",
            "tui.rs",
            "compare.rs",
        ] {
            let source = std::fs::read_to_string(source_dir.join(filename)).unwrap();
            for adapter in all() {
                assert!(
                    !direct_dispatch(&source, adapter.id()),
                    "collector-specific dispatch for {} in {filename}",
                    adapter.id()
                );
            }
        }
    }
}
