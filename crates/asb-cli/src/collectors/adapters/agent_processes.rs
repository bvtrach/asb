//! Sample only the selected agent entry point, not every descendant or host task.
use crate::collectors::{CollectorAdapter, GraphSection, LifecycleEvent};
use crate::measurements::{self, Session};
use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub(super) struct AgentProcesses<const MEMORY: bool>;

#[derive(Default)]
struct State {
    selected: bool,
    memory: bool,
    workers: bool,
    identity: Option<Identity>,
    sampler: Option<Sampler>,
    totals: Totals,
}

#[derive(Clone)]
struct Identity {
    executable: PathBuf,
    interpreter: Option<PathBuf>,
    runtime_arguments: Option<(String, String)>,
}

#[derive(Default)]
struct Totals {
    seen: BTreeSet<(u32, u64)>,
    peak_rss_kib: u64,
    samples: u64,
    incomplete: bool,
}

struct Sampler {
    stop: mpsc::Sender<()>,
    thread: Option<JoinHandle<Totals>>,
}

impl Sampler {
    fn finish(mut self) -> Totals {
        let _ = self.stop.send(());
        self.thread
            .take()
            .and_then(|t| t.join().ok())
            .unwrap_or_else(|| Totals {
                incomplete: true,
                ..Totals::default()
            })
    }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl<const MEMORY: bool> CollectorAdapter for AgentProcesses<MEMORY> {
    fn id(&self) -> &'static str {
        if MEMORY {
            "agent-memory"
        } else {
            "agent-workers"
        }
    }
    fn environment_flag(&self) -> Option<&'static str> {
        Some(if MEMORY {
            "ASB_MEASURE_AGENT_MEMORY"
        } else {
            "ASB_MEASURE_AGENT_WORKERS"
        })
    }
    fn requires_pid(&self) -> bool {
        true
    }

    fn configure(&self, session: &mut Session, selected: &[String]) -> Result<(), String> {
        if crate::collectors::selected_or_environment(self.id(), selected) {
            let state = session.state::<State>();
            let mut state = state.borrow_mut();
            state.selected = true;
            if MEMORY {
                state.memory = true;
            } else {
                state.workers = true;
            }
        }
        Ok(())
    }

    fn agent_launch(&self, session: &mut Session, program: &str, args: &[String]) {
        let state = session.state::<State>();
        let mut state = state.borrow_mut();
        if state.selected {
            state.identity = Identity::resolve(program, args);
        }
    }

    fn attach_pid(&self, session: &mut Session, pid: u32) -> Result<(), String> {
        let state = session.state::<State>();
        let mut state = state.borrow_mut();
        if state.selected
            && state.sampler.is_none()
            && cfg!(target_os = "linux")
            && let Some(identity) = state.identity.clone()
        {
            let (stop, receiver) = mpsc::channel();
            let interval = Duration::from_millis(measurements::measurement_interval_ms());
            state.sampler = Some(Sampler {
                stop,
                thread: Some(thread::spawn(move || {
                    let mut totals = Totals::default();
                    loop {
                        totals.sample(pid, &identity);
                        if receiver.recv_timeout(interval) != Err(mpsc::RecvTimeoutError::Timeout) {
                            return totals;
                        }
                    }
                })),
            });
        }
        Ok(())
    }

    fn lifecycle_event(&self, session: &mut Session, event: LifecycleEvent) -> Result<(), String> {
        if matches!(
            event,
            LifecycleEvent::AgentExecuteEnd | LifecycleEvent::RunFailed
        ) {
            let state = session.state::<State>();
            let mut state = state.borrow_mut();
            if let Some(sampler) = state.sampler.take() {
                state.totals = sampler.finish();
            }
        }
        Ok(())
    }

    fn finish(&self, session: &mut Session) -> Result<(), String> {
        self.lifecycle_event(session, LifecycleEvent::AgentExecuteEnd)?;
        let state = session.state::<State>();
        let state = state.borrow();
        if (MEMORY && !state.memory) || (!MEMORY && !state.workers) {
            return Ok(());
        }
        let available = state.totals.samples > 0 && !state.totals.seen.is_empty();
        let reason = if !cfg!(target_os = "linux") {
            Some("Agent process sampling requires Linux /proc.")
        } else if state.identity.is_none() {
            Some("Cannot resolve the selected agent executable.")
        } else if !available {
            Some(
                "No matching agent process was observed; the run may be shorter than the sampling interval or use a different launcher entry point.",
            )
        } else if state.totals.incomplete {
            Some(
                "Agent process accounting was incomplete (unreadable process data or observation limit); metrics are omitted.",
            )
        } else {
            None
        };
        let metrics = if !available || state.totals.incomplete {
            serde_json::json!({})
        } else if MEMORY {
            serde_json::json!({ "peak_rss_mib": state.totals.peak_rss_kib as f64 / 1024.0 })
        } else {
            serde_json::json!({ "spawned_worker_count": state.totals.seen.len().saturating_sub(1) })
        };
        measurements::write_json(
            &session.dir.join(format!("{}.json", self.id())),
            &serde_json::json!({
                "schema_version": 1, "collector": self.id(),
                "status": if state.totals.incomplete { "incomplete" } else if available { "observed" } else { "unavailable" },
                "reason": reason,
                "source": "linux-proc-sampling", "scope": "selected-agent-entry-point-only",
                "sampling_interval_ms": measurements::measurement_interval_ms(),
                "samples": state.totals.samples,
                "limitations": "Sampled lower bound: short-lived processes can be missed. Workers are additional OS processes with the same executable or script entry point, not logical subagents within one process. Tools and the ASB gate are excluded. RSS includes shared pages per matching process, not unique physical memory. Different wrapper entry points are not inferred.",
                "metrics": metrics
            }),
        )
    }

    fn graph_section(&self, name: &str) -> Option<GraphSection> {
        name.starts_with(&format!("metric-{}.", self.id())).then_some(if MEMORY {
            GraphSection { title: "Agent memory", description: "Sampled peak aggregate RSS (MiB) of matching agent processes only. Tool subprocesses and host memory are excluded; shared pages may be counted in multiple agent processes." }
        } else {
            GraphSection { title: "Agent workers", description: "Observed additional agent OS processes per execution, excluding the primary agent and tool subprocesses. Sampling may miss short-lived workers; logical subagents inside one process are not measured." }
        })
    }
}

impl Identity {
    fn resolve(program: &str, args: &[String]) -> Option<Self> {
        let executable = resolve_executable(program)?;
        let name = executable.file_name()?.to_str()?;
        // A prepared adapter can invoke an interpreter rather than an agent
        // script directly. Never identify all Node/Python/shell tasks as agents.
        if name.starts_with("python")
            || matches!(
                name,
                "node"
                    | "nodejs"
                    | "bun"
                    | "deno"
                    | "sh"
                    | "bash"
                    | "dash"
                    | "zsh"
                    | "perl"
                    | "ruby"
            )
        {
            let first = args.first()?;
            if matches!(first.as_str(), "-c" | "-e" | "--eval" | "-m") {
                return Some(Self {
                    executable,
                    interpreter: None,
                    runtime_arguments: Some((first.clone(), args.get(1)?.clone())),
                });
            }
            if first.starts_with('-') {
                return None;
            }
            return Some(Self {
                executable: fs::canonicalize(first).ok()?,
                interpreter: Some(executable),
                runtime_arguments: None,
            });
        }
        let prefix = read_bounded(&executable, 4096).ok()?;
        let interpreter = if prefix.starts_with(b"#!") {
            let line = std::str::from_utf8(prefix.split(|byte| *byte == b'\n').next()?).ok()?;
            let mut words = line[2..].split_whitespace();
            let first = words.next()?;
            let command = if Path::new(first).file_name()? == "env" {
                words.find(|word| !word.starts_with('-'))?
            } else {
                first
            };
            Some(resolve_executable(command)?)
        } else {
            None
        };
        Some(Self {
            executable,
            interpreter,
            runtime_arguments: None,
        })
    }

    fn matches(&self, proc: &Path) -> bool {
        if let Some((flag, entry)) = &self.runtime_arguments {
            if !fs::read_link(proc.join("exe")).is_ok_and(|path| path == self.executable) {
                return false;
            }
            let Ok(args) = read_bounded(&proc.join("cmdline"), 64 * 1024) else {
                return false;
            };
            let mut args = args.split(|byte| *byte == 0).skip(1);
            return args.next() == Some(flag.as_bytes()) && args.next() == Some(entry.as_bytes());
        }
        let Some(interpreter) = &self.interpreter else {
            return fs::read_link(proc.join("exe")).is_ok_and(|path| path == self.executable);
        };
        if !fs::read_link(proc.join("exe")).is_ok_and(|path| path == *interpreter) {
            return false;
        }
        // Match the interpreter's entry point, never an arbitrary later argument
        // (which might be an agent prompt mentioning its own executable).
        let Ok(args) = read_bounded(&proc.join("cmdline"), 64 * 1024) else {
            return false;
        };
        let Some(entry) = args.split(|byte| *byte == 0).nth(1) else {
            return false;
        };
        // Do not mistake `node -e SCRIPT`, `python -c SCRIPT`, or option
        // values for an entry point. Unrecognized interpreter launch forms
        // are deliberately not guessed.
        if entry.is_empty() || entry.starts_with(b"-") {
            return false;
        }
        let Ok(entry) = std::str::from_utf8(entry) else {
            return false;
        };
        let path = Path::new(entry);
        let path = if path.is_absolute() {
            path.to_owned()
        } else {
            let Ok(cwd) = fs::read_link(proc.join("cwd")) else {
                return false;
            };
            cwd.join(path)
        };
        fs::canonicalize(path).is_ok_and(|path| path == self.executable)
    }
}

impl Totals {
    fn sample(&mut self, root: u32, identity: &Identity) {
        let mut rss = 0u64;
        let mut matched = false;
        for pid in measurements::process_tree_pids(root) {
            let proc = PathBuf::from(format!("/proc/{pid}"));
            if !identity.matches(&proc) {
                continue;
            }
            let Some((start, resident)) = process_memory(&proc) else {
                // A task exiting between identity and RSS reads is normal.
                if proc.exists() {
                    self.incomplete = true;
                }
                continue;
            };
            // Bound memory usage even for pathological long runs.
            if self.seen.len() >= 100_000 && !self.seen.contains(&(pid, start)) {
                self.incomplete = true;
                continue;
            }
            self.seen.insert((pid, start));
            rss = rss.saturating_add(resident);
            matched = true;
        }
        if matched {
            self.samples += 1;
            self.peak_rss_kib = self.peak_rss_kib.max(rss);
        }
    }
}

fn resolve_executable(program: &str) -> Option<PathBuf> {
    let path = if program.contains('/') {
        PathBuf::from(program)
    } else {
        std::env::split_paths(&std::env::var_os("PATH")?)
            .map(|dir| dir.join(program))
            .find(|path| path.is_file())?
    };
    fs::canonicalize(path).ok()
}

fn read_bounded(path: &Path, limit: u64) -> std::io::Result<Vec<u8>> {
    let mut data = Vec::new();
    fs::File::open(path)?.take(limit).read_to_end(&mut data)?;
    Ok(data)
}

fn process_memory(proc: &Path) -> Option<(u64, u64)> {
    let stat = String::from_utf8(read_bounded(&proc.join("stat"), 4096).ok()?).ok()?;
    // Field 22 is start time; comm can contain spaces and closing parentheses.
    let start = stat
        .rsplit_once(')')?
        .1
        .split_whitespace()
        .nth(19)?
        .parse()
        .ok()?;
    let status = String::from_utf8(read_bounded(&proc.join("status"), 65536).ok()?).ok()?;
    let rss = status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    Some((start, rss))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_identity_does_not_match_an_unrelated_process() {
        if !cfg!(target_os = "linux") {
            return;
        }
        let identity = Identity {
            executable: PathBuf::from("/not/the/selected/agent"),
            interpreter: None,
            runtime_arguments: None,
        };
        assert!(!identity.matches(Path::new("/proc/self")));
        let identity =
            Identity::resolve(std::env::current_exe().unwrap().to_str().unwrap(), &[]).unwrap();
        assert!(identity.matches(Path::new("/proc/self")));
        assert!(process_memory(Path::new("/proc/self")).unwrap().1 > 0);
    }

    #[cfg(unix)]
    #[test]
    fn script_match_requires_both_interpreter_and_exact_entry_point() {
        use std::os::unix::fs::symlink;
        let root =
            std::env::temp_dir().join(format!("asb-process-identity-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let script = root.join("agent.js");
        fs::write(&script, "fixture").unwrap();
        let executable = fs::canonicalize(std::env::current_exe().unwrap()).unwrap();
        let identity = Identity {
            executable: fs::canonicalize(&script).unwrap(),
            interpreter: Some(executable.clone()),
            runtime_arguments: None,
        };
        symlink(&executable, root.join("exe")).unwrap();
        fs::write(
            root.join("cmdline"),
            format!("runtime\0{}\0--prompt\0", script.display()),
        )
        .unwrap();
        assert!(identity.matches(&root));
        fs::write(
            root.join("cmdline"),
            format!("runtime\0-e\0{}\0", script.display()),
        )
        .unwrap();
        assert!(!identity.matches(&root));
        fs::write(
            root.join("cmdline"),
            format!("runtime\0tool.js\0{}\0", script.display()),
        )
        .unwrap();
        assert!(!identity.matches(&root));
        fs::remove_file(root.join("exe")).unwrap();
        symlink("/not/the/interpreter", root.join("exe")).unwrap();
        fs::write(root.join("cmdline"), format!("cat\0{}\0", script.display())).unwrap();
        assert!(!identity.matches(&root));
        fs::remove_file(root.join("exe")).unwrap();
        symlink(&executable, root.join("exe")).unwrap();
        let inline = Identity {
            executable,
            interpreter: None,
            runtime_arguments: Some(("-c".into(), "agent_launcher()".into())),
        };
        fs::write(
            root.join("cmdline"),
            b"python\0-c\0agent_launcher()\0prompt\0",
        )
        .unwrap();
        assert!(inline.matches(&root));
        fs::write(root.join("cmdline"), b"python\0-c\0tool_call()\0").unwrap();
        assert!(!inline.matches(&root));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stopping_sampler_does_not_wait_for_a_long_sampling_interval() {
        let (stop, receiver) = mpsc::channel();
        let sampler = Sampler {
            stop,
            thread: Some(thread::spawn(move || {
                let _ = receiver.recv_timeout(Duration::from_secs(60));
                Totals::default()
            })),
        };
        let start = std::time::Instant::now();
        assert!(sampler.finish().seen.is_empty());
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}
