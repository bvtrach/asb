//! Workload prompts loaded from the adjacent `workloads` directory.

use std::fs;
use std::path::Path;

const MAX_PROMPT_BYTES: u64 = 64 * 1024;

struct Fixture {
    name: &'static str,
    files: &'static [(&'static str, &'static str)],
}

const BUILTIN: &[(&str, &str)] = &[
    ("bug-fix", include_str!("../../../workloads/bug-fix.txt")),
    (
        "build-repair",
        include_str!("../../../workloads/build-repair.txt"),
    ),
    (
        "dependency-migration",
        include_str!("../../../workloads/dependency-migration.txt"),
    ),
    (
        "feature-addition",
        include_str!("../../../workloads/feature-addition.txt"),
    ),
    (
        "refactoring",
        include_str!("../../../workloads/refactoring.txt"),
    ),
    (
        "repository-navigation",
        include_str!("../../../workloads/repository-navigation.txt"),
    ),
    (
        "test-generation",
        include_str!("../../../workloads/test-generation.txt"),
    ),
];

const FIXTURES: &[Fixture] = &[
    Fixture {
        name: "bug-fix",
        files: &[
            (
                "README.md",
                include_str!("../../../workloads/fixtures/bug-fix/README.md"),
            ),
            (
                "src/parser.py",
                include_str!("../../../workloads/fixtures/bug-fix/src/parser.py"),
            ),
            (
                "tests/test_parser.py",
                include_str!("../../../workloads/fixtures/bug-fix/tests/test_parser.py"),
            ),
        ],
    },
    Fixture {
        name: "build-repair",
        files: &[
            (
                "README.md",
                include_str!("../../../workloads/fixtures/build-repair/README.md"),
            ),
            (
                "src/app.py",
                include_str!("../../../workloads/fixtures/build-repair/src/app.py"),
            ),
            (
                "tests/test_app.py",
                include_str!("../../../workloads/fixtures/build-repair/tests/test_app.py"),
            ),
            (
                "build.sh",
                include_str!("../../../workloads/fixtures/build-repair/build.sh"),
            ),
        ],
    },
    Fixture {
        name: "dependency-migration",
        files: &[
            (
                "README.md",
                include_str!("../../../workloads/fixtures/dependency-migration/README.md"),
            ),
            (
                "requirements.txt",
                include_str!("../../../workloads/fixtures/dependency-migration/requirements.txt"),
            ),
            (
                "src/client.py",
                include_str!("../../../workloads/fixtures/dependency-migration/src/client.py"),
            ),
            (
                "tests/test_client.py",
                include_str!(
                    "../../../workloads/fixtures/dependency-migration/tests/test_client.py"
                ),
            ),
        ],
    },
    Fixture {
        name: "feature-addition",
        files: &[
            (
                "README.md",
                include_str!("../../../workloads/fixtures/feature-addition/README.md"),
            ),
            (
                "src/formatter.py",
                include_str!("../../../workloads/fixtures/feature-addition/src/formatter.py"),
            ),
            (
                "tests/test_formatter.py",
                include_str!(
                    "../../../workloads/fixtures/feature-addition/tests/test_formatter.py"
                ),
            ),
        ],
    },
    Fixture {
        name: "refactoring",
        files: &[
            (
                "README.md",
                include_str!("../../../workloads/fixtures/refactoring/README.md"),
            ),
            (
                "src/labels.py",
                include_str!("../../../workloads/fixtures/refactoring/src/labels.py"),
            ),
            (
                "tests/test_labels.py",
                include_str!("../../../workloads/fixtures/refactoring/tests/test_labels.py"),
            ),
        ],
    },
    Fixture {
        name: "repository-navigation",
        files: &[
            (
                "README.md",
                include_str!("../../../workloads/fixtures/repository-navigation/README.md"),
            ),
            (
                "src/cli.py",
                include_str!("../../../workloads/fixtures/repository-navigation/src/cli.py"),
            ),
            (
                "src/registry.py",
                include_str!("../../../workloads/fixtures/repository-navigation/src/registry.py"),
            ),
            (
                "tests/test_cli.py",
                include_str!("../../../workloads/fixtures/repository-navigation/tests/test_cli.py"),
            ),
        ],
    },
    Fixture {
        name: "test-generation",
        files: &[
            (
                "README.md",
                include_str!("../../../workloads/fixtures/test-generation/README.md"),
            ),
            (
                "src/temperature.py",
                include_str!("../../../workloads/fixtures/test-generation/src/temperature.py"),
            ),
            (
                "tests/test_temperature.py",
                include_str!(
                    "../../../workloads/fixtures/test-generation/tests/test_temperature.py"
                ),
            ),
        ],
    },
];

/// Populate a new project with the prompts shipped in the ASB binary.
pub(crate) fn populate(directory: &Path) -> Result<usize, std::io::Error> {
    let mut added = 0;
    for (name, prompt) in BUILTIN {
        let path = directory.join(format!("{name}.txt"));
        if !path.exists() {
            fs::write(path, prompt)?;
            added += 1;
        }
    }
    Ok(added)
}

/// Copy all bundled fixture templates into a project's managed fixture area.
pub(crate) fn populate_fixtures(directory: &Path) -> Result<usize, std::io::Error> {
    let mut added = 0;
    for fixture in FIXTURES {
        for (relative, contents) in fixture.files {
            let path = directory.join(fixture.name).join(relative);
            if !path.exists() {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&path, contents)?;
                if relative.ends_with(".sh") {
                    set_executable_io(&path)?;
                }
                added += 1;
            }
        }
    }
    Ok(added)
}

/// Materialize the selected fixture into the run workspace without overwriting edits.
pub(crate) fn prepare_fixture(name: &str, directory: &Path) -> Result<usize, String> {
    let Some(fixture) = FIXTURES.iter().find(|fixture| fixture.name == name) else {
        return Ok(0);
    };
    let mut added = 0;
    for (relative, contents) in fixture.files {
        let path = directory.join(relative);
        if !path.exists() {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    format!(
                        "cannot create fixture directory {}: {error}",
                        parent.display()
                    )
                })?;
            }
            fs::write(&path, contents)
                .map_err(|error| format!("cannot write fixture {}: {error}", path.display()))?;
            if relative.ends_with(".sh") {
                set_executable(&path)?;
            }
            added += 1;
        }
    }
    Ok(added)
}

/// Copy an installed external workload source into the run workspace.
pub(crate) fn prepare_external(
    name: &str,
    workload_directory: &Path,
    workspace: &Path,
) -> Result<usize, String> {
    let source = workload_directory.join(name).join("source");
    if !source.is_dir() {
        return Ok(0);
    }
    copy_tree(&source, &workspace.join("external-workload"))
}

fn copy_tree(source: &Path, destination: &Path) -> Result<usize, String> {
    fs::create_dir_all(destination).map_err(|error| {
        format!(
            "cannot create external workload directory {}: {error}",
            destination.display()
        )
    })?;
    let mut added = 0;
    for entry in fs::read_dir(source).map_err(|error| {
        format!(
            "cannot read external workload source {}: {error}",
            source.display()
        )
    })? {
        let entry =
            entry.map_err(|error| format!("cannot inspect external workload source: {error}"))?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let kind = entry.file_type().map_err(|error| {
            format!(
                "cannot inspect external workload entry {}: {error}",
                source_path.display()
            )
        })?;
        if kind.is_dir() {
            added += copy_tree(&source_path, &destination_path)?;
        } else if kind.is_file() && !destination_path.exists() {
            fs::copy(&source_path, &destination_path).map_err(|error| {
                format!(
                    "cannot copy external workload file {}: {error}",
                    source_path.display()
                )
            })?;
            added += 1;
        }
    }
    Ok(added)
}

fn set_executable(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)
            .map_err(|error| format!("cannot inspect fixture {}: {error}", path.display()))?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).map_err(|error| {
            format!("cannot make fixture {} executable: {error}", path.display())
        })?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn set_executable_io(path: &Path) -> Result<(), std::io::Error> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// Load one workload prompt by name and report the available file names on a miss.
pub fn load(name: &str, directory: &Path) -> Result<String, String> {
    let id = name.strip_prefix("original.").unwrap_or(name);
    if id.is_empty()
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(format!("invalid workload name `{name}`"));
    }
    let path = directory.join(format!("{id}.txt"));
    let metadata = fs::metadata(&path).map_err(|_| missing(name, directory))?;
    if !metadata.is_file() {
        return Err(missing(name, directory));
    }
    if metadata.len() > MAX_PROMPT_BYTES {
        return Err(format!(
            "workload `{name}` exceeds 64 KiB in {}",
            path.display()
        ));
    }
    let prompt = fs::read_to_string(&path).map_err(|error| {
        format!(
            "cannot read workload `{name}` from {}: {error}",
            path.display()
        )
    })?;
    if prompt.trim().is_empty() {
        return Err(format!("workload `{name}` is empty: {}", path.display()));
    }
    Ok(prompt)
}

fn missing(name: &str, directory: &Path) -> String {
    let mut available = fs::read_dir(directory)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            (path.extension().and_then(|value| value.to_str()) == Some("txt"))
                .then(|| path.file_stem()?.to_str().map(str::to_owned))
                .flatten()
        })
        .collect::<Vec<_>>();
    available.sort();
    let choices = if available.is_empty() {
        "none".to_owned()
    } else {
        available.join(", ")
    };
    format!(
        "workload `{name}` was not found in {}; available: {choices}",
        directory.display()
    )
}

#[cfg(test)]
mod tests {
    use super::{load, populate, prepare_external, prepare_fixture};
    use std::fs;

    #[test]
    fn loads_original_alias_and_rejects_path_traversal() {
        let directory = std::env::temp_dir().join(format!("asb-workloads-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("demo.txt"), "demo prompt").unwrap();
        assert_eq!(load("original.demo", &directory).unwrap(), "demo prompt");
        assert!(load("../secret", &directory).is_err());
        let _ = fs::remove_file(directory.join("demo.txt"));
        let _ = fs::remove_dir(directory);
    }

    #[test]
    fn populate_copies_bundled_prompts_without_overwriting_custom_files() {
        let directory =
            std::env::temp_dir().join(format!("asb-bundled-workloads-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("bug-fix.txt"), "custom").unwrap();
        let added = populate(&directory).expect("populate succeeds");
        assert_eq!(added, 6);
        assert_eq!(
            fs::read_to_string(directory.join("bug-fix.txt")).unwrap(),
            "custom"
        );
        assert!(
            !fs::read_to_string(directory.join("repository-navigation.txt"))
                .unwrap()
                .is_empty()
        );
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn every_builtin_fixture_materializes_supporting_files() {
        let directory = std::env::temp_dir().join(format!("asb-fixtures-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        for name in [
            "bug-fix",
            "build-repair",
            "dependency-migration",
            "feature-addition",
            "refactoring",
            "repository-navigation",
            "test-generation",
        ] {
            let fixture_dir = directory.join(name);
            assert!(prepare_fixture(name, &fixture_dir).unwrap() > 0, "{name}");
            assert!(fixture_dir.join("README.md").is_file(), "{name} README");
            assert!(fixture_dir.join("tests").is_dir(), "{name} tests");
        }
        assert!(directory.join("build-repair/build.sh").is_file());
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn installed_external_source_is_copied_without_overwriting_workspace_edits() {
        let root = std::env::temp_dir().join(format!("asb-external-copy-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let source = root.join("workloads/swe-bench/source");
        let workspace = root.join("workspace");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("README.md"), "upstream").unwrap();
        fs::create_dir_all(workspace.join("external-workload")).unwrap();
        fs::write(workspace.join("external-workload/README.md"), "edited").unwrap();
        assert_eq!(
            prepare_external("swe-bench", &root.join("workloads"), &workspace).unwrap(),
            0
        );
        assert_eq!(
            fs::read_to_string(workspace.join("external-workload/README.md")).unwrap(),
            "edited"
        );
        let _ = fs::remove_dir_all(root);
    }
}
