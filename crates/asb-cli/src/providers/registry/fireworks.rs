//! Adapter for fireworks.
use super::*;

fixed_offers!(
    fireworks_offers,
    Some("FIREWORKS_BASE_URL"),
    "https://api.fireworks.ai/inference/v1",
    "",
    false,
    &[
        (Responses, Bearer),
        (Chat, Bearer),
        (Messages, Bearer),
        (Messages, ApiKey)
    ]
);

adapter!(
    fireworks,
    "fireworks",
    Some("FIREWORKS_API_KEY"),
    Some("FIREWORKS_BASE_URL"),
    None,
    None,
    &["FIREWORKS_BASE_URL"],
    Some("FIREWORKS_MODELS_URL"),
    Some("https://api.fireworks.ai/v1/accounts/fireworks/models"),
    ModelsPageTokenBearer,
    "accounts/fireworks/models/llama-v3p1-8b-instruct",
    fireworks_offers,
    fn catalog_base(&self, selected: Option<&str>, _fallback: Option<&str>) -> Option<String> {
        crate::providers::fireworks::catalog_base(selected)
    },
    fn catalog_url_error(&self) -> Option<&'static str> {
        std::env::var_os("FIREWORKS_ACCOUNT_ID").map(|_| "FIREWORKS_ACCOUNT_ID must be a nonempty account ID containing only letters, digits, underscores or hyphens")
    }
);
