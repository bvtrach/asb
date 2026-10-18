//! Adapter for anthropic.
use super::*;

fixed_offers!(
    anthropic_offers,
    Some("ANTHROPIC_BASE_URL"),
    "https://api.anthropic.com/v1",
    "",
    false,
    &[(Messages, ApiKey)]
);

adapter!(
    "anthropic",
    Some("ANTHROPIC_API_KEY"),
    Some("ANTHROPIC_BASE_URL"),
    None,
    None,
    &[],
    Some("ANTHROPIC_BASE_URL"),
    Some("https://api.anthropic.com/v1/models"),
    DataCursor,
    "claude-3-5-sonnet-latest",
    anthropic_offers,
    fn tape_root_environment(&self) -> &'static [&'static str] {
        &["ANTHROPIC_BASE_URL"]
    },
    fn catalog_headers(&self, key: &str) -> Vec<String> {
        vec![
            format!("x-api-key: {key}"),
            "anthropic-version: 2023-06-01".into(),
        ]
    },
    fn default_tool_support(&self) -> Option<bool> {
        Some(true)
    }
);
