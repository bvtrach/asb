//! Adapter for deepseek.
use super::*;

fn deepseek_offers(_: &str) -> Vec<Offer> {
    let mut result = offers(
        Address::Configured {
            environment: Some("DEEPSEEK_BASE_URL"),
            default: "https://api.deepseek.com",
            suffix: "",
            infer_http: false,
        },
        &[(Chat, Bearer)],
    );
    result.extend(offers(
        Address::Configured {
            environment: Some("DEEPSEEK_ANTHROPIC_BASE_URL"),
            default: "https://api.deepseek.com/anthropic",
            suffix: "/v1",
            infer_http: false,
        },
        &[(Messages, ApiKey)],
    ));
    result
}

adapter!(
    deepseek,
    "deepseek",
    Some("DEEPSEEK_API_KEY"),
    Some("DEEPSEEK_BASE_URL"),
    None,
    None,
    &["DEEPSEEK_BASE_URL", "DEEPSEEK_ANTHROPIC_BASE_URL"],
    Some("DEEPSEEK_BASE_URL"),
    Some("https://api.deepseek.com/models"),
    DataUnpaginated,
    "deepseek-chat",
    deepseek_offers
);
