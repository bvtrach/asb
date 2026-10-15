//! Small provider registry for environment-driven integrations.

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

/// Add provider-neutral and provider-specific selection variables to a child.
///
/// The child inherits the complete parent environment, so credential values
/// are never read or copied by ASB. Only non-secret selection metadata is set.
pub fn configure(command: &mut std::process::Command, provider: &str, model: &str) {
    let selected = spec(provider);
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
    use super::{ProviderSpec, spec};

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
}
