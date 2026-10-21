//! Legacy artifact identities and shared CLI reference rejection, offline.
use super::*;
use std::fs;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/benchmark-path-fixtures")
            .join(crate::workloads::TaskIdentity::local("paths", None).attempt_id);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn write(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, contents).unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn component_and_stem_preserve_existing_artifact_identity() {
    for (input, expected) in [
        ("", ""),
        ("ASCII.a-Z_09", "ASCII.a-Z_09"),
        ("model:a/b Ω", "model-a-b--"),
        ("openrouter/llama:1.0", "openrouter-llama-1.0"),
        ("é漢😀", "---"),
        ("e\u{301}", "e-"),
        ("a\\b\n\t", "a-b--"),
        (".", "."),
        ("..", ".."),
    ] {
        assert_eq!(component(input), expected, "{input:?}");
    }
    // This policy does not suddenly make old lossy labels unique or change
    // existing cassette paths; uniqueness belongs to the artifact allocator.
    assert_eq!(component("a/b"), component("a:b"));
    for (input, expected) in [
        ("", "benchmark"),
        ("nested/model:a Ω.toml", "model-a--"),
        ("nested/repeat.part.toml", "repeat.part"),
        ("plain", "plain"),
        (".hidden.toml", ".hidden"),
    ] {
        assert_eq!(benchmark_stem(Path::new(input)), expected, "{input:?}");
    }
}

#[cfg(unix)]
#[test]
fn non_utf8_stems_keep_the_existing_benchmark_fallback() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    let path = PathBuf::from(OsString::from_vec(b"bad-\xff.toml".to_vec()));
    assert_eq!(benchmark_stem(&path), "benchmark");
}

#[test]
fn references_distinguish_unsafe_wrong_type_and_missing_files() {
    let root = Fixture::new();
    let valid = root.write("nested/valid Ω.toml", "fixture");
    assert_eq!(
        checked_benchmark_reference(&root.0, "nested/valid Ω.toml", "sweep.toml").unwrap(),
        valid
    );
    for reference in ["../outside.toml", "nested/../outside.toml"] {
        let error = checked_benchmark_reference(&root.0, reference, "named-sweep")
            .unwrap_err()
            .to_string();
        assert!(error.contains("sweep named-sweep"));
        assert!(error.contains(reference));
        assert!(error.contains("unsafe benchmark path"));
        assert!(error.contains("use a relative .toml path"));
    }
    assert!(
        checked_benchmark_reference(&root.0, valid.to_str().unwrap(), "absolute")
            .unwrap_err()
            .to_string()
            .contains("unsafe benchmark path")
    );
    for reference in ["", "foo", "foo.json", "foo.TOML"] {
        assert!(
            checked_benchmark_reference(&root.0, reference, "extension")
                .unwrap_err()
                .to_string()
                .contains("is not a .toml benchmark")
        );
    }
    fs::create_dir(root.0.join("directory.toml")).unwrap();
    for reference in ["missing.toml", "directory.toml"] {
        let error = checked_benchmark_reference(&root.0, reference, "missing")
            .unwrap_err()
            .to_string();
        assert!(error.contains("references missing benchmark"));
        assert!(error.contains(&root.0.join(reference).display().to_string()));
    }
}

#[cfg(unix)]
#[test]
fn reference_policy_keeps_existing_symlinks_without_claiming_confinement() {
    let root = Fixture::new();
    root.write("target.toml", "fixture");
    std::os::unix::fs::symlink(root.0.join("target.toml"), root.0.join("link.toml")).unwrap();
    assert_eq!(
        checked_benchmark_reference(&root.0, "link.toml", "fixture").unwrap(),
        root.0.join("link.toml")
    );
    std::os::unix::fs::symlink(root.0.join("absent.toml"), root.0.join("broken.toml")).unwrap();
    assert!(
        checked_benchmark_reference(&root.0, "broken.toml", "fixture")
            .unwrap_err()
            .to_string()
            .contains("references missing benchmark")
    );
}

#[test]
fn sweep_and_compare_reject_the_same_bad_references_before_work_starts() {
    let root = Fixture::new();
    for reference in ["../outside.toml", "wrong.json", "missing.toml"] {
        let manifest = root.write(
            "selection.toml",
            &format!("configs = [{}]\n", toml::Value::String(reference.into())),
        );
        let mut errors = Vec::new();
        for command in ["sweep", "compare"] {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let status = crate::run(
                &[command.into(), manifest.as_os_str().into()],
                &mut stdout,
                &mut stderr,
            );
            assert_eq!(status, 2, "{command}: {stderr:?}");
            assert!(
                stdout.is_empty(),
                "no work or report output after bad input"
            );
            errors.push(String::from_utf8(stderr).unwrap());
        }
        assert_eq!(errors[0], errors[1], "{reference}");
        assert!(!root.0.join("selection-results").exists());
        assert!(!root.0.join("results").exists());
    }
}
