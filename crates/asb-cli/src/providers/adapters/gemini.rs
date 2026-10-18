//! Adapter for gemini.
use super::*;

fixed_offers!(
    gemini_offers,
    Some("GOOGLE_GEMINI_BASE_URL"),
    "https://generativelanguage.googleapis.com/v1beta",
    "",
    false,
    &[(Gemini, ApiKey)]
);

adapter!(
    "gemini",
    Some("GEMINI_API_KEY"),
    Some("GOOGLE_GEMINI_BASE_URL"),
    None,
    None,
    &["GOOGLE_GEMINI_BASE_URL"],
    Some("GOOGLE_GEMINI_BASE_URL"),
    Some("https://generativelanguage.googleapis.com/v1beta/models"),
    ModelsPageTokenApiKey,
    "test-model",
    gemini_offers,
    fn catalog_headers(&self, key: &str) -> Vec<String> {
        vec![format!("x-goog-api-key: {key}")]
    },
    fn tool_support(&self, entry: &serde_json::Value) -> Option<bool> {
        let methods = entry.get("supportedGenerationMethods")?.as_array()?;
        (!methods
            .iter()
            .any(|value| value.as_str() == Some("generateContent")))
        .then_some(false)
    },
    fn catalog_suffix(&self, base: &str) -> &'static str {
        if base.ends_with("/v1beta") || base.ends_with("/v1") {
            "/models"
        } else {
            "/v1beta/models"
        }
    }
);
