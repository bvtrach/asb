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

#[cfg(test)]
mod tests {
    #[test]
    fn deepseek_offers_documented_chat_and_anthropic_endpoints() {
        use crate::endpoint::{Address, Authentication, Interface};
        let offers = crate::providers::offered_endpoints("deepseek", "deepseek-flash");
        assert!(
            offers
                .iter()
                .any(|offer| offer.interface == Interface::OpenAiChat
                    && offer.authentication == Authentication::Bearer)
        );
        let messages = offers
            .iter()
            .find(|offer| offer.interface == Interface::AnthropicMessages)
            .unwrap();
        assert_eq!(messages.authentication, Authentication::ApiKey);
        assert_eq!(
            messages.address,
            Address::Configured {
                environment: Some("DEEPSEEK_ANTHROPIC_BASE_URL"),
                default: "https://api.deepseek.com/anthropic",
                suffix: "/v1",
                infer_http: false
            }
        );
    }
}
