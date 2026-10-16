//! Local llmtape proxy lifecycle for deterministic LLM recording and replay.

use crate::Error;
use crate::providers;
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
    mode: Mode,
    cassette: PathBuf,
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
                        "record requires a provider API endpoint; set the provider base URL or use cli2key".into(),
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
                    mode,
                    cassette: cassette.to_path_buf(),
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
            .map_err(|error| Error::Run(format!("could not stop llmtape: {error}")))?;
        if self.mode == Mode::Replay {
            let progress = self.cassette.with_extension("jsonl.progress.json");
            let text = std::fs::read_to_string(&progress).map_err(|error| {
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

/// Point common provider clients at the local tape proxy.
pub fn configure_command(command: &mut Command, provider: &str, url: &str, mode: Mode) {
    let anthropic_url = url.trim_end_matches("/v1");
    for name in [
        "OPENAI_BASE_URL",
        "OPENAI_API_BASE",
        "OPENROUTER_BASE_URL",
        "GROQ_BASE_URL",
        "MISTRAL_BASE_URL",
        "DEEPSEEK_BASE_URL",
        "XAI_BASE_URL",
        "TOGETHER_BASE_URL",
        "FIREWORKS_BASE_URL",
        "GOOGLE_GEMINI_BASE_URL",
        "OLLAMA_HOST",
        "CLI2KEY_BASE_URL",
    ] {
        command.env(name, url);
    }
    command
        .env("ANTHROPIC_BASE_URL", anthropic_url)
        .env("LLM_BASE_URL", anthropic_url)
        .env("ASB_PROVIDER_BASE_URL", url)
        .env(
            "ASB_TAPE_MODE",
            match mode {
                Mode::Record => "record",
                Mode::Replay => "replay",
            },
        );
    if provider.eq_ignore_ascii_case("cli2key") {
        command.env("CLI2KEY_BASE_URL", url);
    }
}

/// Resolve the upstream API base used by llmtape in record mode.
pub fn upstream(provider: &str, bridge_base_url: Option<&str>) -> Option<String> {
    if provider.eq_ignore_ascii_case("cli2key") {
        return bridge_base_url.map(strip_api_version);
    }
    if let Some(value) = std::env::var_os("ASB_PROVIDER_BASE_URL") {
        let value = value.to_string_lossy().trim().to_owned();
        if !value.is_empty() {
            return Some(strip_api_version(&value));
        }
    }
    providers::model_spec(provider)
        .and_then(|spec| spec.url())
        .map(|url| {
            ["/models", "/tags"]
                .iter()
                .find_map(|suffix| url.strip_suffix(suffix))
                .unwrap_or(&url)
                .to_owned()
        })
        .map(|url| strip_api_version(&url))
}

fn strip_api_version(value: &str) -> String {
    value
        .trim_end_matches('/')
        .trim_end_matches("/v1")
        .to_owned()
}

fn free_port() -> Result<u16, Error> {
    TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .map(|listener| listener.local_addr().map(|address| address.port()))
        .map_err(|error| Error::Run(format!("cannot reserve a local llmtape port: {error}")))?
        .map_err(|error| Error::Run(format!("cannot inspect local llmtape port: {error}")))
}

#[cfg(test)]
mod tests {
    use super::{Mode, ProxyExecutable, configure_command, resolve_executable, upstream};
    use std::process::Command;

    #[test]
    fn upstream_uses_bridge_for_cli2key_and_known_provider_defaults() {
        assert_eq!(
            upstream("cli2key", Some("http://127.0.0.1:3456/v1")).as_deref(),
            Some("http://127.0.0.1:3456")
        );
        assert_eq!(
            upstream("openrouter", None).as_deref(),
            Some("https://openrouter.ai/api")
        );
    }

    #[test]
    fn endpoint_configuration_is_local_and_mode_explicit() {
        let mut command = Command::new("agent");
        configure_command(
            &mut command,
            "openrouter",
            "http://127.0.0.1:3456/v1",
            Mode::Replay,
        );
        let values = command
            .get_envs()
            .filter_map(|(name, value)| {
                value.map(|value| (name.to_string_lossy(), value.to_string_lossy()))
            })
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(
            values
                .get("OPENROUTER_BASE_URL")
                .map(|value| value.as_ref()),
            Some("http://127.0.0.1:3456/v1")
        );
        assert_eq!(
            values.get("ASB_TAPE_MODE").map(|value| value.as_ref()),
            Some("replay")
        );
    }

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
