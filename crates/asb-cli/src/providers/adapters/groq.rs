//! Adapter for groq.
use super::*;

fixed_offers!(
    groq_offers,
    Some("GROQ_BASE_URL"),
    "https://api.groq.com/openai/v1",
    "",
    false,
    &[(Chat, Bearer)]
);

adapter!(
    "groq",
    Some("GROQ_API_KEY"),
    Some("GROQ_BASE_URL"),
    None,
    None,
    &["GROQ_BASE_URL"],
    Some("GROQ_BASE_URL"),
    Some("https://api.groq.com/openai/v1/models"),
    DataUnpaginated,
    "llama-3.3-70b-versatile",
    groq_offers
);
