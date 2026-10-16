//! Local llmtape proxy lifecycle for deterministic LLM recording and replay.

use crate::Error;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(8);
const BUILTIN_PROXY: &str = include_str!("../../../scripts/llmtape.mjs");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Record,
    Replay,
}

/// A running local llmtape server. Dropping it always attempts to stop it.
pub struct Session {
    child: Child,
    url: String,
}

impl Session {
    /// Start llmtape on an available loopback port and wait until it accepts connections.
    pub fn start(
        mode: Mode,
        cassette: &Path,
        upstream: Option<&str>,
        respect_record_timings: bool,
    ) -> Result<Self, Error> {
        if mode == Mode::Replay && !cassette.is_file() {
            return Err(Error::Input(format!(
                "replay cassette {} does not exist; run `asb run CONFIG --record --cassette {}` first",
                cassette.display(),
                cassette.display()
            )));
        }
        if mode == Mode::Record {
            let parent = cassette.parent().unwrap_or_else(|| Path::new("."));
            std::fs::create_dir_all(parent).map_err(|error| {
                Error::Run(format!(
                    "cannot create cassette directory {}: {error}",
                    parent.display()
                ))
            })?;
        }
        let port = free_port()?;
        let executable = resolve_executable(cassette)?;
        let mut command = executable.command();
        command
            .arg(match mode {
                Mode::Record => "record",
                Mode::Replay => "replay",
            })
            .arg("--port")
            .arg(port.to_string());
        match mode {
            Mode::Record => {
                let upstream = upstream.ok_or_else(|| {
                    Error::Run(
                        "record requires a provider API endpoint; set the provider base URL or use its configured local bridge".into(),
                    )
                })?;
                command.args(["--upstream", upstream, "--out"]);
                command.arg(cassette);
            }
            Mode::Replay => {
                command.args(["--cassette"]);
                command.arg(cassette);
                if respect_record_timings {
                    command.arg("--respect-record-timings");
                }
            }
        }
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let child = command.spawn().map_err(|error| {
            Error::Run(format!(
                "cannot start ASB's built-in tape proxy; install Node.js or set ASB_LLM_TAPE to an explicit compatible executable: {error}"
            ))
        })?;
        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
        let deadline = Instant::now() + STARTUP_TIMEOUT;
        while Instant::now() < deadline {
            if TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_ok() {
                return Ok(Self {
                    child,
                    url: format!("http://127.0.0.1:{port}/v1"),
                });
            }
            thread::sleep(Duration::from_millis(50));
        }
        let mut child = child;
        let _ = child.kill();
        let _ = child.wait();
        Err(Error::Run(
            "ASB's tape proxy did not become ready within 8 seconds; verify Node.js and the cassette format".into(),
        ))
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn stop(mut self) -> Result<(), Error> {
        let _ = self.child.kill();
        self.child
            .wait()
            .map(|_| ())
            .map_err(|error| Error::Run(format!("could not stop llmtape: {error}")))
    }
}

/// A tape proxy command. The default proxy is compiled into ASB and materialized
/// beside the cassette; no npm package or catalog installation is involved.
enum ProxyExecutable {
    Builtin(PathBuf),
    Override(String),
}

impl ProxyExecutable {
    fn command(&self) -> Command {
        match self {
            Self::Builtin(script) => {
                let mut command = Command::new("node");
                command.arg(script);
                command
            }
            Self::Override(executable) => Command::new(executable),
        }
    }
}

/// Resolve an explicitly supplied proxy or materialize ASB's built-in proxy.
/// ASB deliberately does not look up an unqualified host `llmtape` command.
fn resolve_executable(cassette: &Path) -> Result<ProxyExecutable, Error> {
    if let Some(override_path) = std::env::var("ASB_LLM_TAPE")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        return Ok(ProxyExecutable::Override(override_path));
    }
    let parent = cassette.parent().unwrap_or_else(|| Path::new("."));
    let script = parent.join(".asb-tape-proxy.mjs");
    std::fs::write(&script, BUILTIN_PROXY).map_err(|error| {
        Error::Run(format!(
            "cannot materialize ASB's built-in tape proxy at {}: {error}",
            script.display()
        ))
    })?;
    Ok(ProxyExecutable::Builtin(script))
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn free_port() -> Result<u16, Error> {
    TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .map(|listener| listener.local_addr().map(|address| address.port()))
        .map_err(|error| Error::Run(format!("cannot reserve a local llmtape port: {error}")))?
        .map_err(|error| Error::Run(format!("cannot inspect local llmtape port: {error}")))
}

#[cfg(test)]
mod tests {
    use super::{ProxyExecutable, resolve_executable};

    #[test]
    fn built_in_proxy_is_materialized_beside_the_cassette() {
        let root = std::env::temp_dir().join(format!("asb-tape-test-{}", std::process::id()));
        let cassette = root.join("cassettes/run.jsonl");
        std::fs::create_dir_all(cassette.parent().unwrap()).unwrap();
        let resolved = resolve_executable(&cassette).unwrap();
        match resolved {
            ProxyExecutable::Builtin(path) => {
                assert_eq!(path, root.join("cassettes/.asb-tape-proxy.mjs"));
                assert!(
                    std::fs::read_to_string(path)
                        .unwrap()
                        .contains("http.createServer")
                );
            }
            ProxyExecutable::Override(_) => panic!("expected the embedded tape proxy"),
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explicit_proxy_override_remains_supported() {
        // Environment changes are process-global, so exercise the command shape
        // directly rather than changing ASB_LLM_TAPE in a parallel test.
        let command = ProxyExecutable::Override("custom-tape".into()).command();
        assert_eq!(command.get_program(), "custom-tape");
    }
}
