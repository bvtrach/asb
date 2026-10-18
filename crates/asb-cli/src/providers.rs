//! Small provider registry for environment-driven integrations.

mod preparation;
pub(crate) use preparation::apply_with_tape;

use crate::endpoint::Interface;
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
    Together,
    Fireworks,
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
        self.url_for_model(None)
    }

    /// A Fireworks model resource identifies the account whose catalog contains it.
    pub fn url_for_model(self, selected: Option<&str>) -> Option<String> {
        let base = self
            .endpoint
            .and_then(std::env::var_os)
            .map(|value| value.to_string_lossy().into_owned())
            .or_else(|| {
                if matches!(self.kind, ModelKind::Fireworks) {
                    let account = std::env::var("FIREWORKS_ACCOUNT_ID")
                        .ok()
                        .or_else(|| {
                            selected
                                .and_then(fireworks_model_account)
                                .map(str::to_owned)
                        })
                        .unwrap_or_else(|| "fireworks".into());
                    if account.is_empty()
                        || !account
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                    {
                        return None;
                    }
                    Some(format!(
                        "https://api.fireworks.ai/v1/accounts/{account}/models"
                    ))
                } else {
                    self.default_url.map(str::to_owned)
                }
            })?;
        let base = if matches!(self.kind, ModelKind::Ollama) && !base.contains("://") {
            format!("http://{base}")
        } else {
            base
        };
        let trimmed = base.trim_end_matches('/');
        if trimmed.ends_with("/models") || trimmed.ends_with("/tags") {
            return Some(trimmed.to_owned());
        }
        let suffix = match self.kind {
            ModelKind::Gemini if trimmed.ends_with("/v1beta") || trimmed.ends_with("/v1") => {
                "/models"
            }
            ModelKind::Gemini => "/v1beta/models",
            ModelKind::Ollama => "/api/tags",
            ModelKind::OpenAi
            | ModelKind::Anthropic
            | ModelKind::Together
            | ModelKind::Fireworks => "/models",
        };
        Some(format!("{trimmed}{suffix}"))
    }
}

fn fireworks_model_account(model: &str) -> Option<&str> {
    let mut parts = model.split('/');
    (parts.next()? == "accounts").then_some(())?;
    let account = parts.next()?;
    (parts.next()? == "models"
        && parts.next().is_some_and(|part| !part.is_empty())
        && parts.next().is_none())
    .then_some(account)
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

/// Offline UI suggestions only; never use this stale snapshot to validate runs.
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
            Some("https://api.together.ai/v1/models"),
            ModelKind::Together,
        ),
        "fireworks" => (
            "fireworks",
            Some("FIREWORKS_API_KEY"),
            Some("FIREWORKS_MODELS_URL"),
            Some("https://api.fireworks.ai/v1/accounts/fireworks/models"),
            ModelKind::Fireworks,
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

/// Interfaces offered by the provider for this model, in provider preference
/// order. A model whose endpoint family is not yet known must not be guessed.
pub(crate) fn offered_endpoints(provider: &str, model: &str) -> Vec<crate::endpoint::Offer> {
    use crate::endpoint::{
        Address,
        Authentication::{ApiKey, Bearer, None as NoAuth},
        Offer,
    };
    use Interface::{
        AnthropicMessages as Messages, Gemini, OpenAiChat as Chat, OpenAiResponses as Responses,
    };
    let provider = provider.to_ascii_lowercase();
    // These are inference endpoints, deliberately independent of model discovery URLs.
    let (environment, default, suffix, infer_http) = match provider.as_str() {
        "openai" => (
            Some("OPENAI_BASE_URL"),
            "https://api.openai.com/v1",
            "",
            false,
        ),
        "openrouter" => (
            Some("OPENROUTER_BASE_URL"),
            "https://openrouter.ai/api/v1",
            "",
            false,
        ),
        "opencode-zen" => (
            Some("OPENCODE_ZEN_BASE_URL"),
            "https://opencode.ai/zen/v1",
            "",
            false,
        ),
        "anthropic" => (
            Some("ANTHROPIC_BASE_URL"),
            "https://api.anthropic.com/v1",
            "",
            false,
        ),
        "gemini" => (
            Some("GOOGLE_GEMINI_BASE_URL"),
            "https://generativelanguage.googleapis.com/v1beta",
            "",
            false,
        ),
        "ollama" => (Some("OLLAMA_HOST"), "http://localhost:11434", "/v1", true),
        "groq" => (
            Some("GROQ_BASE_URL"),
            "https://api.groq.com/openai/v1",
            "",
            false,
        ),
        "mistral" => (
            Some("MISTRAL_BASE_URL"),
            "https://api.mistral.ai/v1",
            "",
            false,
        ),
        "deepseek" => (
            Some("DEEPSEEK_BASE_URL"),
            "https://api.deepseek.com",
            "",
            false,
        ),
        "xai" => (Some("XAI_BASE_URL"), "https://api.x.ai/v1", "", false),
        "together" => (
            Some("TOGETHER_BASE_URL"),
            "https://api.together.xyz/v1",
            "",
            false,
        ),
        "fireworks" => (
            Some("FIREWORKS_BASE_URL"),
            "https://api.fireworks.ai/inference/v1",
            "",
            false,
        ),
        "cli2key" => (None, "", "", false),
        _ => return Vec::new(),
    };
    let address = if provider == "cli2key" {
        Address::Bridge
    } else {
        Address::Configured {
            environment,
            default,
            suffix,
            infer_http,
        }
    };
    let capabilities: &[(Interface, crate::endpoint::Authentication)] = match provider.as_str() {
        "openai" | "xai" => &[(Responses, Bearer), (Chat, Bearer)],
        "ollama" => &[(Responses, NoAuth), (Chat, NoAuth)],
        "openrouter" => &[(Chat, Bearer), (Responses, Bearer), (Messages, Bearer)],
        "cli2key" => &[(Responses, Bearer), (Messages, ApiKey), (Messages, Bearer)],
        "anthropic" => &[(Messages, ApiKey)],
        "gemini" => &[(Gemini, ApiKey)],
        "groq" | "mistral" | "together" => &[(Chat, Bearer)],
        "fireworks" => &[
            (Responses, Bearer),
            (Chat, Bearer),
            (Messages, Bearer),
            (Messages, ApiKey),
        ],
        "deepseek" => &[(Chat, Bearer), (Messages, ApiKey)],
        "opencode-zen"
            if matches!(
                model,
                "gpt-6-astra"
                    | "gpt-6-sol"
                    | "gpt-6.1-sol"
                    | "gpt-6-luna"
                    | "gpt-5.6-sol"
                    | "gpt-5.1-codex-mini"
                    | "gpt-5"
                    | "gpt-5-codex"
                    | "gpt-5-nano"
                    | "grok-4.7"
                    | "grok-4.6"
                    | "grok-4.5"
                    | "grok-build-0.1"
                    | "muse-spark-1.3"
                    | "muse-spark-1.2"
            ) =>
        {
            &[(Responses, Bearer)]
        }
        "opencode-zen"
            if matches!(
                model,
                "claude-fable-5-1"
                    | "claude-fable-5"
                    | "claude-opus-5-5"
                    | "claude-opus-5"
                    | "claude-opus-4-8"
                    | "claude-opus-4-7"
                    | "claude-opus-4-6"
                    | "claude-opus-4-5"
                    | "claude-sonnet-5"
                    | "claude-sonnet-4-6"
                    | "claude-sonnet-4-5"
                    | "claude-haiku-4-5"
                    | "qwen3.8-flash"
                    | "qwen3.7-max"
                    | "qwen3.7-plus"
                    | "qwen3.6-plus"
                    | "qwen3.5-plus"
            ) =>
        {
            &[(Messages, ApiKey)]
        }
        "opencode-zen"
            if matches!(
                model,
                "gemini-3.8-flash"
                    | "gemini-3.7-flash"
                    | "gemini-3.6-flash"
                    | "gemini-3.5-flash"
                    | "gemini-3.5-flash-lite"
                    | "gemini-3.1-pro"
                    | "gemini-3-flash"
            ) =>
        {
            &[(Gemini, ApiKey)]
        }
        "opencode-zen"
            if matches!(
                model,
                "qwen3.8-max"
                    | "deepseek-v4.1-flash"
                    | "deepseek-v4-pro"
                    | "deepseek-v4-flash"
                    | "deepseek-v4-flash-vision-exp"
                    | "minimax-m3"
                    | "minimax-m2.7"
                    | "minimax-m2.5"
                    | "glm-5.3-flash"
                    | "glm-5.3"
                    | "glm-5.2"
                    | "glm-5.1"
                    | "glm-5"
                    | "kimi-k2.5"
                    | "kimi-k2.6"
                    | "kimi-k2.7-code"
                    | "kimi-k3"
                    | "mistral-large-4"
                    | "big-pickle"
                    | "space-bunny-free"
                    | "longcat-2.5-preview-free"
                    | "exo-free"
                    | "fledge-alpha-free"
                    | "mimo-v2.6-flash-free"
                    | "mimo-v2.5-free"
            ) =>
        {
            &[(Chat, Bearer)]
        }
        _ => &[],
    };
    capabilities
        .iter()
        .map(|&(interface, authentication)| Offer {
            interface,
            authentication,
            // DeepSeek's documented Anthropic API has a different base URL
            // than its OpenAI-compatible API. Keep overrides independent.
            address: if provider == "deepseek" && interface == Messages {
                Address::Configured {
                    environment: Some("DEEPSEEK_ANTHROPIC_BASE_URL"),
                    default: "https://api.deepseek.com/anthropic",
                    suffix: "/v1",
                    infer_http: false,
                }
            } else {
                address.clone()
            },
        })
        .collect()
}

/// Refresh model-specific endpoint metadata where the provider publishes it.
/// Other providers retain their documented interface registry and explicit URL
/// overrides; a model list alone does not establish wire-protocol support.
pub(crate) fn discover_endpoints(
    provider: &str,
    model: &str,
) -> (Vec<crate::endpoint::Offer>, bool, Option<bool>) {
    if provider.eq_ignore_ascii_case("opencode-zen") {
        if let Ok(catalog) = crate::models::fetch_endpoint_catalog() {
            if let Some(offer) = opencode_catalog_offer(&catalog, model) {
                return (
                    vec![offer],
                    false,
                    opencode_catalog_tool_support(&catalog, model),
                );
            }
        }
    }
    (
        offered_endpoints(provider, model),
        provider.eq_ignore_ascii_case("opencode-zen"),
        None,
    )
}

fn opencode_catalog_tool_support(body: &[u8], model: &str) -> Option<bool> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    value
        .get("opencode")?
        .get("models")?
        .get(model)?
        .get("tool_call")?
        .as_bool()
}

/// Replay uses the cassette's recorded wire interface, not a live catalog.
pub(crate) fn replay_endpoints(
    provider: &str,
    model: &str,
    recorded: Option<Interface>,
) -> Vec<crate::endpoint::Offer> {
    let offered = offered_endpoints(provider, model);
    if !provider.eq_ignore_ascii_case("opencode-zen") {
        return offered;
    }
    let Some(interface) = recorded else {
        return offered;
    };
    let Some(mut offer) = offered_endpoints(provider, "gpt-5").into_iter().next() else {
        return offered;
    };
    offer.interface = interface;
    offer.authentication = match interface {
        Interface::AnthropicMessages | Interface::Gemini => crate::endpoint::Authentication::ApiKey,
        _ => crate::endpoint::Authentication::Bearer,
    };
    vec![offer]
}

fn opencode_catalog_offer(body: &[u8], model: &str) -> Option<crate::endpoint::Offer> {
    use crate::endpoint::{Address, Authentication, Interface, Offer};
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    let provider = value.get("opencode")?;
    let item = provider.get("models")?.get(model)?;
    let metadata = item.get("provider");
    let package = metadata
        .and_then(|m| m.get("npm"))
        .or_else(|| provider.get("npm"))?
        .as_str()?;
    let api = metadata
        .and_then(|m| m.get("api"))
        .or_else(|| provider.get("api"))?
        .as_str()?;
    // Catalog data must never redirect credentials to an unrelated host.
    if !api.starts_with("https://opencode.ai/") || api.contains(['?', '#', '\\', '@']) {
        return None;
    }
    let (interface, authentication) = match package {
        "@ai-sdk/openai" => (Interface::OpenAiResponses, Authentication::Bearer),
        "@ai-sdk/openai-compatible" => (Interface::OpenAiChat, Authentication::Bearer),
        "@ai-sdk/anthropic" => (Interface::AnthropicMessages, Authentication::ApiKey),
        "@ai-sdk/google" => (Interface::Gemini, Authentication::ApiKey),
        _ => return None,
    };
    Some(Offer {
        interface,
        authentication,
        address: Address::Discovered {
            environment: Some("OPENCODE_ZEN_BASE_URL"),
            default: api.trim_end_matches('/').to_owned(),
        },
    })
}

pub(crate) fn endpoint_connection(
    provider: &str,
    offer: crate::endpoint::Offer,
    bridge: Option<(&str, &str)>,
    tape: Option<(crate::tape::Mode, &str)>,
) -> Result<crate::endpoint::Connection, crate::Error> {
    let metadata = model_spec(provider).ok_or_else(|| {
        crate::Error::Run(format!(
            "provider `{provider}` has no configured API endpoint"
        ))
    })?;
    let endpoint = if let Some((_, url)) = tape {
        url.to_owned()
    } else {
        endpoint_address(provider, offer.address, bridge.map(|(_, url)| url))?
    };
    let credential = if tape.is_some_and(|(mode, _)| mode == crate::tape::Mode::Replay) {
        Some("asb-replay".to_owned())
    } else if let Some((key, _)) = bridge {
        Some(key.to_owned())
    } else if let Some(variable) = metadata.credential {
        Some(credential(provider).ok_or_else(|| crate::Error::Run(format!(
            "provider `{provider}` requires a credential; set {variable} before starting this run"
        )))?)
    } else {
        None
    };
    Ok(crate::endpoint::Connection {
        interface: offer.interface,
        base_url: endpoint,
        authentication: offer.authentication,
        credential,
    })
}

fn endpoint_address(
    provider: &str,
    address: crate::endpoint::Address,
    bridge: Option<&str>,
) -> Result<String, crate::Error> {
    match address {
        crate::endpoint::Address::Bridge => bridge.map(str::to_owned).ok_or_else(|| {
            crate::Error::Run(format!(
                "provider `{provider}` requires its local bridge to be started"
            ))
        }),
        crate::endpoint::Address::Configured {
            environment,
            default,
            suffix,
            infer_http,
        } => {
            let mut base = environment
                .and_then(std::env::var_os)
                .map(|v| v.to_string_lossy().into_owned())
                .unwrap_or_else(|| default.to_owned());
            if infer_http && !base.contains("://") {
                base = format!("http://{base}");
            }
            let base = base.trim_end_matches('/');
            Ok(if suffix.is_empty() || base.ends_with(suffix) {
                base.to_owned()
            } else {
                format!("{base}{suffix}")
            })
        }
        crate::endpoint::Address::Discovered {
            environment,
            default,
        } => Ok(environment
            .and_then(std::env::var_os)
            .map(|v| v.to_string_lossy().trim_end_matches('/').to_owned())
            .unwrap_or(default)),
    }
}

/// The proxy receives /v1/... paths; retain the selected API's path prefix.
pub(crate) fn selected_tape_upstream(
    provider: &str,
    offer: crate::endpoint::Offer,
    bridge: Option<&str>,
) -> Result<String, crate::Error> {
    endpoint_address(provider, offer.address, bridge).map(|url| strip_api_version(&url))
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
        command.env_remove("ASB_AGENT_API_KEY");
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
                ["/api/tags", "/models", "/tags"]
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
    #[test]
    fn fireworks_account_is_derived_only_from_complete_model_resources() {
        assert_eq!(
            super::fireworks_model_account("accounts/my-account/models/model"),
            Some("my-account")
        );
        assert_eq!(
            super::fireworks_model_account("accounts/my-account/models/"),
            None
        );
        assert_eq!(
            super::fireworks_model_account("accounts/my-account/models/model/extra"),
            None
        );
        assert_eq!(super::fireworks_model_account("unqualified-model"), None);
    }
    use super::{ProviderSpec, configure_tape, read_key_file, spec, tape_upstream};
    use crate::tape::Mode;
    use std::fs;

    #[test]
    fn deepseek_offers_documented_chat_and_anthropic_endpoints() {
        use crate::endpoint::{Address, Authentication, Interface};
        let offers = super::offered_endpoints("deepseek", "deepseek-flash");
        assert!(
            offers
                .iter()
                .any(|offer| offer.interface == Interface::OpenAiChat
                    && offer.authentication == Authentication::Bearer)
        );
        let messages = offers
            .iter()
            .find(|offer| offer.interface == Interface::AnthropicMessages)
            .unwrap();
        assert_eq!(messages.authentication, Authentication::ApiKey);
        assert_eq!(
            messages.address,
            Address::Configured {
                environment: Some("DEEPSEEK_ANTHROPIC_BASE_URL"),
                default: "https://api.deepseek.com/anthropic",
                suffix: "/v1",
                infer_http: false
            }
        );
    }

    #[test]
    fn gemini_catalog_url_does_not_repeat_api_version() {
        let spec = super::ModelSpec {
            name: "gemini",
            credential: None,
            endpoint: None,
            default_url: Some("https://example.invalid/v1beta"),
            kind: super::ModelKind::Gemini,
        };
        assert_eq!(
            spec.url().as_deref(),
            Some("https://example.invalid/v1beta/models")
        );
    }

    #[test]
    fn opencode_catalog_reads_explicit_model_tool_support() {
        let catalog = br#"{"opencode":{"models":{"yes":{"tool_call":true},"no":{"tool_call":false},"unknown":{}}}}"#;
        assert_eq!(
            super::opencode_catalog_tool_support(catalog, "yes"),
            Some(true)
        );
        assert_eq!(
            super::opencode_catalog_tool_support(catalog, "no"),
            Some(false)
        );
        assert_eq!(
            super::opencode_catalog_tool_support(catalog, "unknown"),
            None
        );
    }

    #[test]
    fn opencode_model_catalog_selects_current_interface_and_address() {
        use crate::endpoint::{Address, Authentication, Interface};
        let catalog = br#"{"opencode":{"api":"https://opencode.ai/inference/openai/v1","npm":"@ai-sdk/openai-compatible","models":{"new-gpt":{"provider":{"npm":"@ai-sdk/openai","api":"https://opencode.ai/inference/openai/v2"}},"new-claude":{"provider":{"npm":"@ai-sdk/anthropic","api":"https://opencode.ai/inference/anthropic/v1"}},"new-chat":{},"bad":{"provider":{"npm":"@ai-sdk/openai","api":"https://opencode.ai.evil.test/v1"}}}}}"#;
        let gpt = super::opencode_catalog_offer(catalog, "new-gpt").unwrap();
        assert_eq!(gpt.interface, Interface::OpenAiResponses);
        assert_eq!(gpt.authentication, Authentication::Bearer);
        assert_eq!(
            gpt.address,
            Address::Discovered {
                environment: Some("OPENCODE_ZEN_BASE_URL"),
                default: "https://opencode.ai/inference/openai/v2".into()
            }
        );
        assert_eq!(
            super::opencode_catalog_offer(catalog, "new-claude")
                .unwrap()
                .interface,
            Interface::AnthropicMessages
        );
        assert_eq!(
            super::opencode_catalog_offer(catalog, "new-chat")
                .unwrap()
                .interface,
            Interface::OpenAiChat
        );
        assert!(super::opencode_catalog_offer(catalog, "bad").is_none());
        assert!(super::opencode_catalog_offer(catalog, "not-listed").is_none());
        assert!(super::opencode_catalog_offer(b"invalid", "new-gpt").is_none());
    }

    #[test]
    fn opencode_replay_uses_recorded_interface_for_new_or_changed_models() {
        use crate::endpoint::Interface;
        for interface in [
            Interface::OpenAiResponses,
            Interface::OpenAiChat,
            Interface::AnthropicMessages,
        ] {
            let offers = super::replay_endpoints(
                "opencode-zen",
                "new-model-not-in-snapshot",
                Some(interface),
            );
            assert_eq!(offers.len(), 1);
            assert_eq!(offers[0].interface, interface);
        }
    }

    #[test]
    fn responses_connections_route_tape_and_reject_incompatible_providers() {
        let live = super::endpoint_connection(
            "cli2key",
            super::offered_endpoints("cli2key", "fixture")[0].clone(),
            Some(("fixture-key", "http://127.0.0.1:10/v1")),
            None,
        )
        .unwrap();
        assert_eq!(live.base_url, "http://127.0.0.1:10/v1");
        assert_eq!(live.credential.as_deref(), Some("fixture-key"));
        for provider in ["cli2key", "openrouter", "openai", "ollama", "xai"] {
            let connection = super::endpoint_connection(
                provider,
                super::offered_endpoints(provider, "fixture")[0].clone(),
                Some(("live-secret", "http://127.0.0.1:10/v1")),
                Some((Mode::Replay, "http://127.0.0.1:20/v1")),
            )
            .unwrap();
            assert_eq!(connection.base_url, "http://127.0.0.1:20/v1");
            assert_eq!(connection.credential.as_deref(), Some("asb-replay"));
        }
        let offered = super::offered_endpoints("anthropic", "test-model");
        assert_eq!(
            crate::endpoint::select(&[crate::endpoint::RESPONSES], &offered),
            None
        );
        assert_eq!(
            super::offered_endpoints("opencode-zen", "gpt-5")[0].interface,
            crate::endpoint::Interface::OpenAiResponses
        );
        assert!(super::offered_endpoints("opencode-zen", "unknown").is_empty());
    }

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
