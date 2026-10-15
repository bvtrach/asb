//! Workload prompts loaded from the adjacent `workloads` directory.

use std::fs;
use std::path::Path;

const MAX_PROMPT_BYTES: u64 = 64 * 1024;

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
    use super::{load, populate};
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
}
