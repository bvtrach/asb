//! Component-owned runtime declarations and bounded local discovery.
use crate::{Error, project};
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Dependency {
    File {
        path: PathBuf,
    },
    Program {
        path: PathBuf,
        search: Option<String>,
    },
    Python {
        path: PathBuf,
        site: bool,
    },
    Process {
        pid: u32,
    },
}

pub(crate) struct Context<'a> {
    pub root: &'a Path,
    pub project: &'a project::ProjectConfig,
}

impl Context<'_> {
    pub(crate) fn tool_root(&self, id: &str) -> PathBuf {
        project::resolve(self.root, &self.project.tools_dir).join(id)
    }
}

impl Dependency {
    pub(crate) fn file(path: impl Into<PathBuf>) -> Self {
        Self::File { path: path.into() }
    }
    pub(crate) fn program(path: impl Into<PathBuf>) -> Self {
        Self::Program {
            path: path.into(),
            search: None,
        }
    }
    pub(crate) fn command(command: &Command) -> Self {
        let search = command
            .get_envs()
            .find(|(key, _)| *key == OsStr::new("PATH"))
            .map(|(_, value)| value.and_then(OsStr::to_str).unwrap_or("").to_owned());
        Self::Program {
            path: PathBuf::from(command.get_program()),
            search,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Discovered {
    schema_version: u32,
    paths: Vec<PathBuf>,
}

pub(crate) fn for_plan(
    parent: &Path,
    plan: &crate::sweep::plan::SweepPlan,
    mode: Option<crate::tape::Mode>,
) -> Result<Vec<Dependency>, Error> {
    let mut inputs = vec![
        Dependency::Process {
            pid: std::process::id(),
        },
        Dependency::file("/etc/os-release"),
        Dependency::file("/proc/sys/kernel/osrelease"),
        Dependency::file("/etc/ld.so.cache"),
        Dependency::file("/etc/ld.so.conf"),
        Dependency::file("/etc/ld.so.conf.d"),
    ];
    let mut selected = std::collections::BTreeSet::new();
    for entry in &plan.entries {
        let (root, project) = project::load(&parent.join(&entry.reference))?;
        let identity = (
            root.clone(),
            entry.configuration.agent.clone(),
            entry.configuration.workload.clone(),
            entry.configuration.provider.clone(),
            serde_json::to_string(&entry.configuration.measurements).map_err(|_| unavailable())?,
        );
        if !selected.insert(identity) {
            continue;
        }
        let context = Context {
            root: &root,
            project: &project,
        };
        let agent = crate::agents::find(&entry.configuration.agent).ok_or_else(unavailable)?;
        inputs.extend(agent.runtime_inputs(&context)?);
        inputs.extend(crate::workloads::runtime_inputs(
            &entry.configuration.workload,
            &context,
        )?);
        if mode != Some(crate::tape::Mode::Replay) {
            inputs.extend(crate::providers::runtime_inputs(
                &entry.configuration.provider,
                &context,
            )?);
        }
        if let Some(settings) = entry
            .configuration
            .measurements
            .as_ref()
            .filter(|settings| settings.enabled)
        {
            let expanded = crate::collectors::expand_dependencies(&settings.collectors);
            for collector in crate::collectors::all() {
                if crate::collectors::selected_or_environment(collector.id(), &expanded) {
                    inputs.extend(collector.runtime_inputs(&context)?);
                }
            }
        }
    }
    if mode.is_some() {
        inputs.extend(crate::tape::runtime_inputs()?);
    }
    inputs.sort();
    inputs.dedup();
    Ok(inputs)
}

pub(crate) fn discover(python: &Path, dependencies: &[Dependency]) -> Result<Vec<PathBuf>, Error> {
    let mut dependencies = dependencies.to_vec();
    dependencies.sort();
    dependencies.dedup();
    if dependencies.len() > 4096 {
        return Err(Error::Input(
            "runtime input declaration exceeds its component bound".into(),
        ));
    }
    // Load the same production ELF inspection code used by guest staging.
    // Its execution module is pure definitions; discovery starts no workload.
    let modules = [
        (
            "asb_execution",
            include_str!("../assets/benchmarks/asb_execution.py"),
        ),
        (
            "asb_runtime",
            include_str!("../assets/benchmarks/asb_runtime.py"),
        ),
    ];
    let modules = serde_json::to_string(&modules).map_err(|_| unavailable())?;
    let source = format!(
        "import sys,types,json\nfor name,source in json.loads({}):\n module=types.ModuleType(name)\n sys.modules[name]=module\n exec(source,module.__dict__)\n{}",
        serde_json::to_string(&modules).map_err(|_| unavailable())?,
        include_str!("../assets/benchmarks/asb_runtime_inputs.py")
    );
    let request = serde_json::to_vec(&dependencies).map_err(|_| unavailable())?;
    if request.len() > 1024 * 1024 {
        return Err(unavailable());
    }
    let mut command = Command::new(python);
    command
        .args(["-I", "-S", "-B", "-c"])
        .arg(source)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn().map_err(|_| unavailable())?;
    // Drain the bounded response concurrently so a legitimate manifest cannot
    // deadlock a pipe while the parent waits for discovery to exit.
    let output = child.stdout.take().expect("runtime discovery stdout");
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        output
            .take(4 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let mut input = child.stdin.take().expect("runtime discovery stdin");
    let sender = std::thread::spawn(move || input.write_all(&request));
    let deadline = Instant::now() + Duration::from_secs(60);
    let success = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => break false,
        }
    };
    if !success {
        stop_owned(&mut child);
    }
    let _ = child.wait();
    let sent = sender.join().map_err(|_| unavailable())?.is_ok();
    let bytes = reader
        .join()
        .map_err(|_| unavailable())?
        .map_err(|_| unavailable())?;
    if !success || !sent || bytes.len() > 4 * 1024 * 1024 {
        return Err(unavailable());
    }
    let result: Discovered = serde_json::from_slice(&bytes).map_err(|_| unavailable())?;
    if result.schema_version != 1
        || result.paths.len() > 100000
        || result.paths.iter().any(|path| !path.is_absolute())
    {
        return Err(unavailable());
    }
    Ok(result.paths)
}

fn unavailable() -> Error {
    Error::Input("runtime inputs could not be fully discovered within their limits; no input values were logged".into())
}

fn stop_owned(child: &mut std::process::Child) {
    if !matches!(child.try_wait(), Ok(None)) {
        return;
    }
    // The unreaped child is the leader of the new group created above. Its
    // PID cannot be reused while this handle is retained. Never signal a host
    // service, inherited group, arbitrary PID or unrelated benchmark worker.
    #[cfg(unix)]
    let _ = Command::new("/bin/kill")
        .args(["-KILL", "--", &format!("-{}", child.id())])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = child.kill();
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let root = std::env::temp_dir()
                .join(crate::workloads::TaskIdentity::local("runtime-input-test", None).attempt_id);
            std::fs::create_dir(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn discovery_binds_real_interpreters_libraries_and_missing_files_without_running_apps() {
        use std::os::unix::fs::PermissionsExt;
        let root = Directory::new();
        let script = root.0.join("fixture-agent");
        let marker = root.0.join("must-not-run");
        std::fs::write(&script, format!("#!/bin/sh\ntouch {}\n", marker.display())).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let missing = root.0.join("missing-configuration");
        let paths = discover(
            Path::new("python3"),
            &[
                Dependency::Process {
                    pid: std::process::id(),
                },
                Dependency::Python {
                    path: "python3".into(),
                    site: false,
                },
                Dependency::program(&script),
                Dependency::file(&missing),
            ],
        )
        .unwrap();
        assert!(paths.contains(&script));
        assert!(paths.contains(&missing));
        assert!(
            paths
                .iter()
                .any(|path| path.file_name().is_some_and(|name| name == "libc.so.6"))
        );
        assert!(
            paths
                .iter()
                .any(|path| path.file_name().is_some_and(|name| name == "os.py"))
        );
        assert!(!marker.exists());
    }

    #[test]
    fn runtime_errors_never_echo_paths_or_interpreter_output() {
        let marker = "PRIVATE_RUNTIME_VALUE";
        let result = discover(
            Path::new("python3"),
            &[Dependency::program(format!("/missing/{marker}"))],
        );
        let error = result.unwrap_err().to_string();
        assert!(!error.contains(marker));
        assert!(error.contains("runtime inputs"));
    }

    #[test]
    fn cancellation_stops_only_the_owned_discovery_group() {
        use std::io::BufRead;
        use std::os::unix::process::CommandExt;
        struct ChildGuard {
            child: std::process::Child,
            group: bool,
        }
        impl Drop for ChildGuard {
            fn drop(&mut self) {
                if self.group {
                    stop_owned(&mut self.child);
                } else {
                    let _ = self.child.kill();
                }
                let _ = self.child.wait();
            }
        }
        let mut unrelated = ChildGuard {
            child: Command::new("sleep").arg("30").spawn().unwrap(),
            group: false,
        };
        let child = Command::new("python3").args(["-I","-S","-B","-c",
            "import subprocess,time; p=subprocess.Popen(['sleep','30']); print(p.pid,flush=True); time.sleep(30)"])
            .process_group(0).stdout(Stdio::piped()).spawn().unwrap();
        let mut owned = ChildGuard { child, group: true };
        let mut line = String::new();
        std::io::BufReader::new(owned.child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let descendant: u32 = line.trim().parse().unwrap();
        stop_owned(&mut owned.child);
        assert!(!owned.child.wait().unwrap().success());
        assert!(unrelated.child.try_wait().unwrap().is_none());
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let state = std::fs::read_to_string(format!("/proc/{descendant}/stat"))
                .ok()
                .and_then(|text| {
                    text.rsplit_once(") ")
                        .map(|(_, fields)| fields.chars().next().unwrap())
                });
            if state.is_none_or(|state| state == 'Z') {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "owned discovery descendant survived cancellation"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
