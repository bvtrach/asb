//! Optional accounting in an exclusive, delegated cgroup-v2 leaf. The process
//! gate is moved before it starts the agent; all descendants inherit the leaf.
use crate::collectors::{CollectorAdapter, PreflightContext};
use crate::measurements::{self, Session};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub(super) struct CpuAccounting;
pub(super) static ADAPTER: CpuAccounting = CpuAccounting;

#[derive(Default)]
struct State {
    leaf: Option<Leaf>,
    reason: Option<String>,
}

struct Leaf {
    path: PathBuf,
}

impl Drop for Leaf {
    fn drop(&mut self) {
        // Never delete recursively, kill tasks, or move a surviving process.
        let _ = fs::remove_dir(&self.path);
    }
}

impl CollectorAdapter for CpuAccounting {
    fn id(&self) -> &'static str {
        "cpu-accounting"
    }
    fn environment_flag(&self) -> Option<&'static str> {
        None
    }
    fn tool_id(&self) -> Option<&'static str> {
        None
    }
    fn requires_pid(&self) -> bool {
        true
    }
    fn preflight(&self, _: &mut PreflightContext<'_>) -> Result<(), String> {
        Ok(())
    }

    fn attach_pid(&self, session: &mut Session, pid: u32) -> Result<(), String> {
        let state = session.state::<State>();
        let mut state = state.borrow_mut();
        match attach(pid, session.cgroup_parent()) {
            Ok(leaf) => state.leaf = Some(leaf),
            Err(reason) => state.reason = Some(reason),
        }
        Ok(())
    }

    fn finish(&self, session: &mut Session) -> Result<(), String> {
        let state = session.state::<State>();
        let mut state = state.borrow_mut();
        let mut values = None;
        let mut source = "proc-sampling";
        let mut cleanup = "not_created";
        if let Some(leaf) = state.leaf.take() {
            let populated = fs::read_to_string(leaf.path.join("cgroup.events"))
                .is_ok_and(|text| text.lines().any(|line| line.trim() == "populated 0"));
            match read_cpu_stat(&leaf.path.join("cpu.stat")) {
                Ok(_) if !populated => {
                    state.reason = Some("agent accounting cgroup still has live tasks or unreadable membership; final CPU totals are unavailable".into());
                }
                Ok(cpu) => {
                    session.record_cpu_accounting(cpu.0, cpu.1, "cgroup-v2");
                    values = Some(cpu);
                    source = "cgroup-v2";
                }
                Err(error) => state.reason = Some(error),
            }
            cleanup = if fs::remove_dir(&leaf.path).is_ok() {
                "removed"
            } else {
                "retained_busy_or_inaccessible"
            };
        }
        if values.is_none()
            && let Some((user_ms, system_ms)) = session.sampled_cpu_accounting()
        {
            values = Some(((user_ms * 1_000.0) as u64, (system_ms * 1_000.0) as u64));
        }
        measurements::write_json(
            &session.dir.join("cpu-accounting.json"),
            &serde_json::json!({
                "schema_version": 1, "collector": "cpu-accounting",
                "status": if values.is_some() { "completed" } else { "unavailable" },
                "source": source,
                "reason": state.reason,
                "remediation": if source == "proc-sampling" { Some("Run ASB inside a delegated cgroup-v2 scope or set ASB_CGROUP_PARENT to an existing writable delegated cgroup; process-tree sampling was used as the fallback.") } else { None },
                "cleanup": cleanup,
                "metrics": values.map(|(user, system)| serde_json::json!({"cpu_user_ms": user as f64 / 1000.0, "cpu_system_ms": system as f64 / 1000.0})),
            }),
        )
    }
}

fn read_cpu_stat(path: &Path) -> Result<(u64, u64), String> {
    let mut text = String::new();
    fs::File::open(path)
        .and_then(|f| f.take(4097).read_to_string(&mut text))
        .map_err(|e| format!("cannot read cgroup CPU accounting: {e}"))?;
    if text.len() > 4096 {
        return Err("cgroup CPU accounting exceeds 4096 bytes".into());
    }
    parse_cpu_stat(&text)
        .ok_or_else(|| "cgroup CPU accounting lacks valid user_usec/system_usec counters".into())
}

fn parse_cpu_stat(text: &str) -> Option<(u64, u64)> {
    let mut user = None;
    let mut system = None;
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        match fields.next()? {
            "user_usec" => {
                if user.is_some() {
                    return None;
                }
                user = Some(fields.next()?.parse().ok()?);
            }
            "system_usec" => {
                if system.is_some() {
                    return None;
                }
                system = Some(fields.next()?.parse().ok()?);
            }
            _ => {}
        }
    }
    Some((user?, system?))
}

fn attach(pid: u32, configured_parent: Option<&Path>) -> Result<Leaf, String> {
    if !cfg!(target_os = "linux") {
        return Err("cgroup-v2 accounting requires Linux".into());
    }
    let root = Path::new("/sys/fs/cgroup");
    let parent = if let Some(parent) = configured_parent {
        parent.to_path_buf()
    } else if let Some(parent) = std::env::var_os("ASB_CGROUP_PARENT") {
        PathBuf::from(parent)
    } else {
        let membership = fs::read_to_string("/proc/self/cgroup").map_err(|e| e.to_string())?;
        let relative = membership
            .lines()
            .find_map(|line| line.strip_prefix("0::"))
            .ok_or("no unified cgroup-v2 membership")?;
        root.join(relative.trim_start_matches('/'))
    };
    let parent =
        fs::canonicalize(parent).map_err(|e| format!("cannot resolve delegated cgroup: {e}"))?;
    if !parent.starts_with(root) || !parent.join("cgroup.controllers").is_file() {
        return Err(
            "ASB_CGROUP_PARENT must identify a cgroup-v2 directory under /sys/fs/cgroup".into(),
        );
    }
    // An unpaused process may already have escaped descendants. Only the
    // runner's direct, stopped gate child can establish complete accounting.
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let status =
            fs::read_to_string(format!("/proc/{pid}/status")).map_err(|e| e.to_string())?;
        let ppid = status
            .lines()
            .find_map(|s| s.strip_prefix("PPid:"))
            .and_then(|s| s.trim().parse::<u32>().ok());
        if ppid != Some(std::process::id()) {
            return Err("CPU accounting requires ASB's direct process gate child".into());
        }
        if status
            .lines()
            .any(|s| s.starts_with("State:") && s.split_whitespace().nth(1) == Some("T"))
        {
            break;
        }
        if Instant::now() >= deadline {
            return Err("CPU accounting gate was not stopped before agent execution".into());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let path = parent.join(format!("asb-{}-{pid}-{nonce}", std::process::id()));
    fs::create_dir(&path)
        .map_err(|e| format!("cannot create delegated CPU accounting cgroup: {e}"))?;
    let leaf = Leaf { path };
    if read_cpu_stat(&leaf.path.join("cpu.stat"))? != (0, 0) {
        return Err("new accounting cgroup is not empty".into());
    }
    OpenOptions::new()
        .write(true)
        .open(leaf.path.join("cgroup.procs"))
        .and_then(|mut file| file.write_all(format!("{pid}\n").as_bytes()))
        .map_err(|e| {
            format!("cannot attach the stopped agent gate to its accounting cgroup: {e}")
        })?;
    Ok(leaf)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cpu_stat_rejects_missing_duplicate_and_invalid_counters() {
        assert_eq!(
            parse_cpu_stat("usage_usec 15\nuser_usec 12\nsystem_usec 3\n"),
            Some((12, 3))
        );
        assert_eq!(parse_cpu_stat("user_usec 0\nsystem_usec 0\n"), Some((0, 0)));
        for text in [
            "user_usec 10",
            "user_usec x\nsystem_usec 1",
            "user_usec 1\nuser_usec 2\nsystem_usec 3",
        ] {
            assert_eq!(parse_cpu_stat(text), None);
        }
    }

    #[test]
    fn unavailable_accounting_keeps_sampled_values() {
        let dir = std::env::temp_dir().join(format!("asb-cgroup-fallback-{}", std::process::id()));
        let mut session = Session::start_with_collectors(&dir, &[], None).unwrap();
        session.state::<State>().borrow_mut().reason = Some("fixture: no delegation".into());
        ADAPTER.finish(&mut session).unwrap();
        let json: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.join("cpu-accounting.json")).unwrap()).unwrap();
        assert_eq!(json["status"], "unavailable");
        assert!(json["metrics"].is_null());
        assert_eq!(json["cleanup"], "not_created");
        drop(session);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn cleanup_never_removes_files_in_a_populated_directory() {
        let dir = std::env::temp_dir().join(format!("asb-cgroup-cleanup-{}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("fixture"), "retained").unwrap();
        drop(Leaf { path: dir.clone() });
        assert_eq!(fs::read_to_string(dir.join("fixture")).unwrap(), "retained");
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires a delegated cgroup-v2 scope and python3; no provider or host configuration changes"]
    fn live_cgroup_captures_short_lived_children_and_cleans_up() {
        use std::process::{Child, Command};
        struct Guard(Child);
        impl Drop for Guard {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let mut child = Guard(Command::new("sh").args(["-c", "kill -STOP $$; for i in 1 2 3; do python3 -c 'import time; end=time.process_time()+0.05; exec(\"while time.process_time() < end: pass\")' || exit 1; done"]).spawn().unwrap());
        let leaf = attach(child.0.id(), None)
            .expect("run this test under systemd-run --user --scope -p Delegate=yes");
        let path = leaf.path.clone();
        assert_eq!(read_cpu_stat(&path.join("cpu.stat")).unwrap(), (0, 0));
        assert!(
            Command::new("kill")
                .args(["-CONT", &child.0.id().to_string()])
                .status()
                .unwrap()
                .success()
        );
        assert!(child.0.wait().unwrap().success());
        let (user, system) = read_cpu_stat(&path.join("cpu.stat")).unwrap();
        assert!(
            user + system >= 100_000,
            "CPU accounting must retain exited children: {user} + {system}"
        );
        let deadline = Instant::now() + Duration::from_secs(2);
        while fs::remove_dir(&path).is_err() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!path.exists(), "accounting leaf must be cleaned up");
        drop(leaf);
    }
}
