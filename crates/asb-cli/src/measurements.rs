//! Optional, file-backed measurements for a benchmark run.
//!
//! Measurement collection is deliberately opt-in.  The default run path does
//! not inspect the host or start any auxiliary process.  When enabled, every
//! result is written as bounded JSON so later processing never has to parse
//! terminal output.

use crate::collectors;
use serde::Serialize;
use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MAX_CAPTURE: usize = 256 * 1024;
const MAX_RUNS: usize = 10_000;
static NEXT_TRACE_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize)]
pub struct TokenUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cached_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub source: Option<String>,
}

/// Stable names used in generated configuration files and downstream tools.
pub fn available_collectors() -> Vec<String> {
    collectors::all()
        .iter()
        .map(|collector| collector.id().to_owned())
        .collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct RunMeasurement<'a> {
    pub schema_version: u32,
    pub run_id: String,
    pub status: &'a str,
    pub success: bool,
    pub process_success: bool,
    pub workload_success: Option<bool>,
    pub success_rate: f64,
    pub agent: &'a str,
    pub workload: &'a str,
    pub workload_evaluation: &'a str,
    pub provider: &'a str,
    pub model: &'a str,
    pub elapsed_ms: u128,
    pub exit_code: Option<i32>,
    pub token_usage: TokenUsage,
    pub steps: u64,
    pub started_unix_ms: u128,
}

#[derive(Debug, Serialize)]
struct CollectorResult {
    schema_version: u32,
    collector: String,
    status: String,
    exit_code: Option<i32>,
    elapsed_ms: u128,
    stdout: String,
    stderr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    artifact: Option<String>,
    /// Parsed, stable metrics. Raw stdout/stderr remain available for detail,
    /// but comparison must consume this bounded machine-readable map instead.
    metrics: BTreeMap<String, f64>,
    /// Bounded samples emitted by collectors that support interval output.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    series: BTreeMap<String, Vec<f64>>,
    /// Timestamp in milliseconds for each Perfetto series sample.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    series_timestamps: BTreeMap<String, Vec<f64>>,
}

#[derive(Serialize)]
struct LifecycleRecord {
    sequence: usize,
    elapsed_us: u128,
    event: collectors::LifecycleEvent,
}

pub(crate) struct Collector {
    pub(crate) name: String,
    pub(crate) adapter: &'static dyn collectors::CollectorAdapter,
    pub(crate) child: Child,
    pub(crate) started: Instant,
    pub(crate) stdout_path: PathBuf,
    pub(crate) stderr_path: PathBuf,
}

impl Drop for Collector {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = stop_collector(&mut self.child, self.adapter.graceful_shutdown(&self.name));
        }
    }
}

/// A measurement session owns all optional collector processes for one run.
pub struct Session {
    pub(crate) dir: PathBuf,
    pub(crate) tool_dir: Option<PathBuf>,
    started_unix_ms: u128,
    pub(crate) collectors: Vec<Collector>,
    states: BTreeMap<TypeId, Box<dyn Any>>,
    shared_artifacts: BTreeMap<collectors::SharedArtifact, PathBuf>,
    run_started: bool,
    run_failed: bool,
    run_finishing: bool,
    lifecycle_started: Instant,
    lifecycle: Vec<LifecycleRecord>,
    #[cfg(test)]
    pub(crate) test_attach_failure: bool,
    pub(crate) trace_id: String,
    sudo_collectors: BTreeSet<String>,
}

impl Drop for Session {
    fn drop(&mut self) {
        // An early runner error (for example, an incomplete replay cassette)
        // must not orphan process-scoped collectors. `finish` takes ownership
        // of the collector list, so this only handles paths that return before
        // normal finalization.
        if self.run_started && !self.run_finishing && !self.run_failed {
            let _ = self.event(collectors::LifecycleEvent::RunFailed);
        }
        for mut collector in self.collectors.drain(..) {
            let _ = stop_collector(
                &mut collector.child,
                collector.adapter.graceful_shutdown(&collector.name),
            );
        }
        if self.run_started {
            let _ = write_json(&self.dir.join("lifecycle.json"), &self.lifecycle);
        }
    }
}

impl Session {
    pub fn started_unix_ms(&self) -> u128 {
        self.started_unix_ms
    }
    /// Start a session and write the host description.  Collectors are enabled
    /// only by explicit ASB_MEASURE_* variables, keeping CI deterministic.
    /// Start with the stable collector names from a generated configuration.
    /// An empty list means environment-only opt-in, useful for tests and
    /// hand-written configurations.
    pub fn start_with_collectors(
        dir: &Path,
        selected: &[String],
        tool_dir: Option<&Path>,
    ) -> Result<Self, String> {
        fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create measurements directory: {e}"))?;
        // Privilege and tracefs checks must complete before any agent process
        // is prepared. This prevents a partially measured benchmark run.
        let sudo_collectors = preflight_collectors(dir, selected, tool_dir)?;
        let started_unix_ms = unix_ms();
        let mut session = Self {
            dir: dir.to_path_buf(),
            tool_dir: tool_dir.map(Path::to_path_buf),
            started_unix_ms,
            collectors: Vec::new(),
            states: BTreeMap::new(),
            shared_artifacts: BTreeMap::new(),
            run_started: false,
            run_failed: false,
            run_finishing: false,
            lifecycle_started: Instant::now(),
            lifecycle: Vec::new(),
            #[cfg(test)]
            test_attach_failure: false,
            trace_id: format!(
                "{}-{}",
                std::process::id(),
                NEXT_TRACE_ID.fetch_add(1, Ordering::Relaxed)
            ),
            sudo_collectors,
        };
        for collector in collectors::all() {
            collector.configure(&mut session, selected)?;
        }
        for collector in collectors::all() {
            collector.start(&mut session)?;
        }
        Ok(session)
    }

    /// Deliver a non-secret typed lifecycle event to registered collectors.
    pub(crate) fn event(&mut self, event: collectors::LifecycleEvent) -> Result<(), String> {
        if self.lifecycle.len() >= 256 {
            return Err("too many benchmark lifecycle events".into());
        }
        for collector in collectors::all() {
            collector.lifecycle_event(self, event)?;
        }
        self.lifecycle.push(LifecycleRecord {
            sequence: self.lifecycle.len(),
            elapsed_us: self.lifecycle_started.elapsed().as_micros(),
            event,
        });
        match event {
            collectors::LifecycleEvent::RunStarted => self.run_started = true,
            collectors::LifecycleEvent::RunFailed => self.run_failed = true,
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn trace_id(&self) -> &str {
        &self.trace_id
    }

    pub(crate) fn state<T: Default + 'static>(&mut self) -> Rc<RefCell<T>> {
        self.states
            .entry(TypeId::of::<T>())
            .or_insert_with(|| Box::new(Rc::new(RefCell::new(T::default()))))
            .downcast_ref::<Rc<RefCell<T>>>()
            .expect("state is keyed by its concrete type")
            .clone()
    }

    pub(crate) fn set_artifact(&mut self, kind: collectors::SharedArtifact, path: PathBuf) {
        self.shared_artifacts.insert(kind, path);
    }

    pub(crate) fn artifact(&self, kind: collectors::SharedArtifact) -> Option<&Path> {
        self.shared_artifacts.get(&kind).map(PathBuf::as_path)
    }

    /// Attach perf to the selected child after it has been spawned.
    pub fn attach_pid(&mut self, pid: u32) -> Result<(), String> {
        #[cfg(test)]
        if self.test_attach_failure {
            return Err("injected collector attachment failure".into());
        }
        for collector in collectors::all() {
            if collector.requires_pid() {
                collector.attach_pid(self, pid)?;
            }
        }
        Ok(())
    }

    /// Finish collectors and write the run record and aggregate summary.
    pub fn finish<'a>(mut self, measurement: RunMeasurement<'a>) -> Result<(), String> {
        let collectors = std::mem::take(&mut self.collectors);
        for mut collector in collectors {
            let status = match collector.child.try_wait() {
                Ok(Some(status)) => status,
                Ok(None) => stop_collector(
                    &mut collector.child,
                    collector.adapter.graceful_shutdown(&collector.name),
                )
                .map_err(|error| format!("cannot finish {} collector: {error}", collector.name))?,
                Err(error) => {
                    return Err(format!(
                        "cannot inspect {} collector: {error}",
                        collector.name
                    ));
                }
            };
            let stdout = read_bounded(&collector.stdout_path);
            let stderr = read_bounded(&collector.stderr_path);
            let _ = fs::remove_file(&collector.stdout_path);
            let _ = fs::remove_file(&collector.stderr_path);
            let context = collectors::ProcessContext {
                name: &collector.name,
                status: &status,
                stdout: &stdout,
                stderr: &stderr,
            };
            let data = collector.adapter.process(&mut self, &context)?;
            let result = CollectorResult {
                schema_version: 1,
                collector: collector.name.clone(),
                status: data.status,
                exit_code: status.code(),
                elapsed_ms: collector.started.elapsed().as_millis(),
                stdout,
                stderr,
                artifact: data.artifact,
                metrics: data.metrics,
                series: data.series,
                series_timestamps: data.timestamps,
            };
            write_json(&self.dir.join(format!("{}.json", collector.name)), &result)?;
        }
        write_json(&self.dir.join("run.json"), &measurement)?;
        if self.run_started {
            write_json(&self.dir.join("lifecycle.json"), &self.lifecycle)?;
        }
        for adapter in collectors::all() {
            adapter.finish(&mut self)?;
        }
        update_summary(&self.dir, &measurement)?;
        self.run_finishing = true;
        Ok(())
    }

    pub(crate) fn start_snapshot(
        &mut self,
        name: &str,
        variable: &str,
        args: &[&str],
        enabled: bool,
        command_name: &str,
    ) -> Result<(), String> {
        if !enabled {
            return Ok(());
        }
        let stdout_path = self.dir.join(format!("{name}.stdout"));
        let stderr_path = self.dir.join(format!("{name}.stderr"));
        let child = match self
            .command(command_name)
            .args(args)
            .stdout(Stdio::from(
                File::create(&stdout_path)
                    .map_err(|e| format!("cannot create {name} output: {e}"))?,
            ))
            .stderr(Stdio::from(File::create(&stderr_path).map_err(|e| {
                format!("cannot create {name} error output: {e}")
            })?))
            .spawn()
        {
            Ok(child) => child,
            Err(error) if !env_true(variable) => {
                write_unavailable(
                    &self.dir.join(format!("{name}.json")),
                    name,
                    error.to_string(),
                )?;
                return Ok(());
            }
            Err(error) => {
                return Err(format!(
                    "cannot start {name}; install it or disable {variable}=1: {error}"
                ));
            }
        };
        self.collectors.push(Collector {
            name: name.into(),
            adapter: collectors::find(name).expect("registered collector"),
            child,
            started: Instant::now(),
            stdout_path,
            stderr_path,
        });
        Ok(())
    }

    pub(crate) fn command(&self, name: &str) -> Command {
        let executable = self.executable(name);
        if self.sudo_collectors.contains(name) {
            let mut command = Command::new("sudo");
            command.args(["-n", "--"]).arg(executable);
            return command;
        }
        Command::new(executable)
    }

    fn executable(&self, name: &str) -> PathBuf {
        if let Some(tool_dir) = &self.tool_dir {
            let binary = tool_binary_name(name);
            for local in [
                tool_dir.join(name).join("bin").join(binary),
                tool_dir.join("bin").join(binary),
            ] {
                if local.is_file() {
                    return local;
                }
            }
        }
        PathBuf::from(tool_binary_name(name))
    }
}

pub fn token_usage_from_sources(
    provider: &str,
    agent: &str,
    stdout: &str,
    stderr: &str,
    cassette: Option<&Path>,
) -> TokenUsage {
    let context = collectors::UsageContext {
        provider,
        agent,
        stdout,
        stderr,
        cassette,
    };
    collectors::all()
        .iter()
        .find_map(|adapter| adapter.usage(&context))
        .unwrap_or_default()
}
/// Read and normalize usage events from an agent artifact.
pub fn token_usage_from_path(path: &Path) -> TokenUsage {
    collectors::all()
        .iter()
        .find_map(|adapter| adapter.usage_file(path))
        .unwrap_or_default()
}
/// Count bounded agent actions and turns.
pub fn steps_from_sources(agent: &str, stdout: &str, stderr: &str) -> u64 {
    collectors::all()
        .iter()
        .find_map(|adapter| adapter.steps(agent, stdout, stderr))
        .unwrap_or_default()
}

fn update_summary(dir: &Path, run: &RunMeasurement<'_>) -> Result<(), String> {
    let path = dir.join("summary.json");
    let mut runs: Vec<serde_json::Value> = if path.exists() {
        let mut text = String::new();
        File::open(&path)
            .map_err(|e| format!("cannot open summary: {e}"))?
            .read_to_string(&mut text)
            .map_err(|e| format!("cannot read summary: {e}"))?;
        serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| v.get("runs").cloned())
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    runs.push(serde_json::to_value(run).map_err(|e| e.to_string())?);
    if runs.len() > MAX_RUNS {
        runs.drain(..runs.len() - MAX_RUNS);
    }
    let success_count = runs
        .iter()
        .filter(|v| v.get("success").and_then(serde_json::Value::as_bool) == Some(true))
        .count();
    let summary = serde_json::json!({"schema_version": 1, "runs": runs, "success_count": success_count, "failure_count": runs.len() - success_count, "success_rate": success_count as f64 / runs.len().max(1) as f64});
    write_json(&path, &summary)
}

pub(crate) fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let file = File::create(path).map_err(|e| format!("cannot create {}: {e}", path.display()))?;
    serde_json::to_writer_pretty(file, value)
        .map_err(|e| format!("cannot write {}: {e}", path.display()))
}
pub(crate) fn write_unavailable(path: &Path, collector: &str, error: String) -> Result<(), String> {
    write_json(
        path,
        &CollectorResult {
            schema_version: 1,
            collector: collector.into(),
            status: "unavailable".into(),
            exit_code: None,
            elapsed_ms: 0,
            stdout: String::new(),
            stderr: error,
            artifact: None,
            metrics: BTreeMap::new(),
            series: BTreeMap::new(),
            series_timestamps: BTreeMap::new(),
        },
    )
}

pub(crate) fn escape_svg(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub(crate) fn safe_svg_name(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect()
}

pub(crate) fn parse_pidstat_series(
    output: &str,
    wanted: &[&str],
    name: impl Fn(&str) -> String,
) -> BTreeMap<String, Vec<f64>> {
    let mut header = None;
    let mut samples: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for line in output.lines() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.first().is_some_and(|field| *field == "Average:")
            || fields.first().is_some_and(|field| *field == "Linux")
        {
            continue;
        }
        if fields.iter().any(|field| wanted.contains(field)) {
            header = Some(fields);
            continue;
        }
        let Some(header_fields) = header.as_ref() else {
            continue;
        };
        if fields.len() != header_fields.len() {
            continue;
        }
        for wanted_name in wanted {
            let Some(index) = header_fields.iter().position(|field| field == wanted_name) else {
                continue;
            };
            let Some(value) = fields.get(index).and_then(|field| parse_float(field)) else {
                continue;
            };
            samples.entry(name(wanted_name)).or_default().push(value);
        }
    }
    samples.retain(|_, values| !values.is_empty());
    samples
}

fn tool_binary_name(name: &str) -> &str {
    crate::tools::find(name)
        .and_then(|tool| tool.executable())
        .unwrap_or(name)
}

pub(crate) fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("artifact")
        .to_owned()
}

pub(crate) fn parse_pidstat_columns(
    output: &str,
    wanted: &[&str],
    name: impl Fn(&str) -> String,
) -> BTreeMap<String, f64> {
    let mut header = None;
    let mut totals: BTreeMap<String, (f64, usize)> = BTreeMap::new();
    for line in output.lines() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.first().is_some_and(|field| *field == "Average:") {
            continue;
        }
        if fields.iter().any(|field| wanted.contains(field)) {
            header = Some(fields);
            continue;
        }
        let Some(header_fields) = header.as_ref() else {
            continue;
        };
        if fields.len() != header_fields.len()
            || fields.first().is_some_and(|field| *field == "Linux")
        {
            continue;
        }
        for wanted_name in wanted {
            let Some(index) = header_fields.iter().position(|field| field == wanted_name) else {
                continue;
            };
            let Some(value) = fields.get(index).and_then(|field| parse_float(field)) else {
                continue;
            };
            let entry = totals.entry((*wanted_name).to_owned()).or_default();
            entry.0 += value;
            entry.1 += 1;
        }
    }
    totals
        .into_iter()
        .filter(|(_, (_, count))| *count > 0)
        .map(|(metric, (sum, count))| (name(&metric), sum / count as f64))
        .collect()
}

pub(crate) fn parse_float(value: &str) -> Option<f64> {
    value
        .trim_matches(|character: char| {
            !character.is_ascii_digit() && character != '.' && character != '-'
        })
        .parse()
        .ok()
}

pub(crate) fn measurement_interval_ms() -> u64 {
    std::env::var("ASB_MEASURE_INTERVAL_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(1_000)
        .clamp(100, 60_000)
}

/// Stop a collector without allowing a wrapper such as `sudo` to strand its
/// real tracing child.  The previous unbounded `wait()` could deadlock an
/// entire sweep after the agent had already exited.
fn stop_collector(child: &mut Child, graceful: bool) -> std::io::Result<std::process::ExitStatus> {
    let pid = child.id();
    if graceful {
        let _ = Command::new("kill")
            .args(["-INT", &pid.to_string()])
            .status();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = child.try_wait()? {
                return Ok(status);
            }
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    let _ = child.kill();
    // `sudo` may have spawned the actual collector as a direct child. Reap
    // that child too; otherwise the pipe-backed collector can outlive ASB.
    let _ = Command::new("pkill")
        .args(["-KILL", "-P", &pid.to_string()])
        .status();
    child.wait()
}

pub(crate) fn pidstat_interval_seconds() -> String {
    // pidstat accepts whole-second intervals. Preserve the requested lower
    // bound while rounding sub-second requests up to the nearest supported
    // interval; perf stat below supports the full millisecond range.
    measurement_interval_ms().div_ceil(1_000).to_string()
}

pub(crate) fn bounded(bytes: &[u8]) -> String {
    String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_CAPTURE)]).into_owned()
}
fn read_bounded(path: &Path) -> String {
    let Ok(mut file) = File::open(path) else {
        return String::new();
    };
    let mut bytes = Vec::new();
    let mut limited = (&mut file).take(MAX_CAPTURE as u64);
    if limited.read_to_end(&mut bytes).is_err() {
        return String::new();
    }
    bounded(&bytes)
}
pub(crate) fn env_true(name: &str) -> bool {
    matches!(
        std::env::var(name).as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    )
}

/// Probe privileged collectors before the benchmark starts. A direct probe is
/// preferred; only collectors that actually fail the probe are retried with
/// non-interactive sudo. Perf is intentionally not granted a sudoers rule:
/// its kernel policy has a narrower, user-applied repair path below.
fn preflight_collectors(
    measurement_dir: &Path,
    selected: &[String],
    tool_dir: Option<&Path>,
) -> Result<BTreeSet<String>, String> {
    let mut sudo_collectors = BTreeSet::new();
    let mut context = collectors::PreflightContext {
        measurement_dir,
        selected,
        tool_dir,
        sudo_collectors: &mut sudo_collectors,
    };
    for collector in collectors::all() {
        collector.preflight(&mut context)?;
    }
    Ok(sudo_collectors)
}

pub(crate) fn passwordless_sudo_available() -> bool {
    Command::new("sudo")
        .args(["-n", "true"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

pub(crate) fn resolve_executable(tool_dir: Option<&Path>, name: &str) -> PathBuf {
    if let Some(tool_dir) = tool_dir {
        let binary = tool_binary_name(name);
        for local in [
            tool_dir.join(name).join("bin").join(binary),
            tool_dir.join("bin").join(binary),
        ] {
            if local.is_file() {
                return local;
            }
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            let candidate = directory.join(tool_binary_name(name));
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    PathBuf::from(tool_binary_name(name))
}

pub(crate) fn passwordless_sudo_error(name: &str, executable: &Path) -> String {
    let command = sudoers_repair_command(name, executable);
    format!(
        "{name} collector needs elevated access, but its direct probe failed and passwordless sudo is unavailable.\nWhat to do: grant only this executable passwordless sudo, then rerun:\n  {command}\nThe generated rule should authorize `{}` for the current user; ASB never prompts for a sudo password.",
        executable.display()
    )
}

fn sudoers_repair_command(name: &str, executable: &Path) -> String {
    let path = shell_quote(&executable.to_string_lossy());
    let file = format!("/etc/sudoers.d/asb-{name}");
    format!(
        "sudo sh -c 'printf \"%s ALL=(root) NOPASSWD: {path}\\n\" \"$USER\" > {file} && chmod 440 {file} && visudo -cf {file}'"
    )
}

fn shell_quote(value: &str) -> String {
    value.replace('\'', "'\\''")
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis())
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::collectors::adapters::bpftrace::{
        bpftrace_profile_names, parse_bpftrace_metrics, parse_bpftrace_series,
        selected_bpftrace_profiles, write_bpftrace_profile_svg,
    };
    use crate::collectors::adapters::iostat::{parse_iostat_metrics, parse_pidstat_io_metrics};
    use crate::collectors::adapters::mpstat::{parse_mpstat_metrics, parse_pidstat_cpu_metrics};
    use crate::collectors::adapters::perf::{
        parse_perf_stat_metrics, parse_perf_stat_series, perf_permission_error,
    };
    use crate::collectors::adapters::perfetto::{
        parse_perfetto_series_output, perfetto_config, perfetto_metric_queries, perfetto_scope_sql,
        perfetto_series_queries, write_perfetto_metric_svgs,
    };
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn perfetto_metrics_produce_distinct_escaped_svg_artifacts() {
        let dir = std::env::temp_dir().join(format!("asb-metric-svg-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let metrics = BTreeMap::from([
            ("perfetto.sched_switch".to_owned(), 3.0),
            ("perfetto.wait<unsafe>".to_owned(), 12.0),
        ]);
        write_perfetto_metric_svgs(&dir, None, 2, &metrics).unwrap();
        let first = fs::read_to_string(dir.join("perfetto-metric-sched_switch.svg")).unwrap();
        let second = fs::read_to_string(dir.join("perfetto-metric-wait-unsafe-.svg")).unwrap();
        assert!(first.contains("sched_switch"));
        assert!(second.contains("wait&lt;unsafe&gt;"));
        assert!(!second.contains("wait<unsafe>"));
        assert!(second.contains("native trace unavailable"));
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn perfetto_without_metrics_reports_tracepoints_as_a_fallback() {
        let dir = std::env::temp_dir().join(format!("asb-metric-fallback-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        write_perfetto_metric_svgs(&dir, None, 9, &BTreeMap::new()).unwrap();
        let svg = fs::read_to_string(dir.join("perfetto-metric-tracepoints.svg")).unwrap();
        assert!(svg.contains("lifecycle markers: 9"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn bpftrace_svg_renders_each_metric_and_escapes_status() {
        let dir = std::env::temp_dir().join(format!("asb-bpf-svg-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let metrics = BTreeMap::from([
            ("sched.total".to_owned(), 7.0),
            ("sched.max".to_owned(), 3.0),
        ]);
        write_bpftrace_profile_svg(&dir, "sched_latency", "failure <mock>", &metrics).unwrap();
        let svg = fs::read_to_string(dir.join("bpftrace-sched_latency.svg")).unwrap();
        assert!(svg.contains("failure &lt;mock&gt;"));
        assert!(svg.contains("total: 7"));
        assert!(svg.contains("max: 3"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn collector_svg_write_errors_report_the_artifact_type() {
        let absent =
            std::env::temp_dir().join(format!("asb-no-such-metric-dir-{}", std::process::id()));
        assert!(
            write_perfetto_metric_svgs(&absent, None, 0, &BTreeMap::new())
                .unwrap_err()
                .contains("Perfetto metric SVG")
        );
        assert!(
            write_bpftrace_profile_svg(&absent, "sched", "error", &BTreeMap::new())
                .unwrap_err()
                .contains("bpftrace profile SVG")
        );
    }

    #[test]
    fn writes_machine_readable_run_and_summary() {
        let dir = std::env::temp_dir().join(format!("asb-measure-{}", unix_ms()));
        let session = Session::start_with_collectors(&dir, &[], None).expect("session");
        let run = RunMeasurement {
            schema_version: 1,
            run_id: "test".into(),
            status: "completed",
            success: true,
            process_success: true,
            workload_success: Some(true),
            success_rate: 1.0,
            agent: "a",
            workload: "w",
            workload_evaluation: "bundled-fixture",
            provider: "p",
            model: "m",
            elapsed_ms: 2,
            exit_code: Some(0),
            token_usage: TokenUsage::default(),
            steps: 1,
            started_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis(),
        };
        session.finish(run).expect("finish");
        let parsed: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join("summary.json")).unwrap()).unwrap();
        assert_eq!(parsed["success_rate"], 1.0);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn parses_openrouter_usage_details_from_agent_json() {
        let usage = token_usage_from_sources(
            "openrouter",
            "opencode",
            r#"{"usage":{"prompt_tokens":120,"completion_tokens":30,"total_tokens":150,"prompt_tokens_details":{"cached_tokens":40}}}"#,
            "",
            None,
        );
        assert_eq!(usage.input_tokens, Some(120));
        assert_eq!(usage.output_tokens, Some(30));
        assert_eq!(usage.cached_tokens, Some(40));
        assert_eq!(usage.total_tokens, Some(150));
        assert_eq!(usage.source.as_deref(), Some("agent-output"));
    }

    #[test]
    fn parses_cli2key_text_aliases_and_derives_total() {
        let usage = token_usage_from_sources(
            "cli2key",
            "codex",
            "input_tokens=8 output_tokens=5 cached_tokens=2",
            "",
            None,
        );
        assert_eq!(usage.input_tokens, Some(8));
        assert_eq!(usage.output_tokens, Some(5));
        assert_eq!(usage.cached_tokens, Some(2));
        assert_eq!(usage.total_tokens, Some(13));
        assert_eq!(usage.source.as_deref(), Some("agent-output"));
    }

    #[test]
    fn parses_human_agent_token_summary() {
        let usage = token_usage_from_sources("cli2key", "codex", "tokens used\n42,567\n", "", None);
        assert_eq!(usage.total_tokens, Some(42_567));
        assert_eq!(usage.source.as_deref(), Some("agent-output"));
    }

    #[test]
    fn parses_usage_events_emitted_by_each_supported_agent_family() {
        let cases = [
            (
                "codex",
                r#"response.completed {"usage":{"input_tokens":7,"output_tokens":3,"total_tokens":10}}"#,
                7,
                3,
                10,
            ),
            (
                "pi",
                r#"event: {"type":"turn_end","usage":{"input":7,"output":3,"total":10,"cacheRead":2}}"#,
                7,
                3,
                10,
            ),
            (
                "qwen-code",
                r#"{"type":"result","stats":{"total_tokens":10,"input_tokens":7,"output_tokens":3,"cached":2}}"#,
                7,
                3,
                10,
            ),
            (
                "workbuddy",
                r#"done {"usage":{"inputTokens":7,"outputTokens":3,"totalTokens":10}}"#,
                7,
                3,
                10,
            ),
            (
                "openjiuwen",
                r#"{"benchmark_usage":{"task_summary":{"prompt_tokens":7,"completion_tokens":3,"total_tokens":10}}}"#,
                7,
                3,
                10,
            ),
            (
                "openhands",
                r#"llm response {"usage":{"input_tokens":7,"output_tokens":3,"total_tokens":10}}"#,
                7,
                3,
                10,
            ),
            (
                "mini-swe",
                r#"result {"usage":{"input_tokens":7,"output_tokens":3,"total_tokens":10}}"#,
                7,
                3,
                10,
            ),
            (
                "claude-code",
                r#"{"type":"result","usage":{"input_tokens":17,"output_tokens":4,"cache_read_input_tokens":2,"total_tokens":21}}"#,
                17,
                4,
                21,
            ),
            (
                "goose",
                r#"{"message":{"usage":{"prompt_tokens":19,"completion_tokens":6,"total_tokens":25}}}"#,
                19,
                6,
                25,
            ),
            (
                "cline",
                r#"event {"usage":{"inputTokens":23,"outputTokens":8,"cachedTokens":3,"totalTokens":31}}"#,
                23,
                8,
                31,
            ),
            (
                "continue",
                r#"{"usage":{"input":31,"output":10,"cached":4,"total":41}}"#,
                31,
                10,
                41,
            ),
        ];
        for (agent, output, input, response, total) in cases {
            let usage = token_usage_from_sources("cli2key", agent, output, "", None);
            assert_eq!(usage.input_tokens, Some(input), "{agent} input");
            assert_eq!(usage.output_tokens, Some(response), "{agent} output");
            assert_eq!(usage.total_tokens, Some(total), "{agent} total");
        }
    }

    #[test]
    fn derives_mini_swe_total_from_complete_input_and_output() {
        let usage = token_usage_from_sources(
            "cli2key",
            "mini-swe",
            r#"{"usage":{"input_tokens":182398,"output_tokens":4189,"total_tokens":116}}"#,
            "",
            None,
        );
        assert_eq!(usage.input_tokens, Some(182398));
        assert_eq!(usage.output_tokens, Some(4189));
        assert_eq!(usage.total_tokens, Some(186587));
    }

    #[test]
    fn parses_provider_usage_metadata_and_agent_raw_usage_shapes() {
        let cases = [
            (
                "workbuddy",
                r#"[{"rawUsage":{"inputTokens":21,"outputTokens":5,"totalTokens":26,"inputTokensDetails":{"cached_tokens":3}}}]"#,
            ),
            (
                "openhands",
                r#"usage: input: 31 output: 7 cached: 4 total: 38"#,
            ),
            (
                "openjiuwen",
                r#"Token usage: input_tokens=41 output_tokens=9 cacheReadInputTokens=6 totalTokens=50"#,
            ),
        ];
        for (agent, output) in cases {
            let usage = token_usage_from_sources("cli2key", agent, output, "", None);
            assert_eq!(
                usage.input_tokens,
                Some(if agent == "workbuddy" {
                    21
                } else if agent == "openhands" {
                    31
                } else {
                    41
                }),
                "{agent} input"
            );
            assert_eq!(
                usage.output_tokens,
                Some(if agent == "workbuddy" {
                    5
                } else if agent == "openhands" {
                    7
                } else {
                    9
                }),
                "{agent} output"
            );
            assert_eq!(
                usage.cached_tokens,
                Some(if agent == "workbuddy" {
                    3
                } else if agent == "openhands" {
                    4
                } else {
                    6
                }),
                "{agent} cached"
            );
            assert_eq!(
                usage.total_tokens,
                Some(if agent == "workbuddy" {
                    26
                } else if agent == "openhands" {
                    38
                } else {
                    50
                }),
                "{agent} total"
            );
        }
    }

    #[test]
    fn counts_structured_agent_steps_and_falls_back_to_one() {
        let output = r#"{"type":"tool_call","step":1}
{"type":"tool_call","step":2}"#;
        assert_eq!(steps_from_sources("codex", output, ""), 2);
        assert_eq!(steps_from_sources("codex", "completed", ""), 1);
        assert_eq!(steps_from_sources("codex", "", ""), 0);
    }

    #[test]
    fn reads_usage_from_a_recorded_cassette() {
        let path = std::env::temp_dir().join(format!("asb-usage-cassette-{}", unix_ms()));
        fs::write(
            &path,
            r#"{"request":{},"response":{"body":"{\"usage\":{\"prompt_tokens\":2,\"completion_tokens\":3,\"total_tokens\":5}}"}}"#,
        )
        .expect("cassette writes");
        let usage = token_usage_from_sources("cli2key", "codex", "", "", Some(&path));
        assert_eq!(usage.input_tokens, Some(2));
        assert_eq!(usage.output_tokens, Some(3));
        assert_eq!(usage.total_tokens, Some(5));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn perf_permission_error_suggests_kernel_policy_not_sudoers() {
        let message = perf_permission_error();
        assert!(message.contains("kernel.perf_event_paranoid=-1"));
        assert!(message.contains("sudo sysctl -w kernel.perf_event_paranoid=-1"));
        assert!(message.contains("/etc/sysctl.d/99-asb-perf.conf"));
        assert!(message.contains("does not create sudoers rules"));
        assert!(!message.contains("sudoers.d"));
    }

    #[test]
    fn parses_mpstat_cpu_breakdown_into_averaged_metrics() {
        let metrics = parse_mpstat_metrics(
            "12:00:00 AM CPU %usr %nice %sys %iowait %irq %soft %steal %guest %gnice %idle\n12:00:00 AM all 1.0 2.0 3.0 4.0 5.0 6.0 7.0 8.0 9.0 55.0\n",
        );
        assert_eq!(metrics["cpu.user_pct"], 1.0);
        assert_eq!(metrics["cpu.system_pct"], 3.0);
        assert_eq!(metrics["cpu.iowait_pct"], 4.0);
        assert_eq!(metrics["cpu.guest_nice_pct"], 9.0);
        assert_eq!(metrics["cpu.idle_pct"], 55.0);
    }

    #[test]
    fn parses_process_scoped_pidstat_cpu_and_io_metrics() {
        let cpu = parse_pidstat_cpu_metrics(
            "Linux 6.1\n12:00:00 UID PID %usr %system %guest %wait %CPU CPU Command\n12:00:01 1000 42 10.00 2.00 0.00 1.00 12.00 3 agent\n12:00:02 1000 42 20.00 4.00 0.00 0.00 24.00 3 agent\n",
        );
        assert_eq!(cpu["process.cpu.user_pct"], 15.0);
        assert_eq!(cpu["process.cpu.system_pct"], 3.0);
        assert_eq!(cpu["process.cpu.total_pct"], 18.0);
        let io = parse_pidstat_io_metrics(
            "Linux 6.1\n12:00:00 UID PID kB_rd/s kB_wr/s kB_ccwr/s iodelay Command\n12:00:01 1000 42 1.0 2.0 0.5 3 agent\n",
        );
        assert_eq!(io["process.io.read_kib_per_s"], 1.0);
        assert_eq!(io["process.io.write_kib_per_s"], 2.0);
        assert_eq!(io["process.io.delay_ticks"], 3.0);
        let series = crate::collectors::adapters::mpstat::parse_series(
            "Linux 6.1\n12:00:00 UID PID %usr %system %guest %wait %CPU CPU Command\n12:00:01 1000 42 10.00 2.00 0.00 1.00 12.00 3 agent\n12:00:02 1000 42 20.00 4.00 0.00 0.00 24.00 3 agent\n",
        );
        assert_eq!(series["process.cpu.user_pct"], vec![10.0, 20.0]);
        assert_eq!(series["process.cpu.total_pct"], vec![12.0, 24.0]);
        let io_series = crate::collectors::adapters::iostat::parse_series(
            "Linux 6.1\n12:00:01 UID PID kB_rd/s kB_wr/s kB_ccwr/s iodelay Command\n12:00:01 1000 42 1.0 2.0 0.5 3 agent\n12:00:02 1000 42 2.0 4.0 1.0 5 agent\n",
        );
        assert_eq!(io_series["process.io.read_kib_per_s"], vec![1.0, 2.0]);
    }

    #[test]
    fn parses_iostat_cpu_and_device_throughput_and_latency() {
        let metrics = parse_iostat_metrics(
            "avg-cpu:  %user   %nice %system %iowait  %steal   %idle\n           1.00    2.00    3.00    4.00    5.00   85.00\n\nDevice            r/s     rkB/s   r_await w/s     wkB/s   w_await aqu-sz %util\nvda              10.00   20.00   1.50    30.00   40.00   2.50    0.10   7.00\n",
        );
        assert_eq!(metrics["cpu.user_pct"], 1.0);
        assert_eq!(metrics["cpu.iowait_pct"], 4.0);
        assert_eq!(metrics["device.read_ops_per_s"], 10.0);
        assert_eq!(metrics["device.read_await_ms"], 1.5);
        assert_eq!(metrics["device.write_kib_per_s"], 40.0);
        assert_eq!(metrics["device.util_pct"], 7.0);
    }

    #[test]
    fn parses_perf_stat_counters_into_stable_metrics() {
        let metrics = parse_perf_stat_metrics(
            "12.34,msec,task-clock,100.00,\n8,,context-switches,100.00,\n2,,cpu-migrations,100.00,\n4,,page-faults,100.00,\n1234,,cycles,100.00,\n567,,instructions,100.00,\n",
        );
        assert_eq!(metrics["perf.task_clock_ms"], 12.34);
        assert_eq!(metrics["perf.context_switches"], 8.0);
        assert_eq!(metrics["perf.cpu_migrations"], 2.0);
        assert_eq!(metrics["perf.page_faults"], 4.0);
        assert_eq!(metrics["perf.cycles"], 1234.0);
        assert_eq!(metrics["perf.instructions"], 567.0);
        let series = parse_perf_stat_series(
            "0.000,12.34,msec,task-clock,100.00,\n1.000,20.00,msec,task-clock,100.00,\n0.000,8,,context-switches,100.00,\n",
        );
        assert_eq!(series["perf.task_clock_ms"], vec![12.34, 20.0]);
        assert_eq!(series["perf.context_switches"], vec![8.0]);
    }

    #[test]
    fn bpftrace_profiles_are_target_scoped_and_parse_only_asb_metrics() {
        assert_eq!(
            bpftrace_profile_names().collect::<Vec<_>>(),
            vec![
                "sched-latency",
                "cpu-migrations",
                "futex-wait",
                "syscall-rate",
                "lock-contention"
            ]
        );
        let profiles = selected_bpftrace_profiles(&[
            "bpftrace:sched-latency".into(),
            "bpftrace:futex-wait".into(),
        ])
        .expect("profile selection");
        assert_eq!(profiles.len(), 2);
        assert_eq!(
            selected_bpftrace_profiles(&["bpftrace".into()])
                .expect("generic bpftrace selects built-ins")
                .len(),
            bpftrace_profile_names().count()
        );
        assert!(
            profiles
                .iter()
                .all(|profile| profile.program.contains("$1"))
        );
        assert!(
            profiles
                .iter()
                .all(|profile| profile.program.contains("interval:s:1"))
        );
        let metrics = parse_bpftrace_metrics(
            "sched-latency",
            "@asb_wakeup_latency_events: 7\n@asb_wakeup_latency_total_ns: 420\n@other: 99\n",
        );
        assert_eq!(metrics["bpftrace.sched-latency.wakeup_latency_events"], 7.0);
        assert_eq!(
            metrics["bpftrace.sched-latency.wakeup_latency_total_ns"],
            420.0
        );
        assert_eq!(metrics.len(), 2);
        let series = parse_bpftrace_series(
            "sched-latency",
            "@asb_wakeup_latency_events: 2\n@asb_wakeup_latency_events: 5\n",
        );
        assert_eq!(
            series["bpftrace.sched-latency.wakeup_latency_events"],
            vec![2.0, 5.0]
        );
    }

    #[test]
    fn perfetto_config_collects_scheduler_process_and_trace_marker_events() {
        let config = perfetto_config(None);
        for event in [
            "sched/sched_switch",
            "sched/sched_waking",
            "sched/sched_wakeup_new",
            "sched/sched_process_exit",
            "task/task_newtask",
            "ftrace/print",
        ] {
            assert!(config.contains(event), "missing {event}");
        }
        assert!(config.contains("linux.process_stats"));
        assert!(config.contains("compact_sched"));
    }

    #[test]
    fn project_local_perfetto_uses_tracebox_binary_name() {
        assert_eq!(tool_binary_name("perfetto"), "tracebox");
        assert_eq!(tool_binary_name("perf"), "perf");
    }

    #[test]
    fn perfetto_configuration_uses_supported_tracebox_schema() {
        let config = perfetto_config(Some(42));
        assert!(!config.contains("pid_filter"));
        assert!(config.contains("scan_all_processes_on_start: false"));
    }

    #[test]
    fn perfetto_queries_scope_to_the_target_process_tree() {
        let metrics = perfetto_metric_queries(Some(42), "1-2");
        assert!(
            metrics
                .iter()
                .all(|(_, query)| query.contains("WITH RECURSIVE tree"))
        );
        assert!(metrics.iter().all(|(_, query)| query.contains("pid=42")));
        assert!(
            metrics
                .iter()
                .any(|(_, query)| query.contains("th.upid IN tree"))
        );
        let series = perfetto_series_queries(Some(42), "1-2");
        assert!(
            series
                .iter()
                .all(|(_, query)| query.contains("asb.agent.started.1-2"))
        );
        assert!(
            series
                .iter()
                .all(|(_, query)| query.contains("asb.agent.exited.1-2"))
        );
        assert_ne!(perfetto_scope_sql(42, "1-2"), perfetto_scope_sql(42, "1-3"));
        assert!(
            series
                .iter()
                .all(|(_, query)| query.contains("bounds.start_ts"))
        );
        let event_series = &series[0].1;
        assert!(event_series.contains("CAST((s.ts-bounds.start_ts)/1000000000 AS INTEGER)"));
        assert!(event_series.contains("s.ts >= bounds.start_ts"));
        let duration_series = &series[1].1;
        assert!(duration_series.contains("bucket_ranges(bucket)"));
        assert!(duration_series.contains("relevant AS"));
        assert!(
            duration_series
                .contains("MIN(r.ts+r.dur,bounds.start_ts+(bucket_ranges.bucket+1)*1000000000)")
        );
        assert!(
            duration_series.contains("MAX(r.ts,bounds.start_ts+bucket_ranges.bucket*1000000000)")
        );
        assert!(perfetto_metric_queries(None, "1-2").is_empty());
    }

    #[test]
    fn parses_perfetto_csv_series_with_trace_timestamps() {
        let (timestamps, values) = parse_perfetto_series_output(
            "\"timestamp_s\",\"value\"\n\"12\",\"3\"\n\"13.5\",\"7\"\n",
        );
        assert_eq!(timestamps, vec![12_000.0, 13_500.0]);
        assert_eq!(values, vec![3.0, 7.0]);
    }
}
