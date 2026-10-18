//! Local cli2key API lifecycle and client configuration.

use crate::Error;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) fn prepare(
    command: &mut Command,
    model: &str,
    bridge: Option<(&str, &str)>,
) -> Result<(), Error> {
    let (key, base_url) = bridge.ok_or_else(|| {
        Error::Run("cli2key provider has no API endpoint; run `asb tool install cli2key` or set CLI2KEY_BASE_URL and CLI2KEY_API_KEY".into())
    })?;
    command
        .env("CLI2KEY_API_KEY", key)
        .env("CLI2KEY_BASE_URL", base_url)
        .env("CLI2KEY_MODEL", model)
        .env("OPENAI_API_KEY", key)
        .env("OPENAI_BASE_URL", base_url)
        .env("OPENAI_API_BASE", base_url)
        .env("OPENAI_MODEL", model)
        .env("ANTHROPIC_MODEL", model)
        .env("ANTHROPIC_API_KEY", key)
        .env("ANTHROPIC_BASE_URL", base_url.trim_end_matches("/v1"));
    Ok(())
}

pub(super) fn local_executable() -> Option<String> {
    let (root, config) = crate::project::load(std::path::Path::new(".")).ok()?;
    let mut candidates = Vec::new();
    if let Some(directory) = std::env::var_os("ASB_TOOL_DIR") {
        candidates.push(std::path::PathBuf::from(directory).join("codex-bridge"));
    }
    candidates.push(
        crate::project::resolve(&root, &config.tools_dir)
            .join("cli2key/node_modules/.bin/codex-bridge"),
    );
    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .and_then(|candidate| candidate.canonicalize().ok())
        .map(|candidate| candidate.to_string_lossy().into_owned())
}

/// A cli2key API endpoint and the optional server process ASB owns.
pub struct Bridge {
    api_key: String,
    base_url: String,
    process: Option<Child>,
}

impl super::ProviderSession for Bridge {
    fn api_key(&self) -> &str {
        Bridge::api_key(self)
    }
    fn base_url(&self) -> &str {
        Bridge::base_url(self)
    }
}

impl Bridge {
    /// Connect to a configured endpoint or start the project-local bridge.
    pub fn connect(executable: Option<&str>) -> Result<Self, Error> {
        if let Some(endpoint) = std::env::var_os("CLI2KEY_BASE_URL") {
            let base_url = normalize_endpoint(&endpoint.to_string_lossy())?;
            let api_key = required_key()?;
            return Ok(Self {
                api_key,
                base_url,
                process: None,
            });
        }
        let executable = executable.ok_or(Error::Run(
            "cli2key provider needs a local bridge; run `asb tool install cli2key` in the initialized project, or set CLI2KEY_BASE_URL and CLI2KEY_API_KEY for an external bridge".into(),
        ))?;
        let host = std::env::var("CODEX_BRIDGE_HOST").unwrap_or_else(|_| "127.0.0.1".into());
        let port = std::env::var("CODEX_BRIDGE_PORT").unwrap_or_else(|_| "3456".into());
        let port_number = port.parse::<u16>().map_err(|_| {
            Error::Run("CODEX_BRIDGE_PORT must be an integer from 1 to 65535".into())
        })?;
        if port_number == 0 {
            return Err(Error::Run(
                "CODEX_BRIDGE_PORT must be an integer from 1 to 65535".into(),
            ));
        }
        let client_host = if host == "0.0.0.0" || host == "::" {
            "127.0.0.1"
        } else {
            host.as_str()
        };
        let base_url = format!("http://{client_host}:{port_number}/v1");
        let process = Command::new(executable)
            .arg("serve")
            .env("CODEX_BRIDGE_HOST", &host)
            .env("CODEX_BRIDGE_PORT", &port)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| Error::Run(format!("could not start cli2key bridge: {error}")))?;
        // Own the child immediately: every startup error must kill and reap it.
        let mut bridge = Self {
            api_key: String::new(),
            base_url,
            process: Some(process),
        };
        let process = bridge.process.as_mut().expect("spawned bridge");
        let stdout = process
            .stdout
            .take()
            .ok_or_else(|| Error::Run("cli2key bridge did not expose startup output".into()))?;
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().take(64) {
                let Ok(line) = line else { break };
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        let mut discovered_key = None;
        let deadline = Instant::now() + STARTUP_TIMEOUT;
        for _ in 0..64 {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(Error::Run(
                    "cli2key bridge did not become ready within 5 seconds".into(),
                ));
            }
            match receiver.recv_timeout(remaining) {
                Ok(line) => {
                    if let Some(key) = line.strip_prefix("Codex Bridge API key: ") {
                        discovered_key = Some(key.trim().to_owned());
                        break;
                    }
                    if let Some(status) = process.try_wait().map_err(|error| {
                        Error::Run(format!("cannot inspect cli2key bridge: {error}"))
                    })? {
                        return Err(Error::Run(format!(
                            "cli2key bridge exited during startup with {status}"
                        )));
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let status = process.try_wait().map_err(|error| {
                        Error::Run(format!("cannot inspect cli2key bridge: {error}"))
                    })?;
                    return Err(Error::Run(match status {
                        Some(status) => {
                            format!("cli2key bridge exited during startup with {status}")
                        }
                        None => "cli2key bridge did not become ready within 5 seconds".into(),
                    }));
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(Error::Run(
                        "cli2key bridge closed its startup output".into(),
                    ));
                }
            }
        }
        let api_key = std::env::var("CLI2KEY_API_KEY")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .or(discovered_key)
            .ok_or_else(|| Error::Run("cli2key bridge did not provide an API key".into()))?;
        bridge.api_key = api_key;
        Ok(bridge)
    }

    /// Return the API key to configure the selected agent.
    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    /// Return the OpenAI-compatible base URL to configure the selected agent.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        if let Some(process) = self.process.as_mut() {
            if process.try_wait().ok().flatten().is_none() {
                let _ = process.kill();
            }
            let _ = process.wait();
        }
    }
}

fn required_key() -> Result<String, Error> {
    std::env::var("CLI2KEY_API_KEY")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            Error::Run(
                "CLI2KEY_BASE_URL is set but CLI2KEY_API_KEY is missing; provide the matching key or unset CLI2KEY_BASE_URL and run `asb tool install cli2key`"
                    .into(),
            )
        })
}

fn normalize_endpoint(value: &str) -> Result<String, Error> {
    let trimmed = value.trim_end_matches('/');
    if trimmed.is_empty() || trimmed.contains(['\r', '\n']) {
        return Err(Error::Run(
            "CLI2KEY_BASE_URL must be a non-empty single-line endpoint".into(),
        ));
    }
    Ok(if trimmed.ends_with("/v1") {
        trimmed.to_owned()
    } else {
        format!("{trimmed}/v1")
    })
}

#[cfg(test)]
mod tests {
    use super::normalize_endpoint;

    #[test]
    fn endpoint_normalization_targets_openai_compatible_api() {
        assert_eq!(
            normalize_endpoint("http://127.0.0.1:3456").unwrap(),
            "http://127.0.0.1:3456/v1"
        );
        assert_eq!(
            normalize_endpoint("http://127.0.0.1:3456/v1/").unwrap(),
            "http://127.0.0.1:3456/v1"
        );
        assert!(normalize_endpoint("\n").is_err());
    }
}
