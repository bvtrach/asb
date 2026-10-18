//! Adapter for opencode-zen.
use super::*;

fn opencode_offers(model: &str) -> Vec<Offer> {
    let capabilities: &[(Interface, Authentication)] = match model {
        "gpt-6-astra" | "gpt-6-sol" | "gpt-6.1-sol" | "gpt-6-luna" | "gpt-5.6-sol"
        | "gpt-5.1-codex-mini" | "gpt-5" | "gpt-5-codex" | "gpt-5-nano" | "grok-4.7"
        | "grok-4.6" | "grok-4.5" | "grok-build-0.1" | "muse-spark-1.3" | "muse-spark-1.2" => {
            &[(Responses, Bearer)]
        }
        "claude-fable-5-1" | "claude-fable-5" | "claude-opus-5-5" | "claude-opus-5"
        | "claude-opus-4-8" | "claude-opus-4-7" | "claude-opus-4-6" | "claude-opus-4-5"
        | "claude-sonnet-5" | "claude-sonnet-4-6" | "claude-sonnet-4-5" | "claude-haiku-4-5"
        | "qwen3.8-flash" | "qwen3.7-max" | "qwen3.7-plus" | "qwen3.6-plus" | "qwen3.5-plus" => {
            &[(Messages, ApiKey)]
        }
        "gemini-3.8-flash"
        | "gemini-3.7-flash"
        | "gemini-3.6-flash"
        | "gemini-3.5-flash"
        | "gemini-3.5-flash-lite"
        | "gemini-3.1-pro"
        | "gemini-3-flash" => &[(Gemini, ApiKey)],
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
        | "mimo-v2.5-free" => &[(Chat, Bearer)],
        _ => &[],
    };
    offers(
        Address::Configured {
            environment: Some("OPENCODE_ZEN_BASE_URL"),
            default: "https://opencode.ai/zen/v1",
            suffix: "",
            infer_http: false,
        },
        capabilities,
    )
}

adapter!(
    opencode_zen,
    "opencode-zen",
    Some("OPENCODE_API_KEY"),
    Some("OPENCODE_ZEN_BASE_URL"),
    None,
    None,
    &["OPENCODE_ZEN_BASE_URL"],
    Some("OPENCODE_ZEN_BASE_URL"),
    Some("https://opencode.ai/zen/v1/models"),
    DataUnpaginated,
    "mimo-v2.6-flash-free",
    opencode_offers,
    fn discover_endpoints(&self, model: &str) -> (Vec<Offer>, bool, Option<bool>) {
        if let Ok(catalog) = crate::providers::opencode_zen::fetch_endpoint_catalog() {
            if let Some(offer) = crate::providers::opencode_zen::catalog_offer(&catalog, model) {
                return (
                    vec![offer],
                    false,
                    crate::providers::opencode_zen::catalog_tool_support(&catalog, model),
                );
            }
        }
        (self.offered_endpoints(model), true, None)
    },
    fn replay_endpoints(&self, model: &str, recorded: Option<Interface>) -> Vec<Offer> {
        let offered = self.offered_endpoints(model);
        let Some(interface) = recorded else {
            return offered;
        };
        let Some(mut offer) = opencode_offers("gpt-5").into_iter().next() else {
            return offered;
        };
        offer.interface = interface;
        offer.authentication = match interface {
            Interface::AnthropicMessages | Interface::Gemini => Authentication::ApiKey,
            _ => Authentication::Bearer,
        };
        vec![offer]
    },
    fn public_catalog(&self, _credential_present: bool) -> bool {
        true
    },
    fn supports_free(&self) -> bool {
        true
    },
    fn free_model(&self, _entry: &serde_json::Value, id: &str) -> bool {
        id.ends_with("-free")
    }
);
