//! Adapter for openrouter.
use super::*;

fixed_offers!(
    openrouter_offers,
    Some("OPENROUTER_BASE_URL"),
    "https://openrouter.ai/api/v1",
    "",
    false,
    &[(Chat, Bearer), (Responses, Bearer), (Messages, Bearer)]
);

adapter!(
    openrouter,
    "openrouter",
    Some("OPENROUTER_API_KEY"),
    Some("OPENROUTER_BASE_URL"),
    Some("OPENROUTER_MODEL"),
    None,
    &["OPENROUTER_BASE_URL"],
    Some("OPENROUTER_BASE_URL"),
    Some("https://openrouter.ai/api/v1/models"),
    DataUnpaginated,
    "openai/gpt-4o-mini",
    openrouter_offers,
    fn prepare(
        &self,
        command: &mut std::process::Command,
        model: &str,
        _bridge: Option<(&str, &str)>,
        tape: Option<&str>,
        replay: bool,
    ) -> Result<(), crate::Error> {
        crate::providers::openrouter::prepare(command, model, tape, replay)
    },
    fn public_catalog(&self, credential_present: bool) -> bool {
        !credential_present
    },
    fn supports_free(&self) -> bool {
        true
    },
    fn free_model(&self, entry: &serde_json::Value, id: &str) -> bool {
        id.ends_with(":free")
            || entry
                .get("pricing")
                .and_then(serde_json::Value::as_object)
                .is_some_and(|pricing| {
                    crate::models::is_zero_price(pricing.get("prompt"))
                        && crate::models::is_zero_price(pricing.get("completion"))
                })
    }
);
