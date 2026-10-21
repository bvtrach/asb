//! Registry-neutral project executable precedence and readiness.
use std::path::{Path, PathBuf};

pub(crate) enum Lookup {
    Missing,
    Unusable(PathBuf),
    Found(PathBuf),
}

impl Lookup {
    pub fn path(self) -> Option<PathBuf> {
        match self {
            Self::Found(path) => Some(path),
            Self::Missing | Self::Unusable(_) => None,
        }
    }
}

pub(crate) fn is_executable(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// A present but invalid preferred entry never silently selects stale fallback bytes.
pub(crate) fn lookup(candidates: impl IntoIterator<Item = PathBuf>) -> Lookup {
    for candidate in candidates {
        match std::fs::symlink_metadata(&candidate) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            _ => {
                return if is_executable(&candidate) {
                    candidate
                        .canonicalize()
                        .map_or(Lookup::Unusable(candidate), Lookup::Found)
                } else {
                    Lookup::Unusable(candidate)
                };
            }
        }
    }
    Lookup::Missing
}

/// An explicit source override wins adapter-owned package preferences.
pub(crate) fn project_lookup(
    tools_root: &Path,
    id: &str,
    executable: &str,
    preferred: impl IntoIterator<Item = PathBuf>,
) -> Lookup {
    let root = tools_root.join(id);
    lookup(
        std::iter::once(root.join("bin").join(executable))
            .chain(preferred)
            .chain([
                root.join("node_modules/.bin").join(executable),
                root.join("venv/bin").join(executable),
                // The legacy shared layout belongs to this configured tool root, not cwd.
                tools_root.join("bin").join(executable),
            ]),
    )
}

#[cfg(all(test, unix))]
mod tests;
