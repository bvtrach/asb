//! Shared naming and manifest-reference policies for benchmark artifacts.
//!
//! Component normalization is deliberately legacy-compatible and lossy. It is
//! not a uniqueness allocator or a standalone path-confinement check. Reports
//! retain their separate collision-free graph identity allocator.

use crate::Error;
use std::path::{Component, Path, PathBuf};

/// Legacy per-replica cassette/result naming; shared by discovery and execution.
pub(crate) fn replica_path(path: &Path, index: usize) -> PathBuf {
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("benchmark");
    let name = match path.extension().and_then(|value| value.to_str()) {
        Some(extension) => format!("{stem}-concurrency-{index}.{extension}"),
        None => format!("{stem}-concurrency-{index}"),
    };
    path.parent()
        .map_or_else(|| PathBuf::from(&name), |parent| parent.join(&name))
}

/// Normalize a label embedded in an existing prefixed artifact filename.
pub(crate) fn component(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '-'
            }
        })
        .collect()
}

/// Preserve the historical fallback for absent or non-UTF-8 benchmark stems.
pub(crate) fn benchmark_stem(path: &Path) -> String {
    component(
        path.file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("benchmark"),
    )
}

/// Validate a manifest's relative TOML reference before either execution or
/// reporting. This is the shared lexical policy, not a canonicalized sandbox:
/// existing symlinks retain the same behavior as ordinary benchmark files.
pub(crate) fn checked_benchmark_reference(
    parent: &Path,
    reference: &str,
    sweep: &str,
) -> Result<PathBuf, Error> {
    let path = Path::new(reference);
    if path.is_absolute() || path.components().any(|part| part == Component::ParentDir) {
        return Err(Error::Input(format!(
            "sweep {sweep} contains unsafe benchmark path `{reference}`; use a relative .toml path"
        )));
    }
    if path.extension().and_then(|value| value.to_str()) != Some("toml") {
        return Err(Error::Input(format!(
            "sweep {sweep} entry `{reference}` is not a .toml benchmark"
        )));
    }
    let resolved = parent.join(path);
    if !resolved.is_file() {
        return Err(Error::Input(format!(
            "sweep {sweep} references missing benchmark {}",
            resolved.display()
        )));
    }
    Ok(resolved)
}

#[cfg(test)]
#[path = "../tests/unit/benchmark_paths.rs"]
mod tests;
