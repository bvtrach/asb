//! Verify source-build orchestration with a fixture's real digest, not a forged
//! external hasher claiming that arbitrary bytes match kernel.org's pin.
use super::*;
use sha2::{Digest, Sha256};
use std::os::unix::fs::PermissionsExt;

const CASE: &str = "ASB_PERF_SOURCE_UNIT_FIXTURE";
const PAYLOAD: &[u8] = b"offline perf source fixture";

#[test]
fn source_build_checks_real_fixture_digest_before_running_build_tools() {
    let Ok(root) = std::env::var_os(CASE).ok_or(()) else {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/perf-source-fixtures")
            .join(crate::workloads::TaskIdentity::local("perf-source", None).attempt_id);
        fs::create_dir_all(root.join("commands")).unwrap();
        for (name, script) in [
            (
                "curl",
                "#!/bin/sh\nwhile [ $# -gt 0 ]; do if [ \"$1\" = --output ]; then shift; printf 'offline perf source fixture' > \"$1\"; exit 0; fi; shift; done\nexit 1\n",
            ),
            (
                "tar",
                "#!/bin/sh\nprintf tar >> \"$ASB_PERF_SOURCE_UNIT_FIXTURE/build-called\"\nwhile [ $# -gt 0 ]; do if [ \"$1\" = -C ]; then shift; /bin/mkdir -p \"$1/tools/perf\"; exit 0; fi; shift; done\nexit 1\n",
            ),
            (
                "make",
                "#!/bin/sh\n[ \"$1\" = -C ] || exit 1\nprintf make >> \"$ASB_PERF_SOURCE_UNIT_FIXTURE/build-called\"\nprintf '#!/bin/sh\\nprintf \"perf fixture version\\\\n\"\\n' > \"$2/perf\"\n/bin/chmod 755 \"$2/perf\"\n",
            ),
            ("pkg-config", "#!/bin/sh\nexit 1\n"),
            (
                "sha256sum",
                "#!/bin/sh\nprintf called > \"$ASB_PERF_SOURCE_UNIT_FIXTURE/external-hasher-called\"\nexit 93\n",
            ),
        ] {
            let file = root.join("commands").join(name);
            fs::write(&file, script).unwrap();
            fs::set_permissions(file, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "tools::perf::source_install_tests::source_build_checks_real_fixture_digest_before_running_build_tools", "--nocapture"])
            .current_dir(&root)
            .env_clear()
            .env(CASE, &root)
            .env("PATH", root.join("commands"));
        if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
            command.env("LLVM_PROFILE_FILE", profile);
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
        fs::remove_dir_all(root).unwrap();
        return;
    };
    let root = PathBuf::from(root);
    let valid = root.join("valid");
    fs::create_dir(&valid).unwrap();
    let checksum = format!("{:x}", Sha256::digest(PAYLOAD));
    let binary = build_perf_from_source(
        &valid,
        &mut |_, _| {},
        "https://fixture.invalid/perf.tar.gz",
        &checksum,
    )
    .unwrap();
    assert!(runnable(&binary));
    assert_eq!(fs::read(root.join("build-called")).unwrap(), b"tarmake");
    assert!(!root.join("external-hasher-called").exists());
    fs::remove_file(root.join("build-called")).unwrap();
    let invalid = root.join("invalid");
    fs::create_dir(&invalid).unwrap();
    let error = build_perf_from_source(
        &invalid,
        &mut |_, _| {},
        "https://fixture.invalid/perf.tar.gz",
        SOURCE_SHA256,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("checksum mismatch"), "{error}");
    assert!(error.contains(&format!("got {checksum}")), "{error}");
    assert!(
        !root.join("build-called").exists(),
        "unverified bytes reached tar/make"
    );
    assert!(!root.join("external-hasher-called").exists());
    assert!(!invalid.join("source").exists());
}
