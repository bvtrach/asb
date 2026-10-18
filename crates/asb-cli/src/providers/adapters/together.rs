//! Adapter for together.
use super::*;

fixed_offers!(
    together_offers,
    Some("TOGETHER_BASE_URL"),
    "https://api.together.xyz/v1",
    "",
    false,
    &[(Chat, Bearer)]
);

adapter!(
    "together",
    Some("TOGETHER_API_KEY"),
    Some("TOGETHER_BASE_URL"),
    None,
    None,
    &["TOGETHER_BASE_URL"],
    Some("TOGETHER_BASE_URL"),
    Some("https://api.together.ai/v1/models"),
    ArrayUnpaginated,
    "meta-llama/Llama-3.3-70B-Instruct-Turbo",
    together_offers
);
