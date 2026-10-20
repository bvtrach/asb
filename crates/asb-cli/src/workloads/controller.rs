//! Private, bounded JSON-lines transport to workload-owned execution engines.
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

const MAX_MESSAGE: u64 = 1024 * 1024;

pub(crate) fn publish_asset(path: &std::path::Path, source: &str) -> Result<(), crate::Error> {
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
    use super::*;
    use serde_json::json;

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
