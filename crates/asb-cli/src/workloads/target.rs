//! Verify isolated daemon-host PIDs against local kernel/process evidence.
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Identity {
    boot_id: String,
    namespace_pid: u32,
    start_time_ticks: u64,
}

#[derive(Deserialize)]
struct Target {
    container_id: String,
    exec_id: String,
    daemon_pid: u32,
    namespace_pid: u32,
    pid_scope: String,
    process_identity: Identity,
}

#[derive(Debug)]
pub(crate) struct LocalTarget {
    pub pid: u32,
    pub namespace_pid: u32,
    pub start_time_ticks: u64,
    pub cgroup: PathBuf,
}

fn read(path: &Path) -> Result<String, String> {
    let mut text = String::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(65537).read_to_string(&mut text))
        .map_err(|_| "cannot read isolated process identity; check local /proc access")?;
    if text.len() > 65536 {
        return Err("isolated process identity exceeds its bound".into());
    }
    Ok(text)
}

fn start_time(stat: &str) -> Result<u64, String> {
    // comm may contain parentheses/spaces. After the final ')', state is
    // field 3 and starttime is field 22.
    stat.rsplit_once(')')
        .and_then(|(_, fields)| fields.split_whitespace().nth(19))
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| "invalid isolated process start time".into())
}

fn snapshot(proc: &Path, pid: u32) -> Result<Identity, String> {
    if pid <= 1 {
        return Err("isolated process must be distinct from init".into());
    }
    Ok(Identity {
        boot_id: read(&proc.join("sys/kernel/random/boot_id"))?.trim().into(),
        namespace_pid: pid,
        start_time_ticks: start_time(&read(&proc.join(pid.to_string()).join("stat"))?)?,
    })
}

fn verify_at(value: &serde_json::Value, proc: &Path) -> Result<LocalTarget, String> {
    let target: Target = serde_json::from_value(value.clone())
        .map_err(|_| "isolated measurement target has invalid identity fields")?;
    let identity = &target.process_identity;
    let hexadecimal =
        |text: &str| text.len() == 64 && text.bytes().all(|byte| byte.is_ascii_hexdigit());
    let uuid = |text: &str| {
        text.len() == 36
            && text.bytes().enumerate().all(|(index, byte)| {
                if matches!(index, 8 | 13 | 18 | 23) {
                    byte == b'-'
                } else {
                    byte.is_ascii_hexdigit()
                }
            })
    };
    if target.pid_scope != "docker-daemon-host"
        || !hexadecimal(&target.container_id)
        || !hexadecimal(&target.exec_id)
        || target.daemon_pid <= 1
        || target.namespace_pid <= 1
        || identity.namespace_pid != target.namespace_pid
        || !uuid(&identity.boot_id)
        || read(&proc.join("sys/kernel/random/boot_id"))?.trim() != identity.boot_id
    {
        return Err("isolated target is not proven to belong to this host; remote Docker measurement is not supported".into());
    }
    let process = proc.join(target.daemon_pid.to_string());
    let before = start_time(&read(&process.join("stat"))?)?;
    let status = read(&process.join("status"))?;
    let pids = status
        .lines()
        .find_map(|line| line.strip_prefix("NSpid:"))
        .ok_or("isolated target lacks namespace PID evidence")?
        .split_whitespace()
        .map(str::parse::<u32>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "isolated target has invalid namespace PID evidence")?;
    if before != identity.start_time_ticks
        || pids.len() < 2
        || pids.first() != Some(&target.daemon_pid)
        || pids.last() != Some(&target.namespace_pid)
        || !status
            .lines()
            .any(|line| line.starts_with("State:") && line.split_whitespace().nth(1) == Some("T"))
    {
        return Err(
            "isolated target is stale, not stopped, or does not match this host PID namespace"
                .into(),
        );
    }
    let membership = read(&process.join("cgroup"))?;
    let relative = membership
        .lines()
        .find_map(|line| line.strip_prefix("0::"))
        .ok_or("isolated measurement requires identifiable cgroup-v2 membership")?;
    if !relative.starts_with('/')
        || relative.split('/').any(|part| matches!(part, "." | ".."))
        || !relative.split('/').any(|part| {
            part == target.container_id || part == format!("docker-{}.scope", target.container_id)
        })
        || start_time(&read(&process.join("stat"))?)? != before
    {
        return Err("isolated target does not match its owned container cgroup or changed during verification".into());
    }
    Ok(LocalTarget {
        pid: target.daemon_pid,
        namespace_pid: target.namespace_pid,
        start_time_ticks: before,
        cgroup: Path::new("/sys/fs/cgroup").join(relative.trim_start_matches('/')),
    })
}

pub(crate) fn verify(value: &serde_json::Value) -> Result<LocalTarget, String> {
    verify_at(value, Path::new("/proc"))
}

pub(crate) fn entry(args: &[std::ffi::OsString]) -> std::process::ExitCode {
    let result = args
        .first()
        .filter(|_| args.len() == 1)
        .and_then(|value| value.to_str())
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| "invalid isolated process identity request".into())
        .and_then(|pid| snapshot(Path::new("/proc"), pid));
    match result
        .and_then(|identity| serde_json::to_string(&identity).map_err(|error| error.to_string()))
    {
        Ok(value) => {
            println!("{value}");
            std::process::ExitCode::SUCCESS
        }
        Err(_) => {
            eprintln!("cannot read isolated process identity");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn local_target_needs_boot_namespace_start_time_stopped_gate_and_owned_cgroup() {
        let root =
            std::env::temp_dir().join(super::super::TaskIdentity::local("target", None).attempt_id);
        std::fs::create_dir_all(root.join("sys/kernel/random")).unwrap();
        std::fs::create_dir_all(root.join("9001")).unwrap();
        let boot = "01234567-89ab-cdef-0123-456789abcdef";
        std::fs::write(root.join("sys/kernel/random/boot_id"), boot).unwrap();
        let mut fields = vec!["0"; 20];
        fields[0] = "T";
        fields[19] = "123456";
        std::fs::write(
            root.join("9001/stat"),
            format!("9001 (name with ) parentheses) {}", fields.join(" ")),
        )
        .unwrap();
        std::fs::write(root.join("9001/status"), "State: T\nNSpid: 9001 42\n").unwrap();
        std::fs::write(
            root.join("9001/cgroup"),
            format!("0::/system.slice/docker-{}.scope\n", "a".repeat(64)),
        )
        .unwrap();
        let value = json!({"container_id":"a".repeat(64),"exec_id":"b".repeat(64),"daemon_pid":9001,
            "namespace_pid":42,"pid_scope":"docker-daemon-host",
            "process_identity":{"boot_id":boot,"namespace_pid":42,"start_time_ticks":123456}});
        let target = verify_at(&value, &root).unwrap();
        assert_eq!(
            (target.pid, target.namespace_pid, target.start_time_ticks),
            (9001, 42, 123456)
        );
        assert!(target.cgroup.to_string_lossy().contains(&"a".repeat(64)));
        for (field, replacement) in [
            ("daemon_pid", json!(0)),
            ("namespace_pid", json!(43)),
            ("pid_scope", json!("local")),
            ("container_id", json!("../escape")),
            ("exec_id", json!("wrong")),
        ] {
            let mut changed = value.clone();
            changed[field] = replacement;
            assert!(verify_at(&changed, &root).is_err(), "{field}");
        }
        for (field, replacement) in [
            ("boot_id", json!("11111111-1111-1111-1111-111111111111")),
            ("namespace_pid", json!(43)),
            ("start_time_ticks", json!(123455)),
        ] {
            let mut changed = value.clone();
            changed["process_identity"][field] = replacement;
            assert!(verify_at(&changed, &root).is_err(), "{field}");
        }
        for status in [
            "State: R\nNSpid: 9001 42\n",
            "State: T\nNSpid: 9002 42\n",
            "State: T\nNSpid: 9001 43\n",
            "State: T\nNSpid: 42\n",
            "State: T\n",
        ] {
            std::fs::write(root.join("9001/status"), status).unwrap();
            assert!(verify_at(&value, &root).is_err());
        }
        std::fs::write(root.join("9001/status"), "State: T\nNSpid: 9001 42\n").unwrap();
        for path in ["/system.slice/unrelated.scope", "/", "/../escape"] {
            std::fs::write(root.join("9001/cgroup"), format!("0::{path}\n")).unwrap();
            assert!(verify_at(&value, &root).is_err());
        }
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn stat_parser_rejects_partial_or_zero_start_times() {
        assert!(start_time("1 (partial)").is_err());
        assert!(start_time(&format!("1 (zero) {}", ["0"; 20].join(" "))).is_err());
    }
}
