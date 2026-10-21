//! Public endpoint metadata uses a private curl double, never a live catalog.
use super::*;
use std::{fs, path::Path};

#[cfg(unix)]
#[test]
fn endpoint_catalog_fetch_bounds_and_failures_do_not_send_credentials() {
    use std::os::unix::fs::PermissionsExt;
    const CASE: &str = "ASB_ZEN_ENDPOINT_FIXTURE";
    let Ok(case) = std::env::var(CASE) else {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/zen-endpoint-fixtures")
            .join(crate::workloads::TaskIdentity::local("fixture", None).attempt_id);
        fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let python = crate::tools::host_executable(Path::new("python3")).unwrap();
        let name = format!(
            "{}::endpoint_catalog_fetch_bounds_and_failures_do_not_send_credentials",
            module_path!().split_once("::").unwrap().1
        );
        for case in ["default", "override", "failed", "oversized", "missing"] {
            let directory = root.join(case);
            fs::create_dir_all(&directory).unwrap();
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .env_clear()
                .current_dir(&directory)
                .env("HOME", &directory)
                .env("PATH", &directory)
                .env(CASE, case)
                .env("ASB_FIXTURE_PYTHON", &python)
                .env("OPENCODE_ZEN_API_KEY", "fixture-secret-must-not-be-sent")
                .args(["--exact", &name, "--nocapture"]);
            if case == "override" {
                command.env(
                    "ASB_ENDPOINT_CATALOG_URL",
                    "https://fixture.invalid/metadata.json",
                );
            }
            if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
                command.env("LLVM_PROFILE_FILE", profile);
            }
            let output = command.output().unwrap();
            assert!(
                output.status.success(),
                "{case}: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        fs::remove_dir_all(root).unwrap();
        return;
    };
    if case != "missing" {
        let python = std::env::var("ASB_FIXTURE_PYTHON").unwrap();
        let script = r#"import os,sys
args=sys.argv[1:]
assert args[0]=='--disable'
assert '--max-filesize' in args and args[args.index('--max-filesize')+1]=='8388608'
assert args[args.index('--max-time')+1]=='8'
assert args[args.index('--proto')+1]=='=http,https'
assert not any('fixture-secret' in arg or 'Authorization' in arg for arg in args)
case=os.environ['ASB_ZEN_ENDPOINT_FIXTURE']
assert args[-1]==('https://fixture.invalid/metadata.json' if case=='override' else 'https://models.dev/api.json')
if case=='failed': sys.exit(22)
if case=='oversized': sys.stdout.write('x'*(8388608+1));sys.exit(0)
sys.stdout.write('{"opencode":{"models":{}}}')
"#;
        fs::write("curl", format!("#!{python}\n{script}")).unwrap();
        fs::set_permissions("curl", fs::Permissions::from_mode(0o755)).unwrap();
    }
    match case.as_str() {
        "default" | "override" => assert_eq!(
            fetch_endpoint_catalog().unwrap(),
            br#"{"opencode":{"models":{}}}"#
        ),
        "failed" | "oversized" => assert!(
            fetch_endpoint_catalog()
                .unwrap_err()
                .to_string()
                .contains("unavailable or exceeds 8 MiB")
        ),
        "missing" => assert!(
            fetch_endpoint_catalog()
                .unwrap_err()
                .to_string()
                .contains("cannot retrieve endpoint metadata")
        ),
        _ => panic!("unknown fixture"),
    }
}

#[test]
fn unsupported_catalog_sdk_and_google_metadata_have_explicit_offer_contracts() {
    use crate::endpoint::{Authentication, Interface};
    let catalog = serde_json::json!({"opencode":{"api":"https://opencode.ai/google/","npm":"@ai-sdk/google","models":{"fixture":{}}}});
    let offer = catalog_offer(&serde_json::to_vec(&catalog).unwrap(), "fixture").unwrap();
    assert_eq!(offer.interface, Interface::Gemini);
    assert_eq!(offer.authentication, Authentication::ApiKey);
    let mut unknown = catalog;
    unknown["opencode"]["npm"] = serde_json::json!("unsupported-sdk");
    assert!(catalog_offer(&serde_json::to_vec(&unknown).unwrap(), "fixture").is_none());
}
