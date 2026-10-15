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
}

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
    },
    ProviderSpec {
        name: "openrouter",
        credential: Some("OPENROUTER_API_KEY"),
        endpoint: Some("OPENROUTER_BASE_URL"),
    },
    ProviderSpec {
        name: "anthropic",
        credential: Some("ANTHROPIC_API_KEY"),
        endpoint: Some("ANTHROPIC_BASE_URL"),
    },
    ProviderSpec {
        name: "gemini",
        credential: Some("GEMINI_API_KEY"),
        endpoint: Some("GOOGLE_GEMINI_BASE_URL"),
    },
    ProviderSpec {
        name: "ollama",
        credential: None,
        endpoint: Some("OLLAMA_HOST"),
    },
    ProviderSpec {
        name: "groq",
        credential: Some("GROQ_API_KEY"),
        endpoint: Some("GROQ_BASE_URL"),
    },
    ProviderSpec {
        name: "mistral",
        credential: Some("MISTRAL_API_KEY"),
        endpoint: Some("MISTRAL_BASE_URL"),
    },
    ProviderSpec {
        name: "deepseek",
        credential: Some("DEEPSEEK_API_KEY"),
        endpoint: Some("DEEPSEEK_BASE_URL"),
    },
    ProviderSpec {
        name: "xai",
        credential: Some("XAI_API_KEY"),
        endpoint: Some("XAI_BASE_URL"),
    },
    ProviderSpec {
        name: "together",
        credential: Some("TOGETHER_API_KEY"),
        endpoint: Some("TOGETHER_BASE_URL"),
    },
    ProviderSpec {
        name: "fireworks",
        credential: Some("FIREWORKS_API_KEY"),
        endpoint: Some("FIREWORKS_BASE_URL"),
    },
    ProviderSpec {
        name: "cli2key",
        credential: Some("CLI2KEY_API_KEY"),
        endpoint: Some("CLI2KEY_BASE_URL"),
    },
    ProviderSpec {
        name: "local",
        credential: None,
        endpoint: None,
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
        })
}

/// Names of providers for which ASB can attempt live model discovery.
pub fn names() -> &'static [&'static str] {
    &[
        "openai",
        "openrouter",
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

/// Return a finite model list only where the local bridge publishes one.
pub fn known_models(provider: &str) -> Option<&'static [&'static str]> {
    (provider.eq_ignore_ascii_case("cli2key")).then_some(&[
        "gpt-6-astra",
        "gpt-6.1-sol",
        "gpt-6-luna",
    ])
}

/// Validate a user-facing provider or agent identifier.
pub fn is_valid_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

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
/// The child inherits the parent environment. If a local key file is used,
/// its bounded credential is injected into the child without being printed.
pub fn configure(command: &mut std::process::Command, provider: &str, model: &str) {
    let selected = spec(provider);
    if let Some(variable) = selected.credential {
        if let Some(value) = credential(provider) {
            command.env(variable, value);
        }
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
    match provider.to_ascii_lowercase().as_str() {
        "openrouter" => {
            command.env("OPENROUTER_MODEL", model);
        }
        "openai" => {
            command.env("OPENAI_MODEL", model);
        }
        "ollama" => {
            command.env("OLLAMA_MODEL", model);
        }
        "cli2key" => {
            command.env("CLI2KEY_MODEL", model);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{ProviderSpec, read_key_file, spec};
    use std::fs;

    #[test]
    fn known_and_custom_providers_are_configurable() {
        assert_eq!(
            spec("cli2key"),
            ProviderSpec {
                name: "cli2key",
                credential: Some("CLI2KEY_API_KEY"),
                endpoint: Some("CLI2KEY_BASE_URL"),
            }
        );
        assert_eq!(spec("my-provider").name, "custom");
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
}
