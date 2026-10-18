//! Adapter for openai.
use super::*;

fixed_offers!(
    openai_offers,
    Some("OPENAI_BASE_URL"),
    "https://api.openai.com/v1",
    "",
    false,
    &[(Responses, Bearer), (Chat, Bearer)]
);

adapter!(
    openai,
    "openai",
    Some("OPENAI_API_KEY"),
    Some("OPENAI_BASE_URL"),
    Some("OPENAI_MODEL"),
    None,
    &["OPENAI_BASE_URL", "OPENAI_API_BASE"],
    Some("OPENAI_BASE_URL"),
    Some("https://api.openai.com/v1/models"),
    DataUnpaginated,
    "gpt-4o-mini",
    openai_offers
);
