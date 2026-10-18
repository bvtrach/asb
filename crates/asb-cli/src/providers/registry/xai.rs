//! Adapter for xai.
use super::*;

fixed_offers!(
    xai_offers,
    Some("XAI_BASE_URL"),
    "https://api.x.ai/v1",
    "",
    false,
    &[(Responses, Bearer), (Chat, Bearer)]
);

adapter!(
    xai,
    "xai",
    Some("XAI_API_KEY"),
    Some("XAI_BASE_URL"),
    None,
    None,
    &["XAI_BASE_URL"],
    Some("XAI_BASE_URL"),
    Some("https://api.x.ai/v1/models"),
    DataUnpaginated,
    "grok-3-mini",
    xai_offers
);
