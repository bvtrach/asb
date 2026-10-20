//! Private, bounded JSON-lines transport to workload-owned execution engines.
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

const MAX_MESSAGE: u64 = 1024 * 1024;

pub(crate) fn publish_asset(path: &std::path::Path, source: &str) -> Result<(), crate::Error> {
    if std::fs::symlink_metadata(path).is_ok_and(|metadata| {
        if !metadata.is_file() || metadata.len() != source.len() as u64 {
            return false;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o077 != 0 {
                return false;
            }
        }
        std::fs::read(path).is_ok_and(|bytes| bytes == source.as_bytes())
    }) {
        return Ok(());
    }
    let temporary =
        path.with_extension(crate::workloads::TaskIdentity::local("helper", None).attempt_id);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|error| crate::Error::Io(error.to_string()))?;
    file.write_all(source.as_bytes())
        .and_then(|()| file.sync_all())
        .and_then(|()| std::fs::rename(&temporary, path))
        .map_err(|error| crate::Error::Io(error.to_string()))
}

pub(crate) fn publish_support(root: &std::path::Path) -> Result<(), crate::Error> {
    for (name, source) in [
        (
            "asb_resources.py",
            include_str!("../../assets/benchmarks/asb_resources.py"),
        ),
        (
            "asb_docker.py",
            include_str!("../../assets/benchmarks/asb_docker.py"),
        ),
        (
            "asb_execution.py",
            include_str!("../../assets/benchmarks/asb_execution.py"),
        ),
        (
            "asb_task_server.py",
            include_str!("../../assets/benchmarks/asb_task_server.py"),
        ),
        (
            "asb_guest.py",
            include_str!("../../assets/benchmarks/asb_guest.py"),
        ),
        (
            "asb_runtime.py",
            include_str!("../../assets/benchmarks/asb_runtime.py"),
        ),
        (
            "asb_relay.py",
            include_str!("../../assets/benchmarks/asb_relay.py"),
        ),
    ] {
        publish_asset(&root.join(name), source)?;
    }
    Ok(())
}

pub(crate) fn decode_base64(value: &Value) -> Result<Vec<u8>, String> {
    let bytes = value
        .as_str()
        .ok_or("controller output is not encoded bytes")?
        .as_bytes();
    if bytes.len() % 4 != 0 || bytes.len() as u64 > MAX_MESSAGE {
        return Err("invalid bounded output encoding".into());
    }
    let digit = |byte| match byte {
        b'A'..=b'Z' => Ok(byte - b'A'),
        b'a'..=b'z' => Ok(byte - b'a' + 26),
        b'0'..=b'9' => Ok(byte - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err("invalid output encoding".to_owned()),
    };
    let mut result = Vec::with_capacity(bytes.len() / 4 * 3);
    for (index, chunk) in bytes.chunks_exact(4).enumerate() {
        let a = digit(chunk[0])?;
        let b = digit(chunk[1])?;
        let c = if chunk[2] == b'=' {
            0
        } else {
            digit(chunk[2])?
        };
        let d = if chunk[3] == b'=' {
            0
        } else {
            digit(chunk[3])?
        };
        if (chunk[2] == b'=' && (chunk[3] != b'=' || b & 15 != 0))
            || (chunk[3] == b'=' && chunk[2] != b'=' && c & 3 != 0)
            || ((chunk[2] == b'=' || chunk[3] == b'=') && (index + 1) * 4 != bytes.len())
        {
            return Err("invalid padded output encoding".into());
        }
        result.push((a << 2) | (b >> 4));
        if chunk[2] != b'=' {
            result.push((b << 4) | (c >> 2));
        }
        if chunk[3] != b'=' {
            result.push((c << 6) | d);
        }
    }
    Ok(result)
}

pub(crate) struct Controller {
    child: Child,
    input: Option<ChildStdin>,
    events: Option<Receiver<Result<Value, String>>>,
}

impl Controller {
    pub(crate) fn spawn(command: &mut Command) -> Result<Self, String> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|error| format!("cannot start workload controller: {error}"))?;
        let input = child.stdin.take();
        let stdout = child.stdout.take().expect("piped controller stdout");
        let (sender, events) = mpsc::sync_channel(8);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = Vec::new();
                let result = reader
                    .by_ref()
                    .take(MAX_MESSAGE + 1)
                    .read_until(b'\n', &mut line);
                let event = match result {
                    Ok(0) => break,
                    Ok(_) if line.len() as u64 > MAX_MESSAGE || !line.ends_with(b"\n") => {
                        Err("workload controller message exceeded its framing limit".into())
                    }
                    Ok(_) => serde_json::from_slice(&line)
                        .map_err(|_| "workload controller returned invalid JSON".into()),
                    Err(_) => Err("cannot read workload controller response".into()),
                };
                let failed = event.is_err();
                if sender.send(event).is_err() || failed {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            input,
            events: Some(events),
        })
    }

    pub(crate) fn send(&mut self, request: &Value) -> Result<(), String> {
        let mut bytes =
            serde_json::to_vec(request).map_err(|_| "cannot encode workload request")?;
        if bytes.len() as u64 >= MAX_MESSAGE {
            return Err("workload request exceeds 1 MiB".into());
        }
        bytes.push(b'\n');
        self.input
            .as_mut()
            .ok_or("workload controller is closed")?
            .write_all(&bytes)
            .map_err(|_| "cannot send private workload request".to_owned())
    }

    pub(crate) fn receive(&mut self, deadline: Instant) -> Result<Value, String> {
        let event = self.receive_raw(deadline)?;
        if event.get("event").and_then(Value::as_str) == Some("error") {
            // ExecutionError reasons are fixed local diagnostics from our
            // engine, not arbitrary SDK HTTP bodies or request dumps.
            if event["error_class"] == "ExecutionError"
                && let Some(reason) = event["reason"]
                    .as_str()
                    .filter(|reason| reason.len() <= 512 && !reason.contains(['\n', '\r']))
            {
                return Err(format!("workload execution failed: {reason}"));
            }
            return Err("workload controller could not complete the operation; inspect the private task artifacts".into());
        }
        Ok(event)
    }

    fn receive_raw(&mut self, deadline: Instant) -> Result<Value, String> {
        let event = self
            .events
            .as_ref()
            .ok_or("workload controller is closed")?
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .map_err(|error| match error {
                mpsc::RecvTimeoutError::Timeout => {
                    "workload controller exceeded its response deadline".to_owned()
                }
                mpsc::RecvTimeoutError::Disconnected => {
                    "workload controller exited before completing its operation".to_owned()
                }
            })??;
        Ok(event)
    }

    /// EOF interrupts active work; continue draining its bounded event stream
    /// until the owner confirms teardown. Cancellation's nonzero child exit
    /// is expected, but it never substitutes for resource-removal evidence.
    pub(crate) fn cancel(&mut self, timeout: Duration) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        self.input.take();
        loop {
            if Instant::now() >= deadline {
                return Err("workload cancellation exceeded its cleanup deadline".into());
            }
            let event = self.receive_raw(deadline)?;
            if event["event"] == "closed" {
                if !matches!(event["cleanup"].as_str(), Some("removed" | "not_created")) {
                    return Err("cancelled workload resource cleanup did not confirm removal; inspect private task cleanup artifacts".into());
                }
                return self.wait_for_exit(deadline, false);
            }
            // A command already in flight can produce output, an error or a
            // final result before the input reader observes EOF. No payload
            // from those discarded events is echoed to human diagnostics.
        }
    }

    pub(crate) fn request(
        &mut self,
        request: Value,
        expected: &str,
        timeout: Duration,
    ) -> Result<Value, String> {
        self.send(&request)?;
        let response = self.receive(Instant::now() + timeout)?;
        if response.get("event").and_then(Value::as_str) != Some(expected) {
            return Err("workload controller returned an unexpected protocol event".into());
        }
        Ok(response)
    }

    pub(crate) fn finish(&mut self, timeout: Duration) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        self.send(&serde_json::json!({"op":"close"}))?;
        // Keep stdin open until cleanup acknowledges completion. EOF is
        // cancellation and is only the fallback for a failed caller.
        let response = self.receive(deadline)?;
        if response["event"] != "closed" || response["cleanup"] != "removed" {
            return Err("workload resource cleanup did not confirm removal; inspect private task cleanup artifacts".into());
        }
        self.input.take();
        self.wait_for_exit(deadline, true)
    }

    fn wait_for_exit(&mut self, deadline: Instant, require_success: bool) -> Result<(), String> {
        loop {
            if let Some(status) = self
                .child
                .try_wait()
                .map_err(|_| "cannot check completed workload controller")?
            {
                return if status.success() || !require_success {
                    Ok(())
                } else {
                    Err("workload controller failed after resource cleanup".into())
                };
            }
            if Instant::now() >= deadline {
                return Err("workload controller did not exit after resource cleanup".into());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        // EOF is cancellation, including while a command/evaluator is active.
        self.input.take();
        self.events.take();
        let started = Instant::now();
        loop {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            if started.elapsed() > Duration::from_secs(3) {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        #[cfg(unix)]
        {
            // Signal only this owned child, never a process-name match/group.
            let _ = Command::new("kill")
                .args(["-TERM", &self.child.id().to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        while started.elapsed() < Duration::from_secs(15) {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    #[test]
    fn helper_publication_is_private_idempotent_and_does_not_follow_destination_symlinks() {
        use std::os::unix::fs::MetadataExt;
        let root = std::env::temp_dir()
            .join(crate::workloads::TaskIdentity::local("helper-publication", None).attempt_id);
        std::fs::create_dir(&root).unwrap();
        let file = root.join("helper.py");
        super::publish_asset(&file, "first").unwrap();
        let before = std::fs::metadata(&file).unwrap().ino();
        super::publish_asset(&file, "first").unwrap();
        assert_eq!(std::fs::metadata(&file).unwrap().ino(), before);
        super::publish_asset(&file, "second").unwrap();
        assert_ne!(std::fs::metadata(&file).unwrap().ino(), before);
        let other = root.join("other");
        std::fs::write(&other, "untouched").unwrap();
        std::fs::remove_file(&file).unwrap();
        std::os::unix::fs::symlink(&other, &file).unwrap();
        super::publish_asset(&file, "second").unwrap();
        assert_eq!(std::fs::read_to_string(other).unwrap(), "untouched");
        assert!(!file.is_symlink());
        std::fs::remove_dir_all(root).unwrap();
    }
    use super::*;
    use serde_json::json;

    #[test]
    fn explicit_cleanup_waits_for_confirmation_and_exit_beyond_drop_grace() {
        let mut command = Command::new("python3");
        command.args(["-u", "-c", "import json,sys,time\nprint(json.dumps(dict(event='ready')),flush=True)\nrequest=json.loads(sys.stdin.readline())\nassert request['op']=='close'\ntime.sleep(3.2)\nprint(json.dumps(dict(event='closed',cleanup='removed')),flush=True)\n"]);
        let mut controller = Controller::spawn(&mut command).unwrap();
        controller
            .receive(Instant::now() + Duration::from_secs(3))
            .unwrap();
        controller.finish(Duration::from_secs(8)).unwrap();
        assert!(controller.child.try_wait().unwrap().unwrap().success());
    }

    #[test]
    fn cleanup_failure_missing_ack_and_failed_controller_exit_are_not_success() {
        for (event, cleanup, code) in [
            ("closed", "cleanup_failed", 0),
            ("closed", "not_created", 0),
            ("completed", "removed", 0),
            ("closed", "removed", 1),
        ] {
            let script = format!(
                "import json,sys\nprint(json.dumps(dict(event='ready')),flush=True)\nsys.stdin.readline()\nprint(json.dumps(dict(event='{event}',cleanup='{cleanup}')),flush=True)\nsys.exit({code})\n"
            );
            let mut command = Command::new("python3");
            command.args(["-u", "-c", &script]);
            let mut controller = Controller::spawn(&mut command).unwrap();
            controller
                .receive(Instant::now() + Duration::from_secs(3))
                .unwrap();
            assert!(controller.finish(Duration::from_secs(3)).is_err());
        }
    }

    #[test]
    fn cancellation_drains_inflight_events_and_waits_for_owned_cleanup() {
        let mut command = Command::new("python3");
        command.args(["-u", "-c", "import json,sys,time\nprint(json.dumps(dict(event='ready')),flush=True)\nassert sys.stdin.read()==''\nfor event in ['output','execution_ended','completed','error']:\n print(json.dumps(dict(event=event,private='never print this')),flush=True)\ntime.sleep(3.2)\nprint(json.dumps(dict(event='closed',cleanup='removed')),flush=True)\nsys.exit(1)\n"]);
        let mut controller = Controller::spawn(&mut command).unwrap();
        controller
            .receive(Instant::now() + Duration::from_secs(3))
            .unwrap();
        controller.cancel(Duration::from_secs(8)).unwrap();
        assert!(!controller.child.try_wait().unwrap().unwrap().success());
    }

    #[test]
    fn cancelled_startup_without_a_created_resource_can_be_confirmed() {
        let mut command = Command::new("python3");
        command.args(["-u", "-c", "import json,sys\nprint(json.dumps(dict(event='error')),flush=True)\nassert sys.stdin.read()==''\nprint(json.dumps(dict(event='closed',cleanup='not_created')),flush=True)\nsys.exit(1)\n"]);
        let mut controller = Controller::spawn(&mut command).unwrap();
        assert!(
            controller
                .receive(Instant::now() + Duration::from_secs(3))
                .is_err()
        );
        controller.cancel(Duration::from_secs(3)).unwrap();
    }

    #[test]
    fn cancellation_never_confuses_child_exit_with_resource_removal() {
        for cleanup in [
            None,
            Some("pending"),
            Some("ownership_mismatch"),
            Some("error"),
        ] {
            let ack = cleanup.map_or(String::new(), |cleanup| {
                format!("print(json.dumps(dict(event='closed',cleanup='{cleanup}')),flush=True)\n")
            });
            let script = format!(
                "import json,sys\nprint(json.dumps(dict(event='ready')),flush=True)\nassert sys.stdin.read()==''\n{ack}"
            );
            let mut command = Command::new("python3");
            command.args(["-u", "-c", &script]);
            let mut controller = Controller::spawn(&mut command).unwrap();
            controller
                .receive(Instant::now() + Duration::from_secs(3))
                .unwrap();
            assert!(controller.cancel(Duration::from_secs(3)).is_err());
        }
    }

    #[test]
    fn cancellation_drains_until_deadline_not_an_unbounded_event_stream() {
        let mut command = Command::new("python3");
        command.args(["-u", "-c", "import json,sys,time\nprint(json.dumps(dict(event='ready')),flush=True)\nassert sys.stdin.read()==''\nfor i in range(30):\n print(json.dumps(dict(event='output')),flush=True)\n time.sleep(.01)\n"]);
        let mut controller = Controller::spawn(&mut command).unwrap();
        controller
            .receive(Instant::now() + Duration::from_secs(3))
            .unwrap();
        let started = Instant::now();
        assert!(controller.cancel(Duration::from_millis(50)).is_err());
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn output_encoding_preserves_binary_and_rejects_noncanonical_padding() {
        for (encoded, expected) in [
            ("", &b""[..]),
            ("YQ==", &b"a"[..]),
            ("YWI=", &b"ab"[..]),
            ("YWJj", &b"abc"[..]),
            ("AP+A", &[0, 255, 128][..]),
        ] {
            assert_eq!(decode_base64(&json!(encoded)).unwrap(), expected);
        }
        for invalid in [
            "a", "====", "YQ=x", "YQ==YQ==", "YR==", "YWJ=", "Y WJ", "!!!!",
        ] {
            assert!(decode_base64(&json!(invalid)).is_err(), "{invalid}");
        }
    }

    #[test]
    fn private_requests_round_trip_and_eof_reaps_the_owned_controller() {
        let mut command = Command::new("python3");
        command.args(["-u", "-c", "import json,sys\nprint(json.dumps({'event':'ready'}),flush=True)\nfor line in sys.stdin:\n request=json.loads(line)\n print(json.dumps({'event':'completed','value':request['value']}),flush=True)\n"]);
        let mut controller = Controller::spawn(&mut command).unwrap();
        assert_eq!(
            controller
                .receive(Instant::now() + Duration::from_secs(5))
                .unwrap()["event"],
            "ready"
        );
        let response = controller
            .request(
                json!({"value":"private fixture"}),
                "completed",
                Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(response["value"], "private fixture");
        let pid = controller.child.id();
        drop(controller);
        #[cfg(target_os = "linux")]
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    }

    #[test]
    fn malformed_or_oversized_messages_do_not_echo_private_payloads() {
        for body in [
            "print('private fixture',flush=True)",
            "print('x'*1048577,flush=True)",
        ] {
            let mut command = Command::new("python3");
            command.args(["-u", "-c", body]);
            let mut controller = Controller::spawn(&mut command).unwrap();
            let error = controller
                .receive(Instant::now() + Duration::from_secs(5))
                .unwrap_err();
            assert!(!error.contains("private fixture"));
            assert!(error.contains("JSON") || error.contains("framing"));
        }
    }
}
