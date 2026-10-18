//! Small provider registry for environment-driven integrations.

use std::fs;
use std::path::{Path, PathBuf};

const MAX_KEY_FILE_BYTES: u64 = 4096;

/// One provider's conventional environment names.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderSpec {
    /// Stable name accepted by `asb setup --provider`.
    pub name: &'static str,
    /// Environment variable holding the provider credential, when applicable.
    pub credential: Option<&'static str>,
    /// Environment variable commonly used to override the provider endpoint.
    pub endpoint: Option<&'static str>,
    /// Provider-specific model selection environment variable.
    pub model_env: Option<&'static str>,
    /// Endpoint variables that must route through the local tape proxy.
    pub tape_endpoint_envs: &'static [&'static str],
    /// Project-local bridge tool used by this provider, when required.
    pub bridge_tool: Option<&'static str>,
}

const TAPE_ENDPOINT_ENVS: &[&str] = &[
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
];

/// Response shape used by a provider's model-list endpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelKind {
    OpenAi,
    Gemini,
    Anthropic,
    Ollama,
}

/// Live model discovery details for a supported provider.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelSpec {
    pub name: &'static str,
    pub credential: Option<&'static str>,
    pub endpoint: Option<&'static str>,
    pub default_url: Option<&'static str>,
    pub kind: ModelKind,
}

impl ModelSpec {
    /// Resolve an endpoint override or the provider's documented default.
    pub fn url(self) -> Option<String> {
        let base = self
            .endpoint
            .and_then(std::env::var_os)
            .map(|value| value.to_string_lossy().into_owned())
            .or_else(|| self.default_url.map(str::to_owned))?;
        let trimmed = base.trim_end_matches('/');
        if trimmed.ends_with("/models") || trimmed.ends_with("/tags") {
            return Some(trimmed.to_owned());
        }
        let suffix = match self.kind {
            ModelKind::Gemini => "/v1beta/models",
            ModelKind::Ollama => "/api/tags",
            ModelKind::OpenAi | ModelKind::Anthropic => "/models",
        };
        Some(format!("{trimmed}{suffix}"))
    }
}

const PROVIDERS: &[ProviderSpec] = &[
    ProviderSpec {
        name: "openai",
        credential: Some("OPENAI_API_KEY"),
        endpoint: Some("OPENAI_BASE_URL"),
        model_env: Some("OPENAI_MODEL"),
        tape_endpoint_envs: TAPE_ENDPOINT_ENVS,
        bridge_tool: None,
    },
    ProviderSpec {
        name: "openrouter",
        credential: Some("OPENROUTER_API_KEY"),
        endpoint: Some("OPENROUTER_BASE_URL"),
        model_env: Some("OPENROUTER_MODEL"),
        tape_endpoint_envs: TAPE_ENDPOINT_ENVS,
        bridge_tool: None,
    },
    ProviderSpec {
        name: "opencode-zen",
        credential: Some("OPENCODE_API_KEY"),
        endpoint: Some("OPENCODE_ZEN_BASE_URL"),
        model_env: None,
        tape_endpoint_envs: TAPE_ENDPOINT_ENVS,
        bridge_tool: None,
    },
    ProviderSpec {
        name: "anthropic",
        credential: Some("ANTHROPIC_API_KEY"),
        endpoint: Some("ANTHROPIC_BASE_URL"),
        model_env: None,
        tape_endpoint_envs: TAPE_ENDPOINT_ENVS,
        bridge_tool: None,
    },
    ProviderSpec {
        name: "gemini",
        credential: Some("GEMINI_API_KEY"),
        endpoint: Some("GOOGLE_GEMINI_BASE_URL"),
        model_env: None,
        tape_endpoint_envs: TAPE_ENDPOINT_ENVS,
        bridge_tool: None,
    },
    ProviderSpec {
        name: "ollama",
        credential: None,
        endpoint: Some("OLLAMA_HOST"),
        model_env: Some("OLLAMA_MODEL"),
        tape_endpoint_envs: TAPE_ENDPOINT_ENVS,
        bridge_tool: None,
    },
    ProviderSpec {
        name: "groq",
        credential: Some("GROQ_API_KEY"),
        endpoint: Some("GROQ_BASE_URL"),
        model_env: None,
        tape_endpoint_envs: TAPE_ENDPOINT_ENVS,
        bridge_tool: None,
    },
    ProviderSpec {
        name: "mistral",
        credential: Some("MISTRAL_API_KEY"),
        endpoint: Some("MISTRAL_BASE_URL"),
        model_env: None,
        tape_endpoint_envs: TAPE_ENDPOINT_ENVS,
        bridge_tool: None,
    },
    ProviderSpec {
        name: "deepseek",
        credential: Some("DEEPSEEK_API_KEY"),
        endpoint: Some("DEEPSEEK_BASE_URL"),
        model_env: None,
        tape_endpoint_envs: TAPE_ENDPOINT_ENVS,
        bridge_tool: None,
    },
    ProviderSpec {
        name: "xai",
        credential: Some("XAI_API_KEY"),
        endpoint: Some("XAI_BASE_URL"),
        model_env: None,
        tape_endpoint_envs: TAPE_ENDPOINT_ENVS,
        bridge_tool: None,
    },
    ProviderSpec {
        name: "together",
        credential: Some("TOGETHER_API_KEY"),
        endpoint: Some("TOGETHER_BASE_URL"),
        model_env: None,
        tape_endpoint_envs: TAPE_ENDPOINT_ENVS,
        bridge_tool: None,
    },
    ProviderSpec {
        name: "fireworks",
        credential: Some("FIREWORKS_API_KEY"),
        endpoint: Some("FIREWORKS_BASE_URL"),
        model_env: None,
        tape_endpoint_envs: TAPE_ENDPOINT_ENVS,
        bridge_tool: None,
    },
    ProviderSpec {
        name: "cli2key",
        credential: Some("CLI2KEY_API_KEY"),
        endpoint: Some("CLI2KEY_BASE_URL"),
        model_env: Some("CLI2KEY_MODEL"),
        tape_endpoint_envs: TAPE_ENDPOINT_ENVS,
        bridge_tool: Some("cli2key"),
    },
    ProviderSpec {
        name: "local",
        credential: None,
        endpoint: None,
        model_env: None,
        tape_endpoint_envs: TAPE_ENDPOINT_ENVS,
        bridge_tool: None,
    },
];

/// Return a known provider or a generic custom provider specification.
pub fn spec(name: &str) -> ProviderSpec {
    PROVIDERS
        .iter()
        .copied()
        .find(|provider| provider.name.eq_ignore_ascii_case(name))
        .unwrap_or(ProviderSpec {
            name: "custom",
            credential: None,
            endpoint: None,
            model_env: None,
            tape_endpoint_envs: TAPE_ENDPOINT_ENVS,
            bridge_tool: None,
        })
}

/// Names of providers for which ASB can attempt live model discovery.
pub fn names() -> &'static [&'static str] {
    &[
        "openai",
        "openrouter",
        "opencode-zen",
        "anthropic",
        "gemini",
        "ollama",
        "groq",
        "mistral",
        "deepseek",
        "xai",
        "together",
        "fireworks",
        "cli2key",
    ]
}

/// Providers whose model responses have an unambiguous free-of-charge meaning.
pub fn free_names() -> &'static [&'static str] {
    &["openrouter", "opencode-zen", "ollama"]
}

/// Return whether `--free` can classify this provider's model catalog safely.
pub fn supports_free_models(name: &str) -> bool {
    free_names()
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(name))
}

/// Return a finite model list only where the local bridge publishes one.
pub fn known_models(provider: &str) -> Option<&'static [&'static str]> {
    (provider.eq_ignore_ascii_case("cli2key")).then_some(&[
        "gpt-6-astra",
        "gpt-6.1-sol",
        "gpt-6-luna",
    ])
}

/// Validate a user-facing provider or agent identifier.
/// Return the endpoint and credential information for live discovery.
pub fn model_spec(name: &str) -> Option<ModelSpec> {
    let lower = name.to_ascii_lowercase();
    let (name, credential, endpoint, default_url, kind) = match lower.as_str() {
        "openai" => (
            "openai",
            Some("OPENAI_API_KEY"),
            Some("OPENAI_BASE_URL"),
            Some("https://api.openai.com/v1/models"),
            ModelKind::OpenAi,
        ),
        "openrouter" => (
            "openrouter",
            Some("OPENROUTER_API_KEY"),
            Some("OPENROUTER_BASE_URL"),
            Some("https://openrouter.ai/api/v1/models"),
            ModelKind::OpenAi,
        ),
        "opencode-zen" => (
            "opencode-zen",
            Some("OPENCODE_API_KEY"),
            Some("OPENCODE_ZEN_BASE_URL"),
            Some("https://opencode.ai/zen/v1/models"),
            ModelKind::OpenAi,
        ),
        "anthropic" => (
            "anthropic",
            Some("ANTHROPIC_API_KEY"),
            Some("ANTHROPIC_BASE_URL"),
            Some("https://api.anthropic.com/v1/models"),
            ModelKind::Anthropic,
        ),
        "gemini" => (
            "gemini",
            Some("GEMINI_API_KEY"),
            Some("GOOGLE_GEMINI_BASE_URL"),
            Some("https://generativelanguage.googleapis.com/v1beta/models"),
            ModelKind::Gemini,
        ),
        "ollama" => (
            "ollama",
            None,
            Some("OLLAMA_HOST"),
            Some("http://localhost:11434/api/tags"),
            ModelKind::Ollama,
        ),
        "groq" => (
            "groq",
            Some("GROQ_API_KEY"),
            Some("GROQ_BASE_URL"),
            Some("https://api.groq.com/openai/v1/models"),
            ModelKind::OpenAi,
        ),
        "mistral" => (
            "mistral",
            Some("MISTRAL_API_KEY"),
            Some("MISTRAL_BASE_URL"),
            Some("https://api.mistral.ai/v1/models"),
            ModelKind::OpenAi,
        ),
        "deepseek" => (
            "deepseek",
            Some("DEEPSEEK_API_KEY"),
            Some("DEEPSEEK_BASE_URL"),
            Some("https://api.deepseek.com/models"),
            ModelKind::OpenAi,
        ),
        "xai" => (
            "xai",
            Some("XAI_API_KEY"),
            Some("XAI_BASE_URL"),
            Some("https://api.x.ai/v1/models"),
            ModelKind::OpenAi,
        ),
        "together" => (
            "together",
            Some("TOGETHER_API_KEY"),
            Some("TOGETHER_BASE_URL"),
            Some("https://api.together.xyz/v1/models"),
            ModelKind::OpenAi,
        ),
        "fireworks" => (
            "fireworks",
            Some("FIREWORKS_API_KEY"),
            Some("FIREWORKS_BASE_URL"),
            Some("https://api.fireworks.ai/inference/v1/models"),
            ModelKind::OpenAi,
        ),
        "cli2key" => (
            "cli2key",
            Some("CLI2KEY_API_KEY"),
            Some("CLI2KEY_BASE_URL"),
            None,
            ModelKind::OpenAi,
        ),
        _ => return None,
    };
    Some(ModelSpec {
        name,
        credential,
        endpoint,
        default_url,
        kind,
    })
}

/// Resolve a provider credential from the environment or the user's local key file.
///
/// Environment variables take precedence. Key files are bounded and never printed;
/// they are intended only as a convenience for local development.
pub fn credential(name: &str) -> Option<String> {
    let spec = model_spec(name)?;
    let variable = spec.credential?;
    if let Some(value) = std::env::var_os(variable) {
        let value = value.to_string_lossy().trim().to_owned();
        if !value.is_empty() {
            return Some(value);
        }
    }
    let home = std::env::var_os("HOME")?;
    read_key_file(Path::new(&home).join(format!(".api_key_{}", spec.name)))
}

fn read_key_file(path: PathBuf) -> Option<String> {
    let metadata = fs::metadata(path.as_path()).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_KEY_FILE_BYTES {
        return None;
    }
    let value = fs::read_to_string(path).ok()?.trim().to_owned();
    (!value.is_empty() && !value.contains(['\r', '\n'])).then_some(value)
}

/// Add provider-neutral and provider-specific selection variables to a child.
///
/// The child inherits the parent environment. During replay, known provider
/// credentials are removed and only the selected provider gets a placeholder.
pub fn configure(command: &mut std::process::Command, provider: &str, model: &str, replay: bool) {
    let selected = spec(provider);
    if replay {
        for variable in PROVIDERS.iter().filter_map(|spec| spec.credential) {
            command.env_remove(variable);
        }
        if let Some(variable) = selected.credential {
            command.env(variable, "asb-replay");
        }
    } else if let Some(variable) = selected.credential
        && let Some(value) = credential(provider)
    {
        command.env(variable, value);
    }
    command
        .env("ASB_PROVIDER", provider)
        .env("ASB_MODEL", model);
    if let Some(name) = selected.credential {
        command.env("ASB_PROVIDER_CREDENTIAL_ENV", name);
    }
    if let Some(name) = selected.endpoint {
        command.env("ASB_PROVIDER_ENDPOINT_ENV", name);
    }
    if let Some(name) = selected.model_env {
        command.env(name, model);
    }
}

/// Start a provider-owned local bridge when that provider requires one.
///
/// The runner deliberately does not know bridge executable names or tool IDs;
/// that relationship is owned by the provider integration.
pub(crate) fn start_local_bridge(
    provider: &str,
    project_root: &Path,
    tools_dir: &str,
    replay: bool,
) -> Result<Option<crate::cli2key::Bridge>, crate::Error> {
    let selected = spec(provider);
    if replay || selected.bridge_tool.is_none() {
        return Ok(None);
    }
    let executable = crate::tools::resolve(
        selected.bridge_tool.expect("bridge tool"),
        project_root,
        tools_dir,
    );
    crate::cli2key::Bridge::connect(executable.as_deref()).map(Some)
}

/// Return local-bridge credentials only for the provider that owns the bridge.
pub(crate) fn local_bridge_credentials<'a>(
    provider: &str,
    bridge: Option<&'a crate::cli2key::Bridge>,
    endpoint: Option<&'a str>,
) -> Option<(&'a str, &'a str)> {
    if spec(provider).bridge_tool.is_none() {
        return None;
    }
    Some((
        bridge.map_or("asb-replay", crate::cli2key::Bridge::api_key),
        endpoint.unwrap_or("http://127.0.0.1:0/v1"),
    ))
}

/// Return the endpoint of a provider-owned bridge without exposing its tool
/// implementation to the runner.
pub(crate) fn local_bridge_endpoint<'a>(
    provider: &str,
    bridge: Option<&'a crate::cli2key::Bridge>,
) -> Option<&'a str> {
    spec(provider)
        .bridge_tool
        .and_then(|_| bridge.map(crate::cli2key::Bridge::base_url))
}

/// Configure a child to route this provider through the tape proxy.
pub(crate) fn configure_tape(
    command: &mut std::process::Command,
    provider: &str,
    url: &str,
    mode: crate::tape::Mode,
) {
    let selected = spec(provider);
    let anthropic_url = url.trim_end_matches("/v1");
    for name in selected.tape_endpoint_envs {
        command.env(name, url);
    }
    command
        .env("ANTHROPIC_BASE_URL", anthropic_url)
        .env("LLM_BASE_URL", anthropic_url)
        .env("ASB_PROVIDER_BASE_URL", url)
        .env(
            "ASB_TAPE_MODE",
            match mode {
                crate::tape::Mode::Record => "record",
                crate::tape::Mode::Replay => "replay",
            },
        );
}

/// Resolve a record-mode upstream through provider-owned metadata.
pub(crate) fn tape_upstream(provider: &str, bridge_base_url: Option<&str>) -> Option<String> {
    if spec(provider).bridge_tool.is_some() {
        return bridge_base_url.map(strip_api_version);
    }
    std::env::var_os("ASB_PROVIDER_BASE_URL")
        .map(|value| value.to_string_lossy().trim().to_owned())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            model_spec(provider).and_then(|spec| spec.url()).map(|url| {
                ["/models", "/tags"]
                    .iter()
                    .find_map(|suffix| url.strip_suffix(suffix))
                    .unwrap_or(&url)
                    .to_owned()
            })
        })
        .map(|url| strip_api_version(&url))
}

fn strip_api_version(value: &str) -> String {
    value
        .trim_end_matches('/')
        .trim_end_matches("/v1")
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::{ProviderSpec, configure_tape, read_key_file, spec, tape_upstream};
    use crate::tape::Mode;
    use std::fs;

    #[test]
    fn known_and_custom_providers_are_configurable() {
        assert_eq!(
            spec("cli2key"),
            ProviderSpec {
                name: "cli2key",
                credential: Some("CLI2KEY_API_KEY"),
                endpoint: Some("CLI2KEY_BASE_URL"),
                model_env: Some("CLI2KEY_MODEL"),
                tape_endpoint_envs: super::TAPE_ENDPOINT_ENVS,
                bridge_tool: Some("cli2key"),
            }
        );
        assert_eq!(spec("my-provider").name, "custom");
        assert_eq!(spec("opencode-zen").name, "opencode-zen");
        assert!(super::supports_free_models("opencode-zen"));
    }

    #[test]
    fn key_file_reader_trims_bounded_single_line_keys() {
        let path = std::env::temp_dir().join(format!("asb-provider-key-{}", std::process::id()));
        fs::write(&path, "  test-key\n").expect("key file writes");
        assert_eq!(read_key_file(path.clone()).as_deref(), Some("test-key"));
        fs::write(&path, "first\nsecond").expect("invalid key writes");
        assert!(read_key_file(path.clone()).is_none());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn tape_routing_is_owned_by_the_provider_adapter() {
        assert_eq!(
            tape_upstream("cli2key", Some("http://127.0.0.1:3456/v1")).as_deref(),
            Some("http://127.0.0.1:3456")
        );
        assert_eq!(
            tape_upstream("openrouter", None).as_deref(),
            Some("https://openrouter.ai/api")
        );
        let mut command = std::process::Command::new("agent");
        configure_tape(
            &mut command,
            "openrouter",
            "http://127.0.0.1:3456/v1",
            Mode::Replay,
        );
        let values = command
            .get_envs()
            .filter_map(|(name, value)| {
                value.map(|value| {
                    (
                        name.to_string_lossy().into_owned(),
                        value.to_string_lossy().into_owned(),
                    )
                })
            })
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(
            values.get("OPENROUTER_BASE_URL"),
            Some(&"http://127.0.0.1:3456/v1".to_owned())
        );
        assert_eq!(values.get("ASB_TAPE_MODE"), Some(&"replay".to_owned()));
    }
}
