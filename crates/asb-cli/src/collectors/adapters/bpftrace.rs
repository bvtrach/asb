use super::super::{
    ArtifactDisplay, GraphSection, PreflightContext, ProcessContext, ProcessData, StaticCollector,
};
use crate::measurements;
use std::fmt::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};
pub(super) static ADAPTER: StaticCollector = StaticCollector::new(
    "bpftrace",
    Some("ASB_MEASURE_BPFTRACE"),
    Some("bpftrace"),
    true,
)
.with_artifacts(
    |name| name.starts_with("bpftrace-"),
    |name| {
        (name.starts_with("bpftrace-") && name.ends_with(".svg")).then_some(ArtifactDisplay::Svg {
            class: "bpftrace-profile",
            label: "bpftrace profile SVG",
        })
    },
)
.with_graph_section(|name| {
    (name.starts_with("metric-bpftrace-") || name.starts_with("series-bpftrace-"))
        .then_some(GraphSection {
            title: "bpftrace profiles",
            description: "Cross-run SVG graphs for every bpftrace aggregate emitted by the selected profiles.",
        })
})
.with_preflight(preflight)
.with_configure(|session, selected| {
    let state = session.state::<BpftraceState>();
    let mut state = state.borrow_mut();
    state.enabled = super::super::selected_or_environment("bpftrace", selected);
    state.profiles = selected_bpftrace_profiles(selected)?;
    Ok(())
})
.with_processing(|_| true, process)
.with_attach_pid(|session, pid| session.attach_bpftrace(pid));

fn process(
    session: &mut measurements::Session,
    context: &ProcessContext<'_>,
) -> Result<ProcessData, String> {
    let mut result = ProcessData::from_output(context);
    if let Some(profile) = context.name.strip_prefix("bpftrace-") {
        result.metrics = parse_bpftrace_metrics(profile, context.stdout);
        result.series = parse_bpftrace_series(profile, context.stdout);
        write_bpftrace_profile_svg(&session.dir, profile, &result.status, &result.metrics)?;
    }
    Ok(result)
}

fn preflight(context: &mut PreflightContext<'_>) -> Result<(), String> {
    let profiles = selected_bpftrace_profiles(context.selected)?;
    let custom = super::super::selected_or_environment("bpftrace", context.selected)
        && std::env::var_os("ASB_BPFTRACE_PROGRAMS").is_some();
    if !custom && profiles.is_empty() {
        return Ok(());
    }
    let executable = measurements::resolve_executable(context.tool_dir, "bpftrace");
    let args = ["-e", "BEGIN { exit(); }"];
    match Command::new(&executable)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Ok(status) if status.success() => {}
        Ok(_) => {
            let sudo = Command::new("sudo")
                .args(["-n", "--"])
                .arg(&executable)
                .args(args)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            if sudo.is_ok_and(|status| status.success()) {
                context.sudo_collectors.insert("bpftrace".into());
            } else {
                return Err(measurements::passwordless_sudo_error(
                    "bpftrace",
                    &executable,
                ));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("cannot probe bpftrace collector: {error}")),
    }
    if !profiles.is_empty() && !executable.is_file() {
        return Err("selected bpftrace profiles need a project-local bpftrace executable. What to do: run `asb tool install bpftrace` in this project, then rerun the benchmark.".into());
    }
    for profile in profiles {
        if bpftrace_profile_probe(&executable, profile, false)? {
            continue;
        }
        if bpftrace_profile_probe(&executable, profile, true)? {
            context.sudo_collectors.insert("bpftrace".into());
            continue;
        }
        if measurements::passwordless_sudo_available() {
            return Err(format!(
                "bpftrace profile `{}` cannot attach on this kernel. It is not a sudo authorization failure; inspect the kernel tracepoint support and the profile's JSON/SVG documentation before rerunning.",
                profile.name
            ));
        }
        return Err(measurements::passwordless_sudo_error(
            "bpftrace",
            &executable,
        ));
    }
    Ok(())
}

use crate::collectors;
use crate::measurements::{Collector, env_true, escape_svg, parse_float, write_unavailable};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::time::Instant;

#[derive(Default)]
struct BpftraceState {
    enabled: bool,
    profiles: Vec<BpftraceProfile>,
}

impl measurements::Session {
    pub(crate) fn attach_bpftrace(&mut self, pid: u32) -> Result<(), String> {
        if self.state::<BpftraceState>().borrow().enabled
            && std::env::var_os("ASB_BPFTRACE_PROGRAMS").is_some()
        {
            self.start_bpftrace(pid)?;
        }
        self.start_bpftrace_profiles(pid)
    }

    fn start_bpftrace(&mut self, pid: u32) -> Result<(), String> {
        let specs = std::env::var("ASB_BPFTRACE_PROGRAMS")
            .map_err(|_| "ASB_MEASURE_BPFTRACE=1 requires ASB_BPFTRACE_PROGRAMS".to_owned())?;
        for (index, spec) in specs
            .split(';')
            .filter(|s| !s.trim().is_empty())
            .enumerate()
        {
            let (program, params) = spec.split_once('?').map_or((spec, ""), |parts| parts);
            let mut command = self.command("bpftrace");
            command.args(["-p", &pid.to_string()]);
            command.arg(program);
            for parameter in params.split('&').filter(|s| !s.is_empty()) {
                let (key, value) = parameter.split_once('=').ok_or_else(|| {
                    format!("invalid bpftrace parameter `{parameter}`; use PROGRAM?name=value")
                })?;
                if key.is_empty() || value.is_empty() {
                    return Err(format!("invalid bpftrace parameter `{parameter}`"));
                }
                command.args(["-D", &format!("{key}={value}")]);
            }
            let stdout_path = self.dir.join(format!("bpftrace-{index}.stdout"));
            let stderr_path = self.dir.join(format!("bpftrace-{index}.stderr"));
            let child = match command
                .stdout(Stdio::from(
                    File::create(&stdout_path)
                        .map_err(|e| format!("cannot create bpftrace output: {e}"))?,
                ))
                .stderr(Stdio::from(File::create(&stderr_path).map_err(|e| {
                    format!("cannot create bpftrace error output: {e}")
                })?))
                .spawn()
            {
                Ok(child) => child,
                Err(error) if !env_true("ASB_MEASURE_BPFTRACE") => {
                    write_unavailable(
                        &self.dir.join(format!("bpftrace-{index}.json")),
                        &format!("bpftrace-{index}"),
                        error.to_string(),
                    )?;
                    continue;
                }
                Err(error) => {
                    return Err(format!("cannot start bpftrace program {program}: {error}"));
                }
            };
            self.collectors.push(Collector {
                name: format!("bpftrace-{index}"),
                adapter: collectors::find("bpftrace").expect("registered collector"),
                child,
                started: Instant::now(),
                stdout_path,
                stderr_path,
            });
        }
        Ok(())
    }

    /// Start each selected built-in profile after the agent PID is known. This
    /// keeps all probes scoped to the benchmarked agent rather than the host.
    fn start_bpftrace_profiles(&mut self, pid: u32) -> Result<(), String> {
        let profiles = self.state::<BpftraceState>().borrow().profiles.clone();
        let prelude = process_tree_prelude(pid);
        for profile in &profiles {
            let name = format!("bpftrace-{}", profile.name);
            let stdout_path = self.dir.join(format!("{name}.stdout"));
            let stderr_path = self.dir.join(format!("{name}.stderr"));
            let stdout = File::create(&stdout_path)
                .map_err(|error| format!("cannot create {name} output: {error}"))?;
            let stderr = File::create(&stderr_path)
                .map_err(|error| format!("cannot create {name} error output: {error}"))?;
            let child = self
                .command("bpftrace")
                .args(["-e", &format!("{prelude}\n{}", profile.program)])
                .arg(pid.to_string())
                .stdout(Stdio::from(stdout))
                .stderr(Stdio::from(stderr))
                .spawn();
            match child {
                Ok(child) => self.collectors.push(Collector {
                    name,
                    adapter: collectors::find("bpftrace").expect("registered collector"),
                    child,
                    started: Instant::now(),
                    stdout_path,
                    stderr_path,
                }),
                Err(error) if !env_true("ASB_MEASURE_BPFTRACE") => write_unavailable(
                    &self.dir.join(format!("bpftrace-{}.json", profile.name)),
                    &format!("bpftrace-{}", profile.name),
                    format!("{}; profile: {}", error, profile.description),
                )?,
                Err(error) => {
                    return Err(format!(
                        "cannot start bpftrace profile {}: {error}",
                        profile.name
                    ));
                }
            }
        }
        Ok(())
    }
}

/// Seed the built-in probes with every process currently below the agent and
/// keep the set current as the agent forks helpers. This prevents unrelated
/// host tracepoints from entering a process-scoped measurement.
fn process_tree_prelude(root: u32) -> String {
    process_tree_prelude_from_pids(measurements::process_tree_pids(root))
}

fn process_tree_prelude_from_pids(pids: impl IntoIterator<Item = u32>) -> String {
    let mut prelude = String::from("BEGIN {\n");
    for pid in pids {
        let _ = writeln!(prelude, "  @asb_tree[{pid}] = 1;");
    }
    prelude.push_str(
        "}\ntracepoint:sched:sched_process_fork /@asb_tree[args->parent_pid]/ { @asb_tree[args->child_pid] = 1; }\n",
    );
    prelude
}

/// A bounded, target-PID-scoped bpftrace program. Profiles use only stable
/// kernel tracepoint families where possible; users can still supply a
/// runtime-specific program through ASB_BPFTRACE_PROGRAMS.
#[derive(Clone, Copy)]
pub(crate) struct BpftraceProfile {
    pub(crate) name: &'static str,
    pub(crate) description: &'static str,
    pub(crate) program: &'static str,
}

const BPFTRACE_PROFILES: &[BpftraceProfile] = &[
    BpftraceProfile {
        name: "sched-latency",
        description: "wakeup-to-run latency for agent tasks",
        program: include_str!("bpftrace/sched_latency.bt"),
    },
    BpftraceProfile {
        name: "cpu-migrations",
        description: "agent task migrations between CPUs",
        program: include_str!("bpftrace/cpu_migrations.bt"),
    },
    BpftraceProfile {
        name: "futex-wait",
        description: "futex wait count and duration for agent threads",
        program: include_str!("bpftrace/futex_wait.bt"),
    },
    BpftraceProfile {
        name: "syscall-rate",
        description: "aggregate system-call pressure from the agent process",
        program: include_str!("bpftrace/syscall_rate.bt"),
    },
    BpftraceProfile {
        name: "lock-contention",
        description: "kernel lock contention for agent threads when supported",
        program: include_str!("bpftrace/lock_contention.bt"),
    },
];

/// Names accepted by `bpftrace:PROFILE` collector selections and
/// `ASB_BPFTRACE_PROFILES=PROFILE,...`.
pub fn bpftrace_profile_names() -> impl Iterator<Item = &'static str> {
    BPFTRACE_PROFILES.iter().map(|profile| profile.name)
}

pub(crate) fn selected_bpftrace_profiles(
    selected: &[String],
) -> Result<Vec<BpftraceProfile>, String> {
    let mut requested = selected
        .iter()
        .filter_map(|value| value.strip_prefix("bpftrace:"))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if let Ok(value) = std::env::var("ASB_BPFTRACE_PROFILES") {
        requested.extend(
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned),
        );
    }
    if selected.iter().any(|value| value == "bpftrace") && requested.is_empty() {
        requested.extend(
            BPFTRACE_PROFILES
                .iter()
                .map(|profile| profile.name.to_owned()),
        );
    }
    if requested.iter().any(|value| value == "all") {
        return Ok(BPFTRACE_PROFILES.to_vec());
    }
    requested.sort();
    requested.dedup();
    requested
        .iter()
        .map(|name| {
            BPFTRACE_PROFILES
                .iter()
                .find(|profile| profile.name == name)
                .copied()
                .ok_or_else(|| {
                    format!(
                        "unknown bpftrace profile `{name}`; choose one of: {}",
                        bpftrace_profile_names().collect::<Vec<_>>().join(", ")
                    )
                })
        })
        .collect()
}

/// Compile and attach a concrete profile before the agent starts. The appended
/// interval exit keeps the probe bounded while exercising the exact tracepoints,
/// predicates, and process-tree parameter form used during the run.
pub(crate) fn bpftrace_profile_probe(
    executable: &Path,
    profile: BpftraceProfile,
    sudo: bool,
) -> Result<bool, String> {
    // Compile the same process-tree prelude used by the live collector. The
    // preflight must not reject a valid profile merely because its tree map is
    // populated by the runtime attach path.
    let program = profile_probe_program(profile);
    let mut command = if sudo {
        let mut command = Command::new("sudo");
        command.args(["-n", "--"]);
        command.arg(executable);
        command
    } else {
        Command::new(executable)
    };
    command
        .args(["-e", &program, "0"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .map_err(|error| format!("cannot probe bpftrace profile {}: {error}", profile.name))
}

pub(crate) fn write_bpftrace_profile_svg(
    dir: &Path,
    profile: &str,
    status: &str,
    metrics: &BTreeMap<String, f64>,
) -> Result<(), String> {
    let mut lines = vec![format!("status: {status}")];
    for (name, value) in metrics {
        lines.push(format!(
            "{}: {:.0}",
            name.rsplit('.').next().unwrap_or(name),
            value
        ));
    }
    if metrics.is_empty() {
        lines.push("no aggregate values emitted; inspect the paired JSON result".into());
    }
    let height = 76 + lines.len() * 26;
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"760\" height=\"{height}\" viewBox=\"0 0 760 {height}\"><title>bpftrace {}</title><rect width=\"760\" height=\"{height}\" fill=\"#101827\"/><rect x=\"16\" y=\"16\" width=\"728\" height=\"{}\" rx=\"10\" fill=\"#182334\"/><text x=\"36\" y=\"48\" fill=\"#e5edf8\" font-family=\"sans-serif\" font-size=\"20\" font-weight=\"bold\">bpftrace: {}</text>",
        escape_svg(profile),
        height - 32,
        escape_svg(profile)
    );
    for (index, line) in lines.iter().enumerate() {
        let color = if index == 0 && status == "completed" {
            "#78dba9"
        } else if index == 0 {
            "#ffb86b"
        } else {
            "#b9c7dc"
        };
        svg.push_str(&format!(
            "<text x=\"36\" y=\"{}\" fill=\"{color}\" font-family=\"monospace\" font-size=\"15\">{}</text>",
            76 + index * 26,
            escape_svg(line)
        ));
    }
    svg.push_str("</svg>\n");
    fs::write(dir.join(format!("bpftrace-{profile}.svg")), svg)
        .map_err(|error| format!("cannot write bpftrace profile SVG: {error}"))
}

fn profile_probe_program(profile: BpftraceProfile) -> String {
    format!(
        "{}\n{}\ninterval:ms:1 {{ exit(); }}",
        // PID 0 is only a compile-time seed. Expanding its live process tree
        // would accidentally scope the preflight to the host init tree.
        process_tree_prelude_from_pids([0]),
        profile.program
    )
}

#[cfg(test)]
mod tests {
    use super::{BPFTRACE_PROFILES, profile_probe_program};

    #[test]
    fn profile_preflight_uses_the_same_seeded_tree_program_as_runtime() {
        let profile = BPFTRACE_PROFILES
            .iter()
            .find(|profile| profile.name == "sched-latency")
            .copied()
            .expect("built-in profile");
        let program = profile_probe_program(profile);
        assert!(program.contains("@asb_tree[0] = 1;"));
        assert!(program.contains("sched_process_fork"));
        assert!(program.contains(profile.program));
        assert!(program.contains("interval:ms:1 { exit(); }"));
        assert!(!program.contains("@asb_tree[1] = 1;"));
        assert!(!program.contains("BEGIN { exit(); }"));
    }
}

/// bpftrace prints aggregation maps as `@asb_metric: value`. Only these
/// ASB-owned map names are accepted, so arbitrary program output cannot create
/// comparison metrics accidentally.
pub(crate) fn parse_bpftrace_metrics(profile: &str, output: &str) -> BTreeMap<String, f64> {
    let mut metrics = BTreeMap::new();
    for line in output.lines() {
        let Some((name, value)) = line.trim().split_once(':') else {
            continue;
        };
        let Some(name) = name.trim().strip_prefix("@asb_") else {
            continue;
        };
        let Some(value) = parse_float(value) else {
            continue;
        };
        metrics.insert(format!("bpftrace.{profile}.{name}"), value);
    }
    metrics
}

pub(crate) fn parse_bpftrace_series(profile: &str, output: &str) -> BTreeMap<String, Vec<f64>> {
    let mut series: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for line in output.lines() {
        let Some((name, value)) = line.trim().split_once(':') else {
            continue;
        };
        let Some(name) = name.trim().strip_prefix("@asb_") else {
            continue;
        };
        let Some(value) = parse_float(value) else {
            continue;
        };
        series
            .entry(format!("bpftrace.{profile}.{name}"))
            .or_default()
            .push(value);
    }
    series
}
