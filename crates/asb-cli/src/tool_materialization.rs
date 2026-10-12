// Copyright (C) Huawei Technologies Co., Ltd. 2026. All rights reserved.
// SPDX-License-Identifier: MIT
//! The fail-closed source-to-executable materialization contract.
//!
//! This module is deliberately independent of network acquisition and package
//! managers.  It validates the immutable identity and bounded shape of an
//! adapter before a caller is allowed to activate its output.  The support
//! matrix is checked in as data so an unknown tool or strategy cannot silently
//! become a generic source build.
#![allow(missing_docs)]

#[cfg(unix)]
use rustix::process::{Pid, Signal, kill_process_group};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Component, Path};
use std::process::{Command, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub const MATRIX_SCHEMA_VERSION: u16 = 1;
pub const MAX_MATRIX_ROWS: usize = 32;
pub const MAX_RUNTIME_COMPONENTS: usize = 16;
pub const MAX_ARGUMENTS: usize = 64;
pub const MAX_ARGUMENT_BYTES: usize = 256;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SupportMatrix {
    pub schema_version: u16,
    pub matrix_id: String,
    pub rows: Vec<SupportRow>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SupportRow {
    pub tool_id: String,
    pub status: SupportStatus,
    pub source_revision: String,
    pub source_sha256: String,
    pub strategy: AdapterStrategy,
    pub package_strategy: PackageStrategy,
    pub runtime_closure: Vec<String>,
    pub launcher: LauncherIdentity,
    pub offline_fixture: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PackageStrategy {
    CatalogPrebuilt,
    PythonWheelOffline,
    NpmPackOffline,
    None,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SupportStatus {
    Supported,
    Unsupported,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdapterStrategy {
    PinnedPrebuilt,
    PinnedSourceBuild,
    Unsupported,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LauncherIdentity {
    pub relative_path: String,
    pub executable: bool,
    pub symlink: SymlinkPolicy,
    pub runtime: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SymlinkPolicy {
    RegularOnly,
    RelativeWithinInstall,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterializationContract {
    pub tool_id: String,
    pub source_revision: String,
    pub strategy: AdapterStrategy,
    pub package_strategy: PackageStrategy,
    pub runtime_closure: Vec<String>,
    pub launcher: LauncherIdentity,
    pub max_output_bytes: u64,
    pub max_smoke_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterializationReceipt {
    pub schema_version: u16,
    pub tool_id: String,
    pub source_revision: String,
    pub strategy: AdapterStrategy,
    pub package_strategy: String,
    pub runtime_closure: Vec<String>,
    pub entrypoint: String,
    pub entrypoint_sha256: String,
    pub smoke: SmokeResult,
    pub rollback_target: Option<String>,
    pub retention: RetentionDecision,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SmokeResult {
    pub passed: bool,
    pub output_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RetentionDecision {
    KeepLastKnownGood,
    ActivateAndRetainPrevious,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterializationError {
    UnknownTool,
    UnsupportedStrategy,
    InvalidSourceIdentity,
    InvalidPackageStrategy,
    UnboundedArguments,
    RuntimeDownloadForbidden,
    InvalidLauncher,
    MissingEntrypoint,
    EntrypointNotExecutable,
    UnsafeSymlink,
    RuntimeMismatch,
    OutputTooLarge,
    SmokeFailed,
    MatrixInvalid,
}

impl MaterializationError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::UnknownTool => "adapter_unknown_tool",
            Self::UnsupportedStrategy => "adapter_unsupported_strategy",
            Self::InvalidSourceIdentity => "adapter_source_identity_invalid",
            Self::InvalidPackageStrategy => "adapter_package_strategy_invalid",
            Self::UnboundedArguments => "adapter_arguments_unbounded",
            Self::RuntimeDownloadForbidden => "adapter_runtime_download_forbidden",
            Self::InvalidLauncher => "adapter_launcher_invalid",
            Self::MissingEntrypoint => "adapter_entrypoint_missing",
            Self::EntrypointNotExecutable => "adapter_entrypoint_not_executable",
            Self::UnsafeSymlink => "adapter_entrypoint_symlink_unsafe",
            Self::RuntimeMismatch => "adapter_runtime_mismatch",
            Self::OutputTooLarge => "adapter_output_too_large",
            Self::SmokeFailed => "adapter_smoke_failed",
            Self::MatrixInvalid => "adapter_matrix_invalid",
        }
    }
}

pub fn support_matrix() -> SupportMatrix {
    serde_json::from_str(include_str!(
        "../fixtures/v1/source-executable-support-matrix.json"
    ))
    .expect("checked-in source executable matrix must be valid")
}

pub fn validate_matrix(matrix: &SupportMatrix) -> Result<(), MaterializationError> {
    if matrix.schema_version != MATRIX_SCHEMA_VERSION
        || matrix.matrix_id != "asb-source-executable-v1"
        || matrix.rows.is_empty()
        || matrix.rows.len() > MAX_MATRIX_ROWS
    {
        return Err(MaterializationError::MatrixInvalid);
    }
    let mut ids = BTreeSet::new();
    for row in &matrix.rows {
        if !ids.insert(&row.tool_id)
            || row.tool_id.is_empty()
            || !valid_revision(&row.source_revision)
            || !is_sha256(&row.source_sha256)
            || row.offline_fixture.is_empty()
        {
            return Err(MaterializationError::MatrixInvalid);
        }
        validate_launcher(&row.launcher)?;
        if row.runtime_closure.len() > MAX_RUNTIME_COMPONENTS
            || row
                .runtime_closure
                .iter()
                .any(|item| !valid_public_id(item))
        {
            return Err(MaterializationError::MatrixInvalid);
        }
        if matches!(row.status, SupportStatus::Unsupported)
            && !matches!(row.strategy, AdapterStrategy::Unsupported)
        {
            return Err(MaterializationError::MatrixInvalid);
        }
        if matches!(row.status, SupportStatus::Supported)
            && matches!(row.strategy, AdapterStrategy::Unsupported)
        {
            return Err(MaterializationError::MatrixInvalid);
        }
    }
    Ok(())
}

pub fn contract_for(tool_id: &str) -> Result<MaterializationContract, MaterializationError> {
    let matrix = support_matrix();
    let catalog = crate::tool_catalog::catalog();
    crate::tool_catalog::validate(&catalog).map_err(|_| MaterializationError::MatrixInvalid)?;
    validate_against_catalog(&matrix, &catalog)?;
    let row = matrix
        .rows
        .into_iter()
        .find(|row| row.tool_id == tool_id)
        .ok_or(MaterializationError::UnknownTool)?;
    if matches!(row.status, SupportStatus::Unsupported) {
        return Err(MaterializationError::UnsupportedStrategy);
    }
    validate_runtime_compatibility(&row.launcher.runtime)?;
    Ok(MaterializationContract {
        tool_id: row.tool_id,
        source_revision: row.source_revision,
        strategy: row.strategy,
        package_strategy: row.package_strategy,
        runtime_closure: row.runtime_closure,
        launcher: row.launcher,
        max_output_bytes: 1024 * 1024 * 1024,
        max_smoke_bytes: 64 * 1024,
    })
}

/// Bind the matrix to the immutable acquisition catalog.  A matrix row is
/// never allowed to invent a revision or artifact digest that the acquisition
/// catalog does not carry.
pub fn validate_against_catalog(
    matrix: &SupportMatrix,
    catalog: &crate::tool_catalog::ToolCatalog,
) -> Result<(), MaterializationError> {
    validate_matrix(matrix)?;
    if matrix.rows.len() != catalog.entries.len() {
        return Err(MaterializationError::MatrixInvalid);
    }
    for entry in &catalog.entries {
        let row = matrix
            .rows
            .iter()
            .find(|row| row.tool_id == entry.id)
            .ok_or(MaterializationError::MatrixInvalid)?;
        if row.source_revision != entry.official_source.revision {
            return Err(MaterializationError::InvalidSourceIdentity);
        }
        let expected_digest = entry
            .alternatives
            .iter()
            .find(|alternative| alternative.kind == "source" || alternative.kind == "prebuilt")
            .map(|alternative| alternative.sha256.as_str())
            .ok_or(MaterializationError::MatrixInvalid)?;
        if row.source_sha256 != expected_digest {
            return Err(MaterializationError::InvalidSourceIdentity);
        }
        if matches!(row.status, SupportStatus::Unsupported)
            && !matches!(row.package_strategy, PackageStrategy::None)
        {
            return Err(MaterializationError::MatrixInvalid);
        }
        if matches!(row.status, SupportStatus::Supported)
            && matches!(row.package_strategy, PackageStrategy::None)
        {
            return Err(MaterializationError::MatrixInvalid);
        }
    }
    Ok(())
}

pub fn validate_recipe_arguments(argv: &[String]) -> Result<(), MaterializationError> {
    if argv.is_empty() || argv.len() > MAX_ARGUMENTS {
        return Err(MaterializationError::UnboundedArguments);
    }
    for arg in argv {
        if arg.is_empty()
            || arg.len() > MAX_ARGUMENT_BYTES
            || arg.contains([';', '|', '&', '`', '$', '\n', '\r'])
            || arg.to_ascii_lowercase().contains("download")
            || arg.to_ascii_lowercase().contains("install")
        {
            return Err(MaterializationError::UnboundedArguments);
        }
    }
    Ok(())
}

pub fn validate_runtime_closure(closure: &[String]) -> Result<(), MaterializationError> {
    if closure.len() > MAX_RUNTIME_COMPONENTS
        || closure.iter().any(|item| {
            !valid_public_id(item)
                || item.to_ascii_lowercase().contains("download")
                || item.to_ascii_lowercase().contains("latest")
        })
    {
        return Err(MaterializationError::RuntimeDownloadForbidden);
    }
    Ok(())
}

pub fn validate_launcher(launcher: &LauncherIdentity) -> Result<(), MaterializationError> {
    let path = Path::new(&launcher.relative_path);
    if launcher.relative_path.is_empty()
        || !launcher.executable
        || path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
        || launcher.runtime.is_empty()
        || launcher.runtime.len() > MAX_ARGUMENT_BYTES
    {
        return Err(MaterializationError::InvalidLauncher);
    }
    Ok(())
}

pub fn validate_runtime_compatibility(runtime: &str) -> Result<(), MaterializationError> {
    if runtime == "native" {
        return Ok(());
    }
    if runtime.starts_with("linux-") && runtime != crate::platform_label() {
        return Err(MaterializationError::RuntimeMismatch);
    }
    Ok(())
}

/// Run a catalogued launcher with a fixed, non-interactive version probe. No
/// output is retained; only its bounded byte count enters the receipt.
pub fn smoke_entrypoint(
    path: &Path,
    max_output_bytes: u64,
) -> Result<SmokeResult, MaterializationError> {
    let mut command = Command::new(path);
    command
        .arg("--version")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command
        .spawn()
        .map_err(|_| MaterializationError::SmokeFailed)?;
    let stdout = bounded_reader(
        child
            .stdout
            .take()
            .ok_or(MaterializationError::SmokeFailed)?,
        max_output_bytes,
    );
    let stderr = bounded_reader(
        child
            .stderr
            .take()
            .ok_or(MaterializationError::SmokeFailed)?,
        max_output_bytes,
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if Instant::now() >= deadline {
            #[cfg(unix)]
            if let Some(pid) = Pid::from_raw(child.id() as i32) {
                let _ = kill_process_group(pid, Signal::KILL);
            }
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout.join();
            let _ = stderr.join();
            return Err(MaterializationError::SmokeFailed);
        }
        let observed = match child.try_wait() {
            Ok(status) => status,
            Err(_) => {
                #[cfg(unix)]
                if let Some(pid) = Pid::from_raw(child.id() as i32) {
                    let _ = kill_process_group(pid, Signal::KILL);
                }
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout.join();
                let _ = stderr.join();
                return Err(MaterializationError::SmokeFailed);
            }
        };
        if observed.is_some() {
            #[cfg(unix)]
            if let Some(pid) = Pid::from_raw(child.id() as i32) {
                // The direct launcher may have exited while a descendant
                // still owns stdout/stderr. Close the private process group
                // before joining drainers so terminal success cannot hang.
                let _ = kill_process_group(pid, Signal::KILL);
            }
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let status = child
        .wait()
        .map_err(|_| MaterializationError::SmokeFailed)?;
    let output_bytes = stdout
        .join()
        .map_err(|_| MaterializationError::SmokeFailed)?
        .saturating_add(
            stderr
                .join()
                .map_err(|_| MaterializationError::SmokeFailed)?,
        );
    if output_bytes > max_output_bytes || !status.success() {
        return Err(MaterializationError::SmokeFailed);
    }
    Ok(SmokeResult {
        passed: true,
        output_bytes,
    })
}

fn bounded_reader<R: Read + Send + 'static>(mut reader: R, max: u64) -> JoinHandle<u64> {
    thread::spawn(move || {
        let mut total = 0_u64;
        let mut buffer = [0_u8; 8192];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => return total,
                Ok(read) => total = total.saturating_add(read as u64).min(max.saturating_add(1)),
            }
        }
    })
}

/// Verify the output without following an attacker-controlled final symlink.
/// Approved relative symlinks must stay inside the installation root and point
/// at a regular executable; regular files are always accepted only when they
/// carry an executable mode.
pub fn inspect_entrypoint(
    install_root: &Path,
    entrypoint: &Path,
    policy: &SymlinkPolicy,
    expected_digest: Option<&str>,
) -> Result<String, MaterializationError> {
    let path = install_root.join(entrypoint);
    let link_meta =
        fs::symlink_metadata(&path).map_err(|_| MaterializationError::MissingEntrypoint)?;
    let target = if link_meta.file_type().is_symlink() {
        if matches!(policy, SymlinkPolicy::RegularOnly) {
            return Err(MaterializationError::UnsafeSymlink);
        }
        let raw = fs::read_link(&path).map_err(|_| MaterializationError::UnsafeSymlink)?;
        if raw.is_absolute()
            || raw
                .components()
                .any(|part| matches!(part, Component::ParentDir))
        {
            return Err(MaterializationError::UnsafeSymlink);
        }
        let resolved = path.parent().unwrap_or(install_root).join(raw);
        let canonical_root = install_root
            .canonicalize()
            .map_err(|_| MaterializationError::UnsafeSymlink)?;
        let canonical = resolved
            .canonicalize()
            .map_err(|_| MaterializationError::MissingEntrypoint)?;
        if !canonical.starts_with(&canonical_root) {
            return Err(MaterializationError::UnsafeSymlink);
        }
        canonical
    } else {
        path.clone()
    };
    let metadata = fs::metadata(&target).map_err(|_| MaterializationError::MissingEntrypoint)?;
    if !metadata.is_file() {
        return Err(MaterializationError::MissingEntrypoint);
    }
    #[cfg(unix)]
    if metadata.permissions().mode() & 0o111 == 0 {
        return Err(MaterializationError::EntrypointNotExecutable);
    }
    let bytes = fs::read(&target).map_err(|_| MaterializationError::MissingEntrypoint)?;
    if bytes.len() > 1024 * 1024 * 1024 {
        return Err(MaterializationError::OutputTooLarge);
    }
    let digest = format!("{:x}", Sha256::digest(&bytes));
    if expected_digest.is_some_and(|expected| expected != digest) {
        return Err(MaterializationError::RuntimeMismatch);
    }
    Ok(digest)
}

fn valid_public_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ARGUMENT_BYTES
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn valid_revision(value: &str) -> bool {
    (value.len() == 40 && value.bytes().all(|b| b.is_ascii_hexdigit()))
        || (!value.is_empty()
            && value.len() <= MAX_ARGUMENT_BYTES
            && value != "main"
            && value != "master"
            && value != "latest"
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'/')))
}

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn checked_matrix_is_explicit_and_fail_closed() {
        let matrix = support_matrix();
        validate_matrix(&matrix).unwrap();
        validate_against_catalog(&matrix, &crate::tool_catalog::catalog()).unwrap();
        assert_eq!(
            contract_for("aider"),
            Err(MaterializationError::UnsupportedStrategy)
        );
        assert!(contract_for("codex").is_ok());
        assert_eq!(
            contract_for("not-catalogued"),
            Err(MaterializationError::UnknownTool)
        );
        assert_eq!(
            contract_for("perf"),
            Err(MaterializationError::UnsupportedStrategy)
        );
    }

    #[test]
    fn hostile_recipe_and_runtime_inputs_are_rejected() {
        assert_eq!(
            validate_recipe_arguments(&["cargo".into(), "install".into()]),
            Err(MaterializationError::UnboundedArguments)
        );
        assert_eq!(
            validate_recipe_arguments(&["cargo".into(), "--offline".into()]),
            Ok(())
        );
        assert_eq!(
            validate_runtime_closure(&["python-latest".into()]),
            Err(MaterializationError::RuntimeDownloadForbidden)
        );
        assert_eq!(validate_runtime_closure(&["python-3-11".into()]), Ok(()));
    }

    #[test]
    fn validation_boundaries_reject_empty_drift_and_shell_shapes() {
        assert_eq!(
            validate_recipe_arguments(&[]),
            Err(MaterializationError::UnboundedArguments)
        );
        assert_eq!(
            validate_recipe_arguments(&["echo\nvalue".into()]),
            Err(MaterializationError::UnboundedArguments)
        );
        assert_eq!(
            validate_recipe_arguments(&["--offline".into(), "$(id)".into()]),
            Err(MaterializationError::UnboundedArguments)
        );
        assert_eq!(
            validate_runtime_closure(&["".into()]),
            Err(MaterializationError::RuntimeDownloadForbidden)
        );
        assert_eq!(
            validate_runtime_closure(&["python-latest".into()]),
            Err(MaterializationError::RuntimeDownloadForbidden)
        );
        assert_eq!(
            validate_runtime_closure(&["python-download".into()]),
            Err(MaterializationError::RuntimeDownloadForbidden)
        );

        for launcher in [
            LauncherIdentity {
                relative_path: "".into(),
                executable: true,
                symlink: SymlinkPolicy::RegularOnly,
                runtime: "native".into(),
            },
            LauncherIdentity {
                relative_path: "/bin/tool".into(),
                executable: true,
                symlink: SymlinkPolicy::RegularOnly,
                runtime: "native".into(),
            },
            LauncherIdentity {
                relative_path: "./tool".into(),
                executable: true,
                symlink: SymlinkPolicy::RegularOnly,
                runtime: "native".into(),
            },
            LauncherIdentity {
                relative_path: "../tool".into(),
                executable: true,
                symlink: SymlinkPolicy::RegularOnly,
                runtime: "native".into(),
            },
            LauncherIdentity {
                relative_path: "tool".into(),
                executable: false,
                symlink: SymlinkPolicy::RegularOnly,
                runtime: "native".into(),
            },
            LauncherIdentity {
                relative_path: "tool".into(),
                executable: true,
                symlink: SymlinkPolicy::RegularOnly,
                runtime: "".into(),
            },
        ] {
            assert_eq!(
                validate_launcher(&launcher),
                Err(MaterializationError::InvalidLauncher)
            );
        }
        assert_eq!(
            validate_runtime_compatibility("linux-not-this-host"),
            Err(MaterializationError::RuntimeMismatch)
        );
        assert_eq!(validate_runtime_compatibility("native"), Ok(()));
        assert_eq!(
            validate_runtime_compatibility(&crate::platform_label()),
            Ok(())
        );
        assert_eq!(
            MaterializationError::OutputTooLarge.code(),
            "adapter_output_too_large"
        );
    }

    #[test]
    fn matrix_and_catalog_mismatches_fail_closed() {
        let catalog = crate::tool_catalog::catalog();
        let mut matrix = support_matrix();
        matrix.matrix_id = "wrong".into();
        assert_eq!(
            validate_matrix(&matrix),
            Err(MaterializationError::MatrixInvalid)
        );
        let mut matrix = support_matrix();
        matrix.rows[0].source_revision = "latest".into();
        assert_eq!(
            validate_matrix(&matrix),
            Err(MaterializationError::MatrixInvalid)
        );
        let mut matrix = support_matrix();
        matrix.rows[0].source_sha256 = "not-a-digest".into();
        assert_eq!(
            validate_against_catalog(&matrix, &catalog),
            Err(MaterializationError::MatrixInvalid)
        );
        let mut matrix = support_matrix();
        matrix.rows.pop();
        assert_eq!(
            validate_against_catalog(&matrix, &catalog),
            Err(MaterializationError::MatrixInvalid)
        );
        let mut matrix = support_matrix();
        matrix.rows[0].status = SupportStatus::Supported;
        assert_eq!(
            validate_matrix(&matrix),
            Err(MaterializationError::MatrixInvalid)
        );
        let mut matrix = support_matrix();
        matrix.rows[1].strategy = AdapterStrategy::Unsupported;
        assert_eq!(
            validate_matrix(&matrix),
            Err(MaterializationError::MatrixInvalid)
        );
        let mut matrix = support_matrix();
        matrix.rows[1].package_strategy = PackageStrategy::None;
        assert_eq!(
            validate_against_catalog(&matrix, &catalog),
            Err(MaterializationError::MatrixInvalid)
        );
        assert_eq!(contract_for(""), Err(MaterializationError::UnknownTool));
    }

    #[test]
    fn entrypoint_requires_executable_regular_file_and_digest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tool");
        fs::write(&path, b"fixture").unwrap();
        assert_eq!(
            inspect_entrypoint(
                dir.path(),
                Path::new("tool"),
                &SymlinkPolicy::RegularOnly,
                None
            ),
            Err(MaterializationError::EntrypointNotExecutable)
        );
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        let digest = inspect_entrypoint(
            dir.path(),
            Path::new("tool"),
            &SymlinkPolicy::RegularOnly,
            None,
        )
        .unwrap();
        assert_eq!(
            inspect_entrypoint(
                dir.path(),
                Path::new("tool"),
                &SymlinkPolicy::RegularOnly,
                Some("bad")
            ),
            Err(MaterializationError::RuntimeMismatch)
        );
        assert_eq!(digest.len(), 64);
        assert_eq!(
            inspect_entrypoint(
                dir.path(),
                Path::new("tool"),
                &SymlinkPolicy::RegularOnly,
                Some(&digest)
            ),
            Ok(digest)
        );
    }

    #[cfg(unix)]
    #[test]
    fn relative_symlink_and_digest_boundaries_are_explicit() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        let link = dir.path().join("tool");
        fs::write(&target, b"fixture").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o700)).unwrap();
        std::os::unix::fs::symlink("target", &link).unwrap();
        let digest = format!("{:x}", Sha256::digest(b"fixture"));
        assert_eq!(
            inspect_entrypoint(
                dir.path(),
                Path::new("tool"),
                &SymlinkPolicy::RelativeWithinInstall,
                Some(&digest)
            ),
            Ok(digest.clone())
        );
        let wrong = "0".repeat(64);
        assert_eq!(
            inspect_entrypoint(
                dir.path(),
                Path::new("tool"),
                &SymlinkPolicy::RelativeWithinInstall,
                Some(&wrong)
            ),
            Err(MaterializationError::RuntimeMismatch)
        );
        fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink("../target", &link).unwrap();
        assert_eq!(
            inspect_entrypoint(
                dir.path(),
                Path::new("tool"),
                &SymlinkPolicy::RelativeWithinInstall,
                None
            ),
            Err(MaterializationError::UnsafeSymlink)
        );
    }

    #[test]
    fn unsafe_symlink_and_missing_entrypoint_fail_before_activation() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("tool")).unwrap();
        assert_eq!(
            inspect_entrypoint(
                dir.path(),
                Path::new("tool"),
                &SymlinkPolicy::RelativeWithinInstall,
                None
            ),
            Err(MaterializationError::UnsafeSymlink)
        );
        assert_eq!(
            inspect_entrypoint(
                dir.path(),
                Path::new("missing"),
                &SymlinkPolicy::RegularOnly,
                None
            ),
            Err(MaterializationError::MissingEntrypoint)
        );
    }

    #[test]
    fn rejected_candidate_does_not_change_last_known_good_bytes_or_receipt() {
        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("good");
        let receipt = dir.path().join("receipt.json");
        fs::write(&good, b"known-good").unwrap();
        fs::set_permissions(&good, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(&receipt, br#"{"digest":"known-good"}"#).unwrap();
        let before_good = fs::read(&good).unwrap();
        let before_receipt = fs::read(&receipt).unwrap();
        let candidate = dir.path().join("candidate");
        std::os::unix::fs::symlink(&good, &candidate).unwrap();
        assert_eq!(
            inspect_entrypoint(
                dir.path(),
                Path::new("candidate"),
                &SymlinkPolicy::RegularOnly,
                None
            ),
            Err(MaterializationError::UnsafeSymlink)
        );
        assert_eq!(fs::read(&good).unwrap(), before_good);
        assert_eq!(fs::read(&receipt).unwrap(), before_receipt);
    }

    #[cfg(unix)]
    #[test]
    fn smoke_rejects_output_over_bound_without_retaining_it() {
        let dir = tempfile::tempdir().unwrap();
        let launcher = dir.path().join("launcher");
        fs::write(
            &launcher,
            b"#!/bin/sh\nprintf '%100s' x\nprintf '%100s' x >&2\n",
        )
        .unwrap();
        fs::set_permissions(&launcher, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            smoke_entrypoint(&launcher, 16),
            Err(MaterializationError::SmokeFailed)
        );
    }

    #[cfg(unix)]
    #[test]
    fn smoke_rejects_missing_nonzero_and_timeout_launchers() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            smoke_entrypoint(&dir.path().join("missing"), 32),
            Err(MaterializationError::SmokeFailed)
        );
        let failing = dir.path().join("failing");
        fs::write(&failing, b"#!/bin/sh\nexit 7\n").unwrap();
        fs::set_permissions(&failing, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            smoke_entrypoint(&failing, 32),
            Err(MaterializationError::SmokeFailed)
        );
        let hanging = dir.path().join("hanging");
        fs::write(&hanging, b"#!/bin/sh\nsleep 5\n").unwrap();
        fs::set_permissions(&hanging, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            smoke_entrypoint(&hanging, 32),
            Err(MaterializationError::SmokeFailed)
        );
    }

    #[cfg(unix)]
    #[test]
    fn smoke_accepts_bounded_successful_launcher() {
        let dir = tempfile::tempdir().unwrap();
        let launcher = dir.path().join("launcher");
        fs::write(&launcher, b"#!/bin/sh\nprintf ok\nprintf err >&2\n").unwrap();
        fs::set_permissions(&launcher, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            smoke_entrypoint(&launcher, 32),
            Ok(SmokeResult {
                passed: true,
                output_bytes: 5,
            })
        );
    }

    #[cfg(unix)]
    #[test]
    fn final_identity_check_rejects_replacement_after_initial_validation() {
        let dir = tempfile::tempdir().unwrap();
        let launcher = dir.path().join("launcher");
        fs::write(&launcher, b"known-good").unwrap();
        fs::set_permissions(&launcher, fs::Permissions::from_mode(0o700)).unwrap();
        let digest = inspect_entrypoint(
            dir.path(),
            Path::new("launcher"),
            &SymlinkPolicy::RegularOnly,
            None,
        )
        .unwrap();
        fs::write(&launcher, b"replacement").unwrap();
        assert_eq!(
            inspect_entrypoint(
                dir.path(),
                Path::new("launcher"),
                &SymlinkPolicy::RegularOnly,
                Some(&digest),
            ),
            Err(MaterializationError::RuntimeMismatch)
        );
    }

    #[cfg(unix)]
    #[test]
    fn smoke_reaps_descendant_that_retains_output_pipes() {
        let dir = tempfile::tempdir().unwrap();
        let launcher = dir.path().join("launcher");
        fs::write(&launcher, b"#!/bin/sh\n(sleep 30; printf late) &\nexit 0\n").unwrap();
        fs::set_permissions(&launcher, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            smoke_entrypoint(&launcher, 32),
            Ok(SmokeResult {
                passed: true,
                output_bytes: 0,
            })
        );
    }
}
