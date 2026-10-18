//! Adapter for mistral.
use super::*;

fixed_offers!(
    mistral_offers,
    Some("MISTRAL_BASE_URL"),
    "https://api.mistral.ai/v1",
    "",
    false,
    &[(Chat, Bearer)]
);

adapter!(
    "mistral",
    Some("MISTRAL_API_KEY"),
    Some("MISTRAL_BASE_URL"),
    None,
    None,
    &["MISTRAL_BASE_URL"],
    Some("MISTRAL_BASE_URL"),
    Some("https://api.mistral.ai/v1/models"),
    DataUnpaginated,
    "mistral-small-latest",
    mistral_offers
);
