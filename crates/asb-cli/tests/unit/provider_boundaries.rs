//! Pure provider routing and private fake-environment boundaries, never HTTP.
use super::*;
use crate::endpoint::{Address, Authentication, Offer};

#[test]
fn unknown_catalog_delegation_does_not_guess_or_start_a_provider() {
    assert_eq!(
        enrich_catalog("not-registered", b"unchanged".to_vec()).unwrap(),
        b"unchanged"
    );
    assert!(catalog_url_error("not-registered").is_none());
    assert_eq!(
        catalog_headers("not-registered", "dummy"),
        ["Authorization: Bearer dummy"]
    );
    assert!(discovery_session("not-registered").unwrap().is_none());
    assert_eq!(
        discover_endpoints("not-registered", "model"),
        (Vec::new(), false, None)
    );
    assert_eq!(
        agent_provider_name("not-registered", Some(Interface::AnthropicMessages)),
        "anthropic"
    );
    let spec = ModelSpec {
        name: "not-registered",
        credential: None,
        endpoint: None,
        default_url: Some("https://fixture.invalid/v1"),
        kind: ModelKind::DataUnpaginated,
    };
    assert_eq!(
        spec.url().as_deref(),
        Some("https://fixture.invalid/v1/models")
    );
    let spec = ModelSpec {
        default_url: None,
        ..spec
    };
    assert!(spec.url().is_none());
}

#[test]
fn endpoint_address_respects_explicit_family_and_requires_bridge_when_declared() {
    assert!(
        endpoint_address("fixture", Address::Bridge, None)
            .unwrap_err()
            .to_string()
            .contains("requires its local bridge")
    );
    assert_eq!(
        endpoint_address("fixture", Address::Bridge, Some("http://127.0.0.1:1/v1")).unwrap(),
        "http://127.0.0.1:1/v1"
    );
    assert_eq!(
        endpoint_address(
            "fixture",
            Address::Configured {
                environment: None,
                default: "localhost:123",
                suffix: "/v1",
                infer_http: true
            },
            None
        )
        .unwrap(),
        "http://localhost:123/v1"
    );
    assert_eq!(
        endpoint_address(
            "fixture",
            Address::Discovered {
                environment: None,
                default: "http://127.0.0.1:1/custom".into()
            },
            None
        )
        .unwrap(),
        "http://127.0.0.1:1/custom"
    );
    let offer = Offer {
        interface: Interface::OpenAiChat,
        authentication: Authentication::Bearer,
        address: Address::Bridge,
    };
    assert!(
        endpoint_connection("not-registered", offer, None, None)
            .err()
            .expect("an unknown provider must fail without starting a session")
            .to_string()
            .contains("no configured API endpoint")
    );
}

#[test]
fn credential_input_paths_and_fireworks_account_errors_use_only_private_fixture_environment() {
    use std::process::Command;
    const CHILD: &str = "ASB_TEST_PROVIDER_BOUNDARY";
    if std::env::var_os(CHILD).is_some() {
        let spec = ProviderSpec {
            name: "fixture",
            credential: Some("FIXTURE_KEY"),
            endpoint: None,
            model_env: None,
            tape_endpoint_envs: &[],
            bridge_tool: None,
        };
        assert_eq!(
            credential_inputs(&spec),
            vec![crate::runtime_inputs::Dependency::file(
                Path::new(&std::env::var_os("HOME").unwrap()).join(".api_key_fixture")
            )]
        );
        assert!(
            credential_inputs(&ProviderSpec {
                credential: None,
                ..spec
            })
            .is_empty()
        );
        assert!(adapters::fireworks::implementation::catalog_base(None).is_none());
        assert!(
            catalog_url_error("fireworks")
                .unwrap()
                .contains("FIREWORKS_ACCOUNT_ID")
        );
        return;
    }
    let root = std::env::temp_dir().join(format!(
        "asb-provider-boundary-{}",
        crate::workloads::TaskIdentity::local("boundary", None).attempt_id
    ));
    std::fs::create_dir_all(&root).unwrap();
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.args(["--exact","providers::boundary_tests::credential_input_paths_and_fireworks_account_errors_use_only_private_fixture_environment"]).env_clear().env(CHILD,"1").env("HOME",&root).env("FIREWORKS_ACCOUNT_ID","../invalid/account").current_dir(&root);
    if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
        command.env("LLVM_PROFILE_FILE", profile);
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn credential_file_directory_is_not_a_key() {
    assert!(read_key_file(PathBuf::from(env!("CARGO_MANIFEST_DIR"))).is_none());
}
