//! Ollama model catalog enrichment.
use crate::models::{curl_config, escape_config, fetch_config};
use crate::{Error, providers};
use serde_json::Value;

// Older Ollama versions omit capabilities from /api/tags. Query the model's
// metadata rather than assuming that every local model supports tools.
pub(in crate::providers) fn enrich_catalog(
    body: &[u8],
    spec: providers::ModelSpec,
) -> Result<Vec<u8>, Error> {
    let mut catalog: Value = serde_json::from_slice(body)
        .map_err(|_| Error::Input("provider returned invalid model JSON".into()))?;
    let entries = catalog
        .get_mut("models")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| Error::Input("provider response has no model list".into()))?;
    let tags_url = spec
        .url()
        .ok_or_else(|| Error::Input("provider model endpoint is not configured".into()))?;
    let show_url = tags_url
        .strip_suffix("/api/tags")
        .map(|base| format!("{base}/api/show"))
        .ok_or_else(|| Error::Input("Ollama model endpoint must end in /api/tags".into()))?;
    for entry in entries {
        if entry.get("capabilities").is_some() {
            continue;
        }
        let Some(name) = entry
            .get("name")
            .or_else(|| entry.get("id"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        let payload = serde_json::json!({"model": name}).to_string();
        let mut config = curl_config(spec, &show_url, None)?;
        config.push_str("header = \"Content-Type: application/json\"\n");
        config.push_str(&format!("data = \"{}\"\n", escape_config(&payload)));
        let response = fetch_config(config, spec.name)?;
        let metadata: Value = serde_json::from_slice(&response).map_err(|_| {
            Error::Input(format!(
                "Ollama returned invalid metadata for model `{name}`"
            ))
        })?;
        if let Some(capabilities) = metadata.get("capabilities") {
            entry["capabilities"] = capabilities.clone();
        }
    }
    serde_json::to_vec(&catalog)
        .map_err(|error| Error::Input(format!("cannot read Ollama catalog: {error}")))
}
