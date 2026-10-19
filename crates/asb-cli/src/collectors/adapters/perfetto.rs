use super::super::{
    ArtifactDisplay, LifecycleEvent, PreflightContext, ProcessContext, ProcessData, StaticCollector,
};
use crate::measurements;
pub(super) static ADAPTER: StaticCollector = StaticCollector::new(
    "perfetto",
    Some("ASB_MEASURE_PERFETTO"),
    Some("perfetto"),
    true,
)
.with_artifacts(
    |name| name.starts_with("perfetto-metric-"),
    |name| {
        (name.starts_with("perfetto-metric-") && name.ends_with(".svg")).then_some(
            ArtifactDisplay::Svg {
                class: "perfetto-metric",
                label: "Perfetto metric SVG",
            },
        )
    },
)
.with_preflight(preflight)
.with_configure(|session, selected| {
    session.state::<PerfettoState>().borrow_mut().enabled =
        super::super::selected_or_environment("perfetto", selected);
    Ok(())
})
.with_processing(|_| true, process)
.with_finish(finish)
.with_start(|session| session.start_perfetto())
.with_lifecycle_event(|session, event| {
    let pid = std::process::id();
    let id = session.trace_id();
    let marker = match event {
        LifecycleEvent::RunStarted => format!("I|{pid}|asb.run.started.{id}"),
        LifecycleEvent::RunFinishing => format!("I|{pid}|asb.run.finishing.{id}"),
        LifecycleEvent::RunFailed => format!("I|{pid}|asb.run.failed.{id}"),
        LifecycleEvent::PhaseBegin(phase) => format!("B|{pid}|asb.{}.{id}", phase.name()),
        LifecycleEvent::PhaseEnd(_) => "E".into(),
        LifecycleEvent::PhaseFailed(phase) => format!("I|{pid}|asb.{}.failed.{id}", phase.name()),
        LifecycleEvent::MeasurementsReady => format!("I|{pid}|asb.measurements.ready.{id}"),
        LifecycleEvent::AgentSpawn => format!("I|{pid}|asb.agent.spawn.{id}"),
        LifecycleEvent::AgentExecuteBegin => format!("B|{pid}|asb.agent.execute.{id}"),
        LifecycleEvent::AgentStarted => format!("I|{pid}|asb.agent.started.{id}"),
        LifecycleEvent::AgentSpawnFailed => format!("I|{pid}|asb.agent.spawn_failed.{id}"),
        LifecycleEvent::AgentAttachFailed => format!("I|{pid}|asb.agent.attach_failed.{id}"),
        LifecycleEvent::AgentWaitFailed => format!("I|{pid}|asb.agent.wait_failed.{id}"),
        LifecycleEvent::AgentExecuteEnd => "E".into(),
        LifecycleEvent::AgentExited => format!("I|{pid}|asb.agent.exited.{id}"),
    };
    session.write_trace_marker(&marker)
})
.with_attach_pid(|session, pid| {
    session.set_perfetto_pid(pid);
    Ok(())
});

fn process(
    session: &mut measurements::Session,
    context: &ProcessContext<'_>,
) -> Result<ProcessData, String> {
    let mut result = ProcessData::from_output(context);
    result.metrics = session.perfetto_trace_metrics();
    (result.series, result.timestamps) = session.perfetto_trace_series();
    let state = session.state::<PerfettoState>();
    let mut state = state.borrow_mut();
    state.metrics = result.metrics.clone();
    result.artifact = state
        .trace
        .as_ref()
        .filter(|path| path.is_file())
        .map(|path| measurements::file_name(path));
    if context.status.code().is_none()
        && state
            .trace
            .as_ref()
            .is_some_and(|path| path.metadata().is_ok_and(|metadata| metadata.len() > 0))
    {
        result.status = "completed".into();
    }
    if context.stderr.contains("FtraceController") {
        result.status = "unavailable".into();
    }
    Ok(result)
}

fn finish(session: &mut measurements::Session) -> Result<(), String> {
    let state = session.state::<PerfettoState>();
    let state = state.borrow();
    if !state.enabled {
        return Ok(());
    }
    measurements::write_json(
        &session.dir.join("perfetto-tracepoints.json"),
        &serde_json::json!({
            "schema_version": 1,
            "collector": "perfetto",
            "tracepoint_count": state.tracepoint_count,
            "trace_marker": state.trace_marker.as_ref().map(|path| path.display().to_string()),
            "trace": state.trace.as_ref().map(|path| measurements::file_name(path)),
        }),
    )?;
    write_perfetto_metric_svgs(
        &session.dir,
        state.trace.as_deref(),
        state.tracepoint_count,
        &state.metrics,
    )
}

fn preflight(context: &mut PreflightContext<'_>) -> Result<(), String> {
    if !super::super::selected_or_environment("perfetto", context.selected) {
        return Ok(());
    }
    let executable = measurements::resolve_executable(context.tool_dir, "perfetto");
    if !executable.is_file() {
        return Ok(());
    }
    if trace_marker_path().is_none() {
        return Err(perfetto_tracefs_error());
    }
    if perfetto_probe(&executable, context.measurement_dir, false)? {
        return Ok(());
    }
    if perfetto_probe(&executable, context.measurement_dir, true)? {
        context.sudo_collectors.insert("perfetto".into());
        Ok(())
    } else {
        Err(measurements::passwordless_sudo_error(
            "perfetto",
            &executable,
        ))
    }
}

use crate::collectors;
use crate::measurements::{
    Collector, bounded, env_true, escape_svg, parse_float, safe_svg_name, write_unavailable,
};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Default)]
struct PerfettoState {
    enabled: bool,
    trace: Option<PathBuf>,
    pid: Option<u32>,
    trace_marker: Option<PathBuf>,
    trace_marker_output: Option<File>,
    tracepoint_count: u64,
    metrics: BTreeMap<String, f64>,
}

impl measurements::Session {
    pub(crate) fn set_perfetto_pid(&mut self, pid: u32) {
        self.state::<PerfettoState>().borrow_mut().pid = Some(pid);
    }

    pub(crate) fn start_perfetto(&mut self) -> Result<(), String> {
        if !self.state::<PerfettoState>().borrow().enabled {
            return Ok(());
        }
        let config = self.dir.join("perfetto-scheduling.pbtx");
        fs::write(&config, perfetto_config(None))
            .map_err(|error| format!("cannot write Perfetto trace configuration: {error}"))?;
        let trace = self.dir.join("perfetto-scheduling.perfetto-trace");
        // A sudo-launched Tracebox otherwise creates a root-only 0600 trace.
        // Precreating the artifact keeps its ownership with the benchmark user.
        File::create(&trace)
            .map_err(|error| format!("cannot create Perfetto trace artifact: {error}"))?;
        let stdout_path = self.dir.join("perfetto.stdout");
        let stderr_path = self.dir.join("perfetto.stderr");
        File::create(&stdout_path)
            .map_err(|error| format!("cannot create Perfetto output: {error}"))?;
        let marker = trace_marker_path().ok_or_else(perfetto_tracefs_error)?;
        let marker_output = OpenOptions::new().write(true).open(&marker).map_err(|error| {
            format!(
                "cannot write ASB lifecycle markers to {}: {error}; grant the benchmark user write access to trace_marker before running with Perfetto",
                marker.display()
            )
        })?;
        let child = self
            .command("perfetto")
            .args(["--txt", "--notify-fd", "1", "-c"])
            .arg(&config)
            .args(["-o"])
            .arg(&trace)
            .stdout(Stdio::piped())
            .stderr(Stdio::from(File::create(&stderr_path).map_err(
                |error| format!("cannot create Perfetto error output: {error}"),
            )?))
            .spawn();
        match child {
            Ok(mut child) => {
                let mut pipe = child
                    .stdout
                    .take()
                    .ok_or_else(|| "cannot receive Perfetto readiness notification".to_owned())?;
                let (tx, rx) = std::sync::mpsc::sync_channel(1);
                std::thread::spawn(move || {
                    let mut status = [1u8];
                    let result = pipe.read_exact(&mut status).map(|()| status[0]);
                    let _ = tx.send(result);
                });
                let ready = rx.recv_timeout(Duration::from_secs(35));
                if !matches!(ready, Ok(Ok(0))) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "Perfetto did not report that tracing is ready: {ready:?}"
                    ));
                }
                let state = self.state::<PerfettoState>();
                let mut state = state.borrow_mut();
                state.trace = Some(trace);
                state.trace_marker = Some(marker);
                state.trace_marker_output = Some(marker_output);
                self.collectors.push(Collector {
                    name: "perfetto".into(),
                    adapter: collectors::find("perfetto").expect("registered collector"),
                    child,
                    started: Instant::now(),
                    stdout_path,
                    stderr_path,
                });
            }
            Err(error) if !env_true("ASB_MEASURE_PERFETTO") => write_unavailable(
                &self.dir.join("perfetto.json"),
                "perfetto",
                format!("{error}; install it with `asb tool install perfetto` in this project"),
            )?,
            Err(error) => {
                return Err(format!(
                    "cannot start Perfetto Tracebox (disable with ASB_MEASURE_PERFETTO=0): {error}"
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn write_trace_marker(&mut self, event: &str) -> Result<(), String> {
        let state = self.state::<PerfettoState>();
        let mut state = state.borrow_mut();
        let path = state
            .trace_marker
            .as_ref()
            .map_or_else(|| "trace_marker".into(), |path| path.display().to_string());
        let Some(output) = state.trace_marker_output.as_mut() else {
            return Ok(());
        };
        output
            .write_all(event.as_bytes())
            .map_err(|error| format!("cannot write ASB lifecycle marker to {path}: {error}"))?;
        state.tracepoint_count = state.tracepoint_count.saturating_add(1);
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn set_test_marker(&mut self, path: &Path) {
        let state = self.state::<PerfettoState>();
        let mut state = state.borrow_mut();
        state.trace_marker = Some(path.to_path_buf());
        state.trace_marker_output = Some(File::create(path).expect("create test marker"));
    }

    /// Summarize the native trace with the installer-provided trace processor.
    /// The trace itself remains the authoritative artifact; these aggregates
    /// make scheduler activity comparable in ASB's offline SVG and HTML views.
    pub(crate) fn perfetto_trace_metrics(&mut self) -> BTreeMap<String, f64> {
        let mut metrics = BTreeMap::new();
        let state = self.state::<PerfettoState>();
        let state = state.borrow();
        let Some(trace) = state.trace.as_ref().filter(|path| path.is_file()) else {
            return metrics;
        };
        metrics.insert(
            "perfetto.trace_bytes".into(),
            trace.metadata().map_or(0, |metadata| metadata.len()) as f64,
        );
        metrics.insert("perfetto.tracepoints".into(), state.tracepoint_count as f64);
        let Some(tool_dir) = self.tool_dir.as_ref() else {
            return metrics;
        };
        let processor = tool_dir.join("perfetto/bin/trace_processor_shell");
        if !processor.is_file() {
            return metrics;
        }
        for (metric, query) in perfetto_metric_queries(state.pid, &self.trace_id) {
            let Ok(output) = Command::new(&processor)
                .args(["query"])
                .arg(trace)
                .arg(query)
                .output()
            else {
                continue;
            };
            if output.status.success()
                && let Some(value) = bounded(&output.stdout).lines().rev().find_map(parse_float)
            {
                metrics.insert(metric, value);
            }
        }
        metrics
    }

    /// Extract timestamped scheduler samples directly from the Perfetto trace.
    /// The timestamp column is retained in milliseconds so comparison graphs
    /// use trace time rather than synthetic point indexes.
    pub(crate) fn perfetto_trace_series(
        &mut self,
    ) -> (BTreeMap<String, Vec<f64>>, BTreeMap<String, Vec<f64>>) {
        let mut series = BTreeMap::new();
        let mut timestamps = BTreeMap::new();
        let state = self.state::<PerfettoState>();
        let state = state.borrow();
        let Some(trace) = state.trace.as_ref().filter(|path| path.is_file()) else {
            return (series, timestamps);
        };
        let Some(tool_dir) = self.tool_dir.as_ref() else {
            return (series, timestamps);
        };
        let processor = tool_dir.join("perfetto/bin/trace_processor_shell");
        if !processor.is_file() {
            return (series, timestamps);
        }
        for (name, query) in perfetto_series_queries(state.pid, &self.trace_id) {
            let Ok(output) = Command::new(&processor)
                .args(["query"])
                .arg(trace)
                .arg(query)
                .output()
            else {
                continue;
            };
            if !output.status.success() {
                continue;
            }
            let (sample_timestamps, values) =
                parse_perfetto_series_output(&bounded(&output.stdout));
            if values.len() >= 2 {
                timestamps.insert(name.clone(), sample_timestamps);
                series.insert(name, values);
            }
        }
        (series, timestamps)
    }
}

pub(crate) fn parse_perfetto_series_output(output: &str) -> (Vec<f64>, Vec<f64>) {
    let mut timestamps = Vec::new();
    let mut values = Vec::new();
    for line in output.lines() {
        let fields = line
            .split(',')
            .map(|field| field.trim().trim_matches('"'))
            .collect::<Vec<_>>();
        if fields.len() < 2 {
            continue;
        }
        let (Some(timestamp_s), Some(value)) = (parse_float(fields[0]), parse_float(fields[1]))
        else {
            continue;
        };
        timestamps.push(timestamp_s * 1_000.0);
        values.push(value);
        if values.len() >= 2_048 {
            break;
        }
    }
    (timestamps, values)
}

/// Write one dependency-free SVG per stable aggregate metric.
pub(crate) fn write_perfetto_metric_svgs(
    dir: &Path,
    trace: Option<&Path>,
    tracepoint_count: u64,
    metrics: &BTreeMap<String, f64>,
) -> Result<(), String> {
    let mut all_metrics = metrics.clone();
    if all_metrics.is_empty() {
        all_metrics.insert("perfetto.tracepoints".into(), tracepoint_count as f64);
    }
    for (name, value) in all_metrics {
        let trace_state = trace
            .filter(|path| path.is_file())
            .map_or("native trace unavailable", |_| "native trace recorded");
        let label = name.trim_start_matches("perfetto.");
        let svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"760\" height=\"180\" viewBox=\"0 0 760 180\"><title>Perfetto {}</title><rect width=\"760\" height=\"180\" fill=\"#101827\"/><rect x=\"16\" y=\"16\" width=\"728\" height=\"148\" rx=\"10\" fill=\"#182334\"/><text x=\"36\" y=\"50\" fill=\"#e5edf8\" font-family=\"sans-serif\" font-size=\"20\" font-weight=\"bold\">Perfetto: {}</text><text x=\"36\" y=\"92\" fill=\"#78dba9\" font-family=\"monospace\" font-size=\"28\">{:.0}</text><text x=\"36\" y=\"132\" fill=\"#b9c7dc\" font-family=\"sans-serif\" font-size=\"15\">{} · lifecycle markers: {tracepoint_count}</text></svg>\n",
            escape_svg(label),
            escape_svg(label),
            value,
            trace_state,
        );
        fs::write(
            dir.join(format!("perfetto-metric-{}.svg", safe_svg_name(label))),
            svg,
        )
        .map_err(|error| format!("cannot write Perfetto metric SVG: {error}"))?;
    }
    Ok(())
}

pub(crate) fn trace_marker_path() -> Option<PathBuf> {
    [
        PathBuf::from("/sys/kernel/tracing/trace_marker"),
        PathBuf::from("/sys/kernel/debug/tracing/trace_marker"),
    ]
    .into_iter()
    .find(|path| path.is_file())
}

/// Build Perfetto queries that restrict scheduler data to the benchmark
/// process and every descendant it launched.  Tracebox cannot apply a pid
/// filter on all supported versions, so the scope is enforced by the offline
/// trace processor instead of silently reporting host-wide activity.
pub(crate) fn perfetto_metric_queries(pid: Option<u32>, trace_id: &str) -> Vec<(String, String)> {
    let Some(pid) = pid else {
        return Vec::new();
    };
    let scope = perfetto_scope_sql(pid, trace_id);
    vec![
        (
            "perfetto.scheduler_events".into(),
            format!(
                "{scope} SELECT COUNT(*) FROM sched s JOIN thread th ON s.utid=th.utid CROSS JOIN bounds WHERE th.upid IN tree AND s.ts >= bounds.start_ts AND s.ts < bounds.end_ts"
            ),
        ),
        (
            "perfetto.processes".into(),
            format!("{scope} SELECT COUNT(*) FROM process WHERE upid IN tree"),
        ),
        (
            "perfetto.threads".into(),
            format!("{scope} SELECT COUNT(*) FROM thread WHERE upid IN tree"),
        ),
    ]
}

pub(crate) fn perfetto_series_queries(pid: Option<u32>, trace_id: &str) -> Vec<(String, String)> {
    let Some(pid) = pid else {
        return Vec::new();
    };
    let scope = perfetto_scope_sql(pid, trace_id);
    vec![
        (
            "perfetto.scheduler_events_per_s".into(),
            format!(
                "{scope} SELECT CAST((s.ts-bounds.start_ts)/1000000000 AS INTEGER) AS timestamp_s, COUNT(*) AS value FROM sched s JOIN thread th ON s.utid=th.utid CROSS JOIN bounds WHERE th.upid IN tree AND s.ts >= bounds.start_ts AND s.ts < bounds.end_ts GROUP BY timestamp_s ORDER BY timestamp_s"
            ),
        ),
        (
            "perfetto.scheduler_duration_ms_per_s".into(),
            format!(
                "{scope}, relevant AS (SELECT s.ts, s.dur FROM sched s JOIN thread th ON s.utid=th.utid CROSS JOIN bounds WHERE th.upid IN tree AND s.ts < bounds.end_ts AND s.ts+s.dur > bounds.start_ts), bucket_ranges(bucket) AS (SELECT 0 UNION ALL SELECT bucket+1 FROM bucket_ranges CROSS JOIN bounds WHERE bounds.start_ts+(bucket+1)*1000000000 < bounds.end_ts) SELECT bucket_ranges.bucket AS timestamp_s, COALESCE(SUM(MAX(0, MIN(r.ts+r.dur,bounds.start_ts+(bucket_ranges.bucket+1)*1000000000)-MAX(r.ts,bounds.start_ts+bucket_ranges.bucket*1000000000)))/1000000.0,0) AS value FROM bucket_ranges CROSS JOIN bounds LEFT JOIN relevant r ON r.ts < bounds.start_ts+(bucket_ranges.bucket+1)*1000000000 AND r.ts+r.dur > bounds.start_ts+bucket_ranges.bucket*1000000000 GROUP BY bucket_ranges.bucket ORDER BY bucket_ranges.bucket"
            ),
        ),
    ]
}

pub(crate) fn perfetto_scope_sql(pid: u32, trace_id: &str) -> String {
    // Prefer explicit ASB lifecycle markers when tracefs captured them, and
    // fall back to the process tree's lifetime when markers are unavailable.
    format!(
        "WITH RECURSIVE tree(upid) AS (SELECT upid FROM process WHERE pid={pid} UNION ALL SELECT p.upid FROM process p JOIN tree t ON p.parent_upid=t.upid), process_bounds AS (SELECT MIN(start_ts) AS process_start, MAX(COALESCE(end_ts,(SELECT MAX(ts+dur) FROM sched))) AS process_end FROM process WHERE upid IN tree), marker_bounds AS (SELECT MIN(ts) FILTER (WHERE name='asb.agent.started.{trace_id}') AS marker_start, MAX(ts) FILTER (WHERE name='asb.agent.exited.{trace_id}') AS marker_end FROM slice), bounds AS (SELECT COALESCE(marker_start,process_start) AS start_ts, COALESCE(marker_end,process_end) AS end_ts FROM process_bounds CROSS JOIN marker_bounds)",
    )
}

pub(crate) fn perfetto_config(pid: Option<u32>) -> String {
    // Tracebox's bundled schema rejects FtraceConfig.pid_filter. Process
    // scoping is applied during post-processing using ASB lifecycle markers.
    let _ = pid;
    r#"duration_ms: 3600000
buffers { size_kb: 65536 fill_policy: RING_BUFFER }
data_sources {
  config {
    name: "linux.ftrace"
    ftrace_config {
      compact_sched { enabled: true }
      ftrace_events: "sched/sched_switch"
      ftrace_events: "sched/sched_waking"
      ftrace_events: "sched/sched_wakeup_new"
      ftrace_events: "sched/sched_process_exit"
      ftrace_events: "sched/sched_process_free"
      ftrace_events: "task/task_newtask"
      ftrace_events: "task/task_rename"
      ftrace_events: "ftrace/print"
    }
  }
}
data_sources {
  config {
    name: "linux.process_stats"
    process_stats_config { scan_all_processes_on_start: false }
  }
}
"#
    .to_owned()
}

fn perfetto_probe_config() -> String {
    perfetto_config(None).replacen("duration_ms: 3600000", "duration_ms: 250", 1)
}

/// Probe the exact ftrace configuration used by ASB. `--background-wait`
/// waits for Perfetto to acknowledge every data source, so a successful
/// process launch cannot mask an unavailable tracefs instance.
pub(crate) fn perfetto_probe(
    executable: &Path,
    measurement_dir: &Path,
    sudo: bool,
) -> Result<bool, String> {
    let config = measurement_dir.join(".perfetto-preflight.pbtx");
    let trace = measurement_dir.join(".perfetto-preflight.perfetto-trace");
    fs::write(&config, perfetto_probe_config())
        .map_err(|error| format!("cannot write Perfetto preflight configuration: {error}"))?;
    let mut command = if sudo {
        let mut command = Command::new("sudo");
        command.args(["-n", "--"]);
        command.arg(executable);
        command
    } else {
        Command::new(executable)
    };
    let result = command
        .args(["--txt", "-c"])
        .arg(&config)
        .args(["-o"])
        .arg(&trace)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output();
    let _ = fs::remove_file(&config);
    let _ = fs::remove_file(&trace);
    match result {
        Ok(output) => {
            if !output.status.success() {
                return Ok(false);
            }
            let stderr = String::from_utf8_lossy(&output.stderr);
            let unavailable_markers = [
                "Failed to reset ftrace",
                "Failed to create FtraceController",
                "Data source id=1 not found",
            ];
            Ok(!unavailable_markers
                .iter()
                .any(|marker| stderr.contains(marker)))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && sudo => Ok(false),
        Err(error) => Err(format!("cannot probe Perfetto ftrace collector: {error}")),
    }
}

pub(crate) fn perfetto_tracefs_error() -> String {
    "Perfetto needs the Linux tracefs ftrace interface, but ASB could not find trace_marker under /sys/kernel/tracing or /sys/kernel/debug/tracing. This is a host kernel interface, not an installable project tool. What to do: mount tracefs or enable kernel tracing, then rerun. If mounting tracefs requires privileges, grant the required passwordless sudo access before starting the benchmark; ASB never prompts for a password.".into()
}
