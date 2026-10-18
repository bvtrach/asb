//! Local llmtape proxy lifecycle for deterministic LLM recording and replay.

use crate::Error;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(8);
const BUILTIN_PROXY: &str = include_str!("../../../scripts/llmtape.mjs");
static NEXT_SESSION: AtomicU64 = AtomicU64::new(0);
static START_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Record,
    Replay,
}

/// A proxy launch request shared by every adapter in a configured chain.
///
/// The request is intentionally provider-neutral: adapters only receive the
/// endpoint of the next hop and the tape lifecycle settings. Agent adapters
/// never see this type or any proxy-specific environment.
#[derive(Clone, Debug)]
pub(crate) struct ProxyRequest {
    pub(crate) mode: Mode,
    pub(crate) cassette: PathBuf,
    pub(crate) upstream: Option<String>,
    pub(crate) respect_record_timings: bool,
    pub(crate) workspace: PathBuf,
}

/// A proxy implementation owns one hop in the agent-to-provider path.
///
/// Adapters are started from the provider inward: the last configured adapter
/// receives the real provider endpoint, and each preceding adapter receives
/// the loopback endpoint of the hop after it. This makes a chain of zero, one,
/// or many proxies transparent to the agent.
pub(crate) trait ProxyAdapter: Sync {
    fn name(&self) -> &'static str;
    fn start(&self, request: &ProxyRequest) -> Result<RunningProxy, Error>;
}

/// A live process owned by one proxy adapter.
pub(crate) struct RunningProxy {
    child: Child,
    url: String,
    progress: Option<PathBuf>,
    _scratch: Scratch,
}

struct LlmtapeAdapter;

static LLMTAPE_ADAPTER: LlmtapeAdapter = LlmtapeAdapter;
static PROXY_ADAPTERS: &[&dyn ProxyAdapter] = &[&LLMTAPE_ADAPTER];

impl ProxyAdapter for LlmtapeAdapter {
    fn name(&self) -> &'static str {
        "llmtape"
    }

    fn start(&self, request: &ProxyRequest) -> Result<RunningProxy, Error> {
        let cassette = &request.cassette;
        if request.mode == Mode::Record {
            let parent = cassette
                .parent()
                .filter(|path| !path.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            std::fs::create_dir_all(parent).map_err(|error| {
                Error::Run(format!(
                    "cannot create cassette directory {}: {error}",
                    parent.display()
                ))
            })?;
        }
        let parent = cassette
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let scratch = Scratch::create(parent)?;
        // Every replay has its own copy and progress file. Sharing the source
        // directly would race on the proxy's cassette progress sidecar.
        let replay_cassette = scratch.path.join("cassette.jsonl");
        if request.mode == Mode::Replay {
            std::fs::copy(cassette, &replay_cassette).map_err(|error| {
                Error::Run(format!(
                    "cannot copy replay cassette {}: {error}",
                    cassette.display()
                ))
            })?;
        }
        // Hold the startup lock until the proxy is listening so sibling
        // replicas do not select the same released ephemeral port.
        let _start_guard = START_LOCK
            .lock()
            .map_err(|_| Error::Run("tape startup lock was poisoned".into()))?;
        let port = free_port()?;
        let executable = resolve_executable(&scratch.path)?;
        let mut command = executable.command();
        command
            .arg(match request.mode {
                Mode::Record => "record",
                Mode::Replay => "replay",
            })
            .arg("--port")
            .arg(port.to_string());
        match request.mode {
            Mode::Record => {
                let upstream = request.upstream.as_deref().ok_or_else(|| {
                    Error::Run(
                        "record requires a provider API endpoint; set the provider base URL or use its configured local bridge".into(),
                    )
                })?;
                command.args(["--upstream", upstream, "--out"]);
                command.arg(cassette);
            }
            Mode::Replay => {
                command.args(["--cassette"]);
                command.arg(&replay_cassette);
                if request.respect_record_timings {
                    command.arg("--respect-record-timings");
                }
            }
        }
        command
            .env("ASB_TAPE_WORKSPACE", &request.workspace)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = command.spawn().map_err(|error| {
            Error::Run(format!(
                "cannot start ASB's built-in tape proxy; install Node.js or set ASB_LLM_TAPE to an explicit compatible executable: {error}"
            ))
        })?;
        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
        let deadline = Instant::now() + STARTUP_TIMEOUT;
        while Instant::now() < deadline {
            if TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_ok() {
                return Ok(RunningProxy {
                    child,
                    url: format!("http://127.0.0.1:{port}/v1"),
                    progress: (request.mode == Mode::Replay)
                        .then(|| replay_cassette.with_extension("jsonl.progress.json")),
                    _scratch: scratch,
                });
            }
            if child
                .try_wait()
                .map_err(|error| Error::Run(format!("cannot inspect tape proxy: {error}")))?
                .is_some()
            {
                return Err(Error::Run("ASB's tape proxy exited before becoming ready; verify Node.js and the cassette format".into()));
            }
            thread::sleep(Duration::from_millis(50));
        }
        let _ = child.kill();
        let _ = child.wait();
        Err(Error::Run(
            "ASB's tape proxy did not become ready within 8 seconds; verify Node.js and the cassette format".into(),
        ))
    }
}

/// A running local proxy chain. Dropping it always attempts to stop every hop.
pub struct Session {
    proxies: Vec<RunningProxy>,
    url: String,
    mode: Mode,
    cassette: PathBuf,
    progress: Option<PathBuf>,
}

impl Session {
    /// Start the configured proxy chain on loopback.
    ///
    /// `ASB_PROXY_CHAIN` is a comma-separated list ordered from the agent
    /// toward the provider. It defaults to `llmtape` for record/replay. The
    /// agent always receives only the first hop's URL; changing the chain is
    /// therefore transparent to every agent adapter.
    pub fn start(
        mode: Mode,
        cassette: &Path,
        upstream: Option<&str>,
        respect_record_timings: bool,
        workspace: &Path,
    ) -> Result<Self, Error> {
        if mode == Mode::Replay && !cassette.is_file() {
            return Err(Error::Input(format!(
                "replay cassette {} does not exist; run `asb run CONFIG --record --cassette {}` first",
                cassette.display(),
                cassette.display()
            )));
        }
        let names = configured_chain()?;
        let adapters: Vec<&'static dyn ProxyAdapter> = names
            .iter()
            .map(|name| {
                proxy_adapter(name).ok_or_else(|| {
                    let available = PROXY_ADAPTERS
                        .iter()
                        .map(|adapter| adapter.name())
                        .collect::<Vec<_>>()
                        .join(", ");
                    Error::Input(format!(
                        "unknown proxy adapter `{name}` in ASB_PROXY_CHAIN; available adapters: {available}"
                    ))
                })
            })
            .collect::<Result<_, _>>()?;
        let mut proxies = Vec::with_capacity(adapters.len());
        let mut next_hop = upstream.map(str::to_owned);
        for adapter in adapters.iter().rev() {
            let request = ProxyRequest {
                mode,
                cassette: cassette.to_path_buf(),
                upstream: next_hop.clone(),
                respect_record_timings,
                workspace: workspace.to_path_buf(),
            };
            match adapter.start(&request) {
                Ok(proxy) => {
                    next_hop = Some(proxy.url.clone());
                    proxies.push(proxy);
                }
                Err(error) => {
                    for mut proxy in proxies {
                        let _ = proxy.child.kill();
                        let _ = proxy.child.wait();
                    }
                    return Err(Error::Run(format!(
                        "proxy adapter `{}` failed: {error}",
                        adapter.name()
                    )));
                }
            }
        }
        proxies.reverse();
        let first = proxies.first().ok_or_else(|| {
            Error::Input("ASB_PROXY_CHAIN must contain at least one proxy adapter when recording or replaying".into())
        })?;
        let progress = proxies.iter().find_map(|proxy| proxy.progress.clone());
        Ok(Self {
            url: first.url.clone(),
            proxies,
            mode,
            cassette: cassette.to_path_buf(),
            progress,
        })
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn stop(mut self) -> Result<(), Error> {
        let mut stop_error = None;
        for proxy in self.proxies.iter_mut().rev() {
            let _ = proxy.child.kill();
            if let Err(error) = proxy.child.wait() {
                stop_error = Some(Error::Run(format!("could not stop proxy adapter: {error}")));
            }
        }
        if let Some(error) = stop_error {
            return Err(error);
        }
        if self.mode == Mode::Replay {
            let progress = self.progress.as_ref().expect("replay has progress path");
            let text = std::fs::read_to_string(progress).map_err(|error| {
                Error::Run(format!(
                    "replay did not produce cassette progress for {}; verify the tape proxy: {error}",
                    self.cassette.display()
                ))
            })?;
            let value: serde_json::Value = serde_json::from_str(&text).map_err(|error| {
                Error::Run(format!("replay cassette progress is malformed: {error}"))
            })?;
            let consumed = value
                .get("consumed")
                .and_then(|value| value.as_u64())
                .unwrap_or(0);
            let total = value
                .get("total")
                .and_then(|value| value.as_u64())
                .unwrap_or(0);
            let rejected = value
                .get("rejected")
                .and_then(|value| value.as_u64())
                .unwrap_or(0);
            if rejected != 0 {
                return Err(Error::Run(format!(
                    "replay rejected {rejected} request(s) for {}; the normalized request or sequence differs from the recording",
                    self.cassette.display()
                )));
            }
            if consumed != total {
                return Err(Error::Run(format!(
                    "replay consumed {consumed} of {total} cassette requests for {}; refusing partial replay",
                    self.cassette.display()
                )));
            }
        }
        Ok(())
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
fn resolve_executable(directory: &Path) -> Result<ProxyExecutable, Error> {
    if let Some(override_path) = std::env::var("ASB_LLM_TAPE")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        return Ok(ProxyExecutable::Override(override_path));
    }
    let script = directory.join(".asb-tape-proxy.mjs");
    std::fs::write(&script, BUILTIN_PROXY).map_err(|error| {
        Error::Run(format!(
            "cannot materialize ASB's built-in tape proxy at {}: {error}",
            script.display()
        ))
    })?;
    Ok(ProxyExecutable::Builtin(script))
}

// Created exclusively, so teardown only removes files owned by this session.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn create(parent: &Path) -> Result<Self, Error> {
        for _ in 0..100 {
            let id = NEXT_SESSION.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!(".asb-tape-{}-{id}", std::process::id()));
            let mut builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(Error::Run(format!(
                        "cannot create tape session directory {}: {error}",
                        path.display()
                    )));
                }
            }
        }
        Err(Error::Run(
            "cannot allocate a unique tape session directory".into(),
        ))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        for proxy in &mut self.proxies {
            let _ = proxy.child.kill();
            let _ = proxy.child.wait();
        }
    }
}

fn configured_chain() -> Result<Vec<String>, Error> {
    let raw = std::env::var("ASB_PROXY_CHAIN").unwrap_or_else(|_| "llmtape".into());
    parse_chain(&raw)
}

fn proxy_adapter(name: &str) -> Option<&'static dyn ProxyAdapter> {
    PROXY_ADAPTERS
        .iter()
        .copied()
        .find(|adapter| adapter.name().eq_ignore_ascii_case(name))
}

fn parse_chain(raw: &str) -> Result<Vec<String>, Error> {
    let names = raw
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    if names.is_empty() {
        return Err(Error::Input(
            "ASB_PROXY_CHAIN must contain at least one adapter when --record or --replay is enabled".into(),
        ));
    }
    Ok(names)
}

fn free_port() -> Result<u16, Error> {
    TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .map(|listener| listener.local_addr().map(|address| address.port()))
        .map_err(|error| Error::Run(format!("cannot reserve a local llmtape port: {error}")))?
        .map_err(|error| Error::Run(format!("cannot inspect local llmtape port: {error}")))
}

#[cfg(test)]
mod tests {
    use super::{Mode, ProxyExecutable, Session, parse_chain, resolve_executable};
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_root() -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("asb-tape-session-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn cassette(root: &Path) -> std::path::PathBuf {
        let path = root.join("record.jsonl");
        let body = r#"{"model":"m","messages":[{"content":"work in <asb-workspace>"}],"tools":[{"type":"function"}]}"#;
        let line = serde_json::json!({"request":{"method":"POST","url":"/v1/chat/completions","body":body},"response":{"status":200,"headers":{"content-type":"application/json"},"body":"{\"ok\":true}"}});
        std::fs::write(&path, format!("{line}\n")).unwrap();
        path
    }

    fn request(session: &Session, body: &str) -> String {
        let address = session
            .url()
            .trim_start_matches("http://")
            .trim_end_matches("/v1");
        let mut socket = TcpStream::connect(address).unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        socket
            .set_write_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        write!(socket, "POST /v1/chat/completions HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
        let mut response = String::new();
        socket.read_to_string(&mut response).unwrap();
        response
    }

    #[test]
    fn eight_sessions_replay_one_cassette_independently() {
        let root = test_root();
        let cassette = cassette(&root);
        std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8).map(|index| {
                let cassette = &cassette;
                scope.spawn(move || {
                    let workspace = format!("/tmp/replica-{index}");
                    let session = Session::start(Mode::Replay, cassette, None, false, Path::new(&workspace)).unwrap();
                    let body = format!(r#"{{"tools":[{{"type":"function"}}],"messages":[{{"content":"work in {workspace}"}}],"model":"m"}}"#);
                    assert!(request(&session, &body).contains("200 OK"));
                    session.stop().unwrap();
                })
            }).collect();
            for handle in handles {
                handle.join().unwrap();
            }
        });
        assert_eq!(
            std::fs::read_dir(&root).unwrap().count(),
            1,
            "session artifacts cleaned up"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn replay_rejects_changed_body_and_extra_requests() {
        let root = test_root();
        let cassette = cassette(&root);
        let workspace = Path::new("/tmp/replay-workspace");
        let session = Session::start(Mode::Replay, &cassette, None, false, workspace).unwrap();
        let changed =
            r#"{"model":"m","messages":[{"content":"different"}],"tools":[{"type":"function"}]}"#;
        assert!(request(&session, changed).contains("409 Conflict"));
        assert!(session.stop().unwrap_err().to_string().contains("rejected"));
        let session = Session::start(Mode::Replay, &cassette, None, false, workspace).unwrap();
        let valid = r#"{"model":"m","messages":[{"content":"work in /tmp/replay-workspace"}],"tools":[{"type":"function"}]}"#;
        assert!(request(&session, valid).contains("200 OK"));
        assert!(request(&session, valid).contains("409 Conflict"));
        assert!(session.stop().unwrap_err().to_string().contains("rejected"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn replay_rejects_oversized_request_during_streaming() {
        let root = test_root();
        let cassette = cassette(&root);
        let session = Session::start(Mode::Replay, &cassette, None, false, &root).unwrap();
        let response = request(&session, &"x".repeat(16 * 1024 * 1024 + 1));
        assert!(response.contains("413 Payload Too Large"), "{response}");
        assert!(response.contains("request body exceeds 16 MiB"));
        assert!(session.stop().unwrap_err().to_string().contains("rejected"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn record_rejects_oversized_upstream_response_during_streaming() {
        let root = test_root();
        let cassette = root.join("record.jsonl");
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let upstream = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut connection, _) = listener.accept().unwrap();
            connection
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            connection
                .set_write_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut incoming = [0u8; 8192];
            assert!(connection.read(&mut incoming).unwrap() > 0);
            connection
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 16777217\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
            let chunk = vec![b'x'; 65536];
            for _ in 0..256 {
                if connection.write_all(&chunk).is_err() {
                    return;
                }
            }
            let _ = connection.write_all(b"x");
        });
        let session =
            Session::start(Mode::Record, &cassette, Some(&upstream), false, &root).unwrap();
        let response = request(&session, "{}");
        assert!(response.contains("502 Bad Gateway"), "{response}");
        assert!(response.contains("upstream response body exceeds 16 MiB"));
        session.stop().unwrap();
        server.join().unwrap();
        assert!(!cassette.exists() || std::fs::metadata(&cassette).unwrap().len() == 0);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn records_then_replays_with_a_different_workspace() {
        let root = test_root();
        let cassette = root.join("record.jsonl");
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let upstream = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut connection, _) = listener.accept().unwrap();
            let mut request = [0u8; 8192];
            let _ = connection.read(&mut request).unwrap();
            connection.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").unwrap();
        });
        let session = Session::start(
            Mode::Record,
            &cassette,
            Some(&upstream),
            false,
            Path::new("/tmp/original-workspace"),
        )
        .unwrap();
        let original = r#"{"model":"m","messages":[{"content":"work in /tmp/original-workspace"}],"tools":[{"type":"function"}]}"#;
        assert!(request(&session, original).contains("200 OK"));
        session.stop().unwrap();
        server.join().unwrap();
        let session = Session::start(
            Mode::Replay,
            &cassette,
            None,
            false,
            Path::new("/tmp/replica-workspace"),
        )
        .unwrap();
        let replay = r#"{"model":"m","messages":[{"content":"work in /tmp/replica-workspace"}],"tools":[{"type":"function"}]}"#;
        assert!(request(&session, replay).contains("200 OK"));
        session.stop().unwrap();
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn replay_can_honor_recorded_response_delay() {
        let root = test_root();
        let cassette = cassette(&root);
        let line = std::fs::read_to_string(&cassette).unwrap();
        let mut interaction: serde_json::Value = serde_json::from_str(&line).unwrap();
        interaction["delay_ms"] = 200.into();
        std::fs::write(&cassette, format!("{interaction}\n")).unwrap();
        let session = Session::start(
            Mode::Replay,
            &cassette,
            None,
            true,
            Path::new("/tmp/replay-workspace"),
        )
        .unwrap();
        let valid = r#"{"model":"m","messages":[{"content":"work in /tmp/replay-workspace"}],"tools":[{"type":"function"}]}"#;
        let started = std::time::Instant::now();
        assert!(request(&session, valid).contains("200 OK"));
        assert!(started.elapsed() >= std::time::Duration::from_millis(180));
        session.stop().unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn built_in_proxy_is_materialized_beside_the_cassette() {
        let root = std::env::temp_dir().join(format!("asb-tape-test-{}", std::process::id()));
        let cassette = root.join("cassettes/run.jsonl");
        std::fs::create_dir_all(cassette.parent().unwrap()).unwrap();
        let resolved = resolve_executable(cassette.parent().unwrap()).unwrap();
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

    #[test]
    fn proxy_chain_is_ordered_from_agent_to_provider() {
        assert_eq!(
            parse_chain("llmtape, audit, redact").unwrap(),
            vec!["llmtape", "audit", "redact"]
        );
        assert!(parse_chain(" , ").is_err());
    }
}
