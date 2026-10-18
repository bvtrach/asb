//! Adapter for ollama.
pub(in crate::providers) mod implementation;
use super::*;

fixed_offers!(
    ollama_offers,
    Some("OLLAMA_HOST"),
    "http://localhost:11434",
    "/v1",
    true,
    &[(Responses, NoAuth), (Chat, NoAuth)]
);

adapter!(
    "ollama",
    None,
    Some("OLLAMA_HOST"),
    Some("OLLAMA_MODEL"),
    None,
    &["OLLAMA_HOST"],
    Some("OLLAMA_HOST"),
    Some("http://localhost:11434/api/tags"),
    ModelsUnpaginated,
    "llama3.2",
    ollama_offers,
    fn supports_free(&self) -> bool {
        true
    },
    fn free_model(&self, _entry: &serde_json::Value, _id: &str) -> bool {
        true
    },
    fn tool_support(&self, entry: &serde_json::Value) -> Option<bool> {
        entry
            .get("capabilities")?
            .as_array()
            .map(|values| values.iter().any(|value| value.as_str() == Some("tools")))
    },
    fn enrich_catalog(&self, body: Vec<u8>) -> Result<Vec<u8>, crate::Error> {
        implementation::enrich_catalog(&body, self.model_spec())
    },
    fn catalog_normalize(&self, base: String) -> String {
        if base.contains("://") {
            base
        } else {
            format!("http://{base}")
        }
    },
    fn catalog_suffix(&self, _base: &str) -> &'static str {
        "/api/tags"
    }
);
