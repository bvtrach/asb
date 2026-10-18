//! Agent/provider-independent negotiation of API interfaces.

/// A wire-level API, not a provider or an agent identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Interface {
    OpenAiResponses,
    OpenAiChat,
    AnthropicMessages,
    Gemini,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Authentication {
    Bearer,
    ApiKey,
    None,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Support {
    pub interface: Interface,
    pub authentication: &'static [Authentication],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Offer {
    pub interface: Interface,
    pub authentication: Authentication,
    pub address: Address,
}

/// Provider-owned address information for a particular offered interface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Address {
    Configured {
        environment: Option<&'static str>,
        default: &'static str,
        suffix: &'static str,
        infer_http: bool,
    },
    Discovered {
        environment: Option<&'static str>,
        default: String,
    },
    Bridge,
}

pub(crate) const RESPONSES: Support = Support {
    interface: Interface::OpenAiResponses,
    authentication: &[Authentication::Bearer, Authentication::None],
};
pub(crate) const CHAT: Support = Support {
    interface: Interface::OpenAiChat,
    authentication: &[Authentication::Bearer, Authentication::None],
};
pub(crate) const MESSAGES_KEY: Support = Support {
    interface: Interface::AnthropicMessages,
    authentication: &[Authentication::ApiKey, Authentication::None],
};
pub(crate) const MESSAGES_BOTH: Support = Support {
    interface: Interface::AnthropicMessages,
    authentication: &[
        Authentication::Bearer,
        Authentication::ApiKey,
        Authentication::None,
    ],
};

/// Select the first agent-preferred interface also offered by the provider.
/// Endpoint URL, authentication and model eligibility are provider-owned and
/// must already be reflected in the offered list.
pub(crate) fn select(supported: &[Support], offered: &[Offer]) -> Option<Offer> {
    supported.iter().find_map(|agent| {
        offered
            .iter()
            .find(|provider| {
                agent.interface == provider.interface
                    && agent.authentication.contains(&provider.authentication)
            })
            .cloned()
    })
}

pub(crate) struct Connection {
    pub interface: Interface,
    pub base_url: String,
    pub credential: Option<String>,
    pub authentication: Authentication,
}

/// Negotiate without branching on either an agent or a provider identifier.
#[cfg(test)]
pub(crate) fn negotiate(
    agent: &str,
    supported: &[Support],
    provider: &str,
    model: &str,
    bridge: Option<(&str, &str)>,
    tape: Option<(crate::tape::Mode, &str)>,
) -> Result<Connection, crate::Error> {
    let endpoint = choose(agent, supported, provider, model)?;
    crate::providers::endpoint_connection(provider, endpoint, bridge, tape)
}

/// Match static capabilities without starting a bridge or contacting a model
/// catalog. This can report an incompatible combination before side effects.
#[cfg(test)]
pub(crate) fn choose(
    agent: &str,
    supported: &[Support],
    provider: &str,
    model: &str,
) -> Result<Offer, crate::Error> {
    let offered = crate::providers::offered_endpoints(provider, model);
    choose_from(agent, supported, provider, model, &offered)
}

pub(crate) fn choose_from(
    agent: &str,
    supported: &[Support],
    provider: &str,
    model: &str,
    offered: &[Offer],
) -> Result<Offer, crate::Error> {
    select(supported, offered).ok_or_else(|| crate::Error::Run(format!(
        "agent `{agent}` and provider `{provider}` have no compatible endpoint for model `{model}` (agent supports {supported:?}; provider offers {offered:?}); choose a compatible agent, provider, or model"
    )))
}

#[cfg(test)]
mod tests {
    use super::{
        Authentication::{ApiKey, Bearer},
        Interface::*,
        Offer, Support, select,
    };

    const ADDRESS: super::Address = super::Address::Configured {
        environment: None,
        default: "http://example.invalid/v1",
        suffix: "",
        infer_http: false,
    };

    #[test]
    fn intersection_is_deterministic_and_fails_when_incompatible() {
        assert_eq!(
            select(
                &[super::CHAT, super::RESPONSES],
                &[
                    Offer {
                        interface: OpenAiResponses,
                        authentication: Bearer,
                        address: ADDRESS,
                    },
                    Offer {
                        interface: OpenAiChat,
                        authentication: Bearer,
                        address: ADDRESS,
                    }
                ]
            ),
            Some(Offer {
                interface: OpenAiChat,
                authentication: Bearer,
                address: ADDRESS,
            })
        );
        assert_eq!(
            select(
                &[super::MESSAGES_KEY],
                &[Offer {
                    interface: OpenAiChat,
                    authentication: Bearer,
                    address: ADDRESS,
                }]
            ),
            None
        );
        assert_eq!(
            select(
                &[],
                &[Offer {
                    interface: OpenAiChat,
                    authentication: Bearer,
                    address: ADDRESS,
                }]
            ),
            None
        );
        assert_eq!(select(&[super::CHAT], &[]), None);
        let gemini = Support {
            interface: Gemini,
            authentication: &[ApiKey],
        };
        assert_eq!(
            select(
                &[gemini],
                &[Offer {
                    interface: Gemini,
                    authentication: ApiKey,
                    address: ADDRESS,
                }]
            ),
            Some(Offer {
                interface: Gemini,
                authentication: ApiKey,
                address: ADDRESS,
            })
        );
        assert_eq!(
            select(
                &[super::MESSAGES_KEY],
                &[Offer {
                    interface: AnthropicMessages,
                    authentication: Bearer,
                    address: ADDRESS,
                }]
            ),
            None
        );
    }

    #[test]
    fn incompatible_pair_explains_the_agent_provider_model_and_interfaces() {
        let error = super::negotiate(
            "fixture-agent",
            &[super::RESPONSES],
            "anthropic",
            "example-model",
            None,
            None,
        )
        .err()
        .unwrap();
        let message = error.to_string();
        for value in [
            "fixture-agent",
            "anthropic",
            "example-model",
            "OpenAiResponses",
            "AnthropicMessages",
        ] {
            assert!(message.contains(value), "{message}");
        }
    }

    #[test]
    fn selected_offer_retains_its_own_address() {
        let chat = Offer {
            interface: OpenAiChat,
            authentication: super::Authentication::None,
            address: super::Address::Configured {
                environment: None,
                default: "http://chat.invalid/api",
                suffix: "",
                infer_http: false,
            },
        };
        let responses = Offer {
            interface: OpenAiResponses,
            authentication: super::Authentication::None,
            address: super::Address::Configured {
                environment: None,
                default: "http://responses.invalid/v2",
                suffix: "",
                infer_http: false,
            },
        };
        let selected = select(
            &[super::RESPONSES, super::CHAT],
            &[chat.clone(), responses.clone()],
        )
        .unwrap();
        let connection =
            crate::providers::endpoint_connection("ollama", selected, None, None).unwrap();
        assert_eq!(connection.base_url, "http://responses.invalid/v2");
        let selected = select(&[super::CHAT], &[chat, responses]).unwrap();
        let connection =
            crate::providers::endpoint_connection("ollama", selected, None, None).unwrap();
        assert_eq!(connection.base_url, "http://chat.invalid/api");
    }
}
