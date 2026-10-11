// Copyright (C) Huawei Technologies Co., Ltd. 2026. All rights reserved.
// SPDX-License-Identifier: MIT
//! Bounded acquisition of catalog-pinned source checkouts.
//!
//! This boundary deliberately does not understand branches, tags, archives,
//! or prebuilt artifacts.  A caller supplies a catalog entry and the provider
//! returns only an exact, detached revision from that entry's repository.
#![allow(missing_docs)]

use crate::tool_catalog::ToolEntry;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub const MAX_CHECKOUT_SECONDS: u64 = 30 * 60;
pub const MAX_CHECKOUT_BYTES: u64 = 1024 * 1024 * 1024;
pub const MAX_CHECKOUT_FILES: u64 = 100_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckoutLimits {
    pub timeout: Duration,
    pub max_bytes: u64,
    pub max_files: u64,
}

impl Default for CheckoutLimits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(15 * 60),
            max_bytes: MAX_CHECKOUT_BYTES,
            max_files: MAX_CHECKOUT_FILES,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CheckoutRequest {
    repository_url: String,
    revision: String,
    staging_root: PathBuf,
    limits: CheckoutLimits,
    cancel: Arc<AtomicBool>,
}

impl CheckoutRequest {
    pub fn for_tool(tool: &ToolEntry, staging_root: PathBuf) -> Result<Self, CheckoutError> {
        let revision = tool.official_source.revision.clone();
        let repository_url = tool.official_source.url.clone();
        if !cfg!(test) {
            let catalog = crate::tool_catalog::catalog();
            crate::tool_catalog::validate(&catalog).map_err(|_| CheckoutError::InvalidRequest)?;
            let allowed = catalog.entries.iter().any(|entry| {
                entry.id == tool.id
                    && entry.official_source.url == repository_url
                    && entry.official_source.revision == revision
            });
            if !allowed {
                return Err(CheckoutError::RepositoryIdentityMismatch);
            }
        }
        validate_identity(&repository_url, &revision)?;
        Ok(Self {
            repository_url,
            revision,
            staging_root,
            limits: CheckoutLimits::default(),
            cancel: Arc::new(AtomicBool::new(false)),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceCheckout {
    pub path: PathBuf,
    pub repository_url: String,
    pub revision: String,
}

impl SourceCheckout {
    /// Remove the transaction-owned checkout after the pinned builder has
    /// consumed it.  Callers must treat cleanup failure as a failed build.
    pub fn cleanup(self) -> Result<(), CheckoutError> {
        fs::remove_dir_all(self.path).map_err(|_| CheckoutError::CleanupFailed)
    }
}

impl CheckoutRequest {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckoutError {
    InvalidRequest,
    Offline,
    Unavailable,
    RepositoryIdentityMismatch,
    RevisionMismatch,
    ResourceLimit,
    Cancelled,
    CleanupFailed,
}

impl CheckoutError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidRequest => "source_checkout_invalid_request",
            Self::Offline => "source_checkout_offline",
            Self::Unavailable => "source_checkout_unavailable",
            Self::RepositoryIdentityMismatch => "source_checkout_repository_mismatch",
            Self::RevisionMismatch => "source_checkout_revision_mismatch",
            Self::ResourceLimit => "source_checkout_resource_limit",
            Self::Cancelled => "source_checkout_cancelled",
            Self::CleanupFailed => "source_checkout_cleanup_failed",
        }
    }
}

pub fn acquire(request: &CheckoutRequest) -> Result<SourceCheckout, CheckoutError> {
    validate_request(request)?;
    fs::create_dir_all(&request.staging_root).map_err(|_| CheckoutError::Unavailable)?;
    let name = format!("checkout-{}", std::process::id());
    let staging = request.staging_root.join(name);
    if staging.exists() {
        return Err(CheckoutError::CleanupFailed);
    }
    let result = acquire_inner(request, &staging);
    if result.is_err() && staging.exists() && fs::remove_dir_all(&staging).is_err() {
        return Err(CheckoutError::CleanupFailed);
    }
    result
}

fn acquire_inner(
    request: &CheckoutRequest,
    staging: &Path,
) -> Result<SourceCheckout, CheckoutError> {
    let deadline = Instant::now() + request.limits.timeout;
    let mut clone = command(request, "clone");
    clone.args([
        "--no-checkout",
        "--no-tags",
        "--depth",
        "1",
        "--filter=blob:none",
        "--no-local",
    ]);
    clone.arg(&request.repository_url).arg(staging);
    run_bounded_until(clone, request, deadline, Some(staging))?;
    enforce_bounds(staging, &request.limits)?;

    let remote = staging.join(".asb-remote");
    let mut remote_command = command(request, "remote");
    remote_command
        .current_dir(staging)
        .args(["get-url", "origin"]);
    run_bounded_capture(remote_command, &remote, request, deadline, Some(staging))?;
    let actual =
        fs::read_to_string(&remote).map_err(|_| CheckoutError::RepositoryIdentityMismatch)?;
    let _ = fs::remove_file(&remote);
    if actual.trim() != request.repository_url {
        return Err(CheckoutError::RepositoryIdentityMismatch);
    }

    let mut fetch = command(request, "fetch");
    fetch
        .current_dir(staging)
        .args(["--no-tags", "origin", &request.revision]);
    run_bounded_until(fetch, request, deadline, Some(staging))?;
    enforce_bounds(staging, &request.limits)?;

    let mut checkout = command(request, "checkout");
    checkout
        .current_dir(staging)
        .args(["--detach", &request.revision]);
    run_bounded_until(checkout, request, deadline, Some(staging))?;

    let head_file = staging.join(".asb-head");
    let mut head = command(request, "rev-parse");
    head.current_dir(staging).args(["--verify", "HEAD"]);
    run_bounded_capture(head, &head_file, request, deadline, Some(staging))?;
    let actual = fs::read_to_string(&head_file).map_err(|_| CheckoutError::RevisionMismatch)?;
    let _ = fs::remove_file(&head_file);
    if actual.trim() != request.revision {
        return Err(CheckoutError::RevisionMismatch);
    }
    enforce_bounds(staging, &request.limits)?;
    Ok(SourceCheckout {
        path: staging.to_path_buf(),
        repository_url: request.repository_url.clone(),
        revision: request.revision.clone(),
    })
}

fn validate_identity(url: &str, revision: &str) -> Result<(), CheckoutError> {
    let local_fixture = cfg!(test) && url.starts_with("file://");
    if (!url.starts_with("https://") && !local_fixture)
        || url.len() > 2048
        || revision.len() != 40
        || url.contains('@')
    {
        return Err(CheckoutError::InvalidRequest);
    }
    if !revision.bytes().all(|b| b.is_ascii_hexdigit()) || url.contains(['\n', '\r', '\0']) {
        return Err(CheckoutError::InvalidRequest);
    }
    Ok(())
}

fn validate_request(request: &CheckoutRequest) -> Result<(), CheckoutError> {
    validate_identity(&request.repository_url, &request.revision)?;
    if request.staging_root.as_os_str().is_empty()
        || request.limits.timeout.is_zero()
        || request.limits.timeout > Duration::from_secs(MAX_CHECKOUT_SECONDS)
        || request.limits.max_bytes == 0
        || request.limits.max_bytes > MAX_CHECKOUT_BYTES
        || request.limits.max_files == 0
        || request.limits.max_files > MAX_CHECKOUT_FILES
    {
        return Err(CheckoutError::InvalidRequest);
    }
    Ok(())
}

fn command(request: &CheckoutRequest, subcommand: &str) -> Command {
    let mut command = Command::new("git");
    command
        .arg("-c")
        .arg("core.hooksPath=/dev/null")
        .arg("-c")
        .arg(if cfg!(test) {
            "protocol.file.allow=always"
        } else {
            "protocol.file.allow=never"
        })
        .arg(subcommand)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("HOME", request.staging_root.join(".home"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

fn run_bounded_until(
    mut command: Command,
    request: &CheckoutRequest,
    deadline: Instant,
    watch: Option<&Path>,
) -> Result<(), CheckoutError> {
    let mut child = command.spawn().map_err(|_| CheckoutError::Unavailable)?;
    loop {
        if request.cancel.load(Ordering::Acquire) {
            terminate(&mut child);
            return Err(CheckoutError::Cancelled);
        }
        if Instant::now() >= deadline {
            terminate(&mut child);
            return Err(CheckoutError::Offline);
        }
        if let Some(root) = watch {
            if enforce_bounds(root, &request.limits) == Err(CheckoutError::ResourceLimit) {
                terminate(&mut child);
                return Err(CheckoutError::ResourceLimit);
            }
        }
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(_)) => return Err(CheckoutError::Unavailable),
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(_) => {
                terminate(&mut child);
                return Err(CheckoutError::Unavailable);
            }
        }
    }
}

fn run_bounded_capture(
    mut command: Command,
    output: &Path,
    request: &CheckoutRequest,
    deadline: Instant,
    watch: Option<&Path>,
) -> Result<(), CheckoutError> {
    let file = fs::File::create(output).map_err(|_| CheckoutError::Unavailable)?;
    command.stdout(Stdio::from(file));
    run_bounded_until(command, request, deadline, watch)?;
    let metadata = fs::metadata(output).map_err(|_| CheckoutError::Unavailable)?;
    if metadata.len() > 256 {
        return Err(CheckoutError::ResourceLimit);
    }
    Ok(())
}

fn terminate(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn enforce_bounds(root: &Path, limits: &CheckoutLimits) -> Result<(), CheckoutError> {
    let mut bytes = 0_u64;
    let mut files = 0_u64;
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        let metadata = fs::symlink_metadata(&path).map_err(|_| CheckoutError::Unavailable)?;
        if metadata.file_type().is_symlink() {
            return Err(CheckoutError::ResourceLimit);
        }
        if metadata.is_file() {
            files = files.saturating_add(1);
            bytes = bytes.saturating_add(metadata.len());
        } else if metadata.is_dir() {
            for entry in fs::read_dir(path).map_err(|_| CheckoutError::Unavailable)? {
                pending.push(entry.map_err(|_| CheckoutError::Unavailable)?.path());
            }
        }
        if files > limits.max_files || bytes > limits.max_bytes {
            return Err(CheckoutError::ResourceLimit);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool_catalog::{OfficialSource, ToolEntry};
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture_repository() -> (PathBuf, String, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "asb-source-checkout-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = root.join("source");
        let staging = root.join("staging");
        std::fs::create_dir_all(&source).unwrap();
        let git = |args: &[&str], cwd: &Path| {
            let status = Command::new("git")
                .args(args)
                .current_dir(cwd)
                .status()
                .unwrap();
            assert!(status.success());
        };
        git(["init", "-q"].as_slice(), &source);
        git(
            ["config", "user.email", "test@example.invalid"].as_slice(),
            &source,
        );
        git(["config", "user.name", "ASB test"].as_slice(), &source);
        std::fs::write(source.join("README"), "fixture\n").unwrap();
        git(["add", "README"].as_slice(), &source);
        git(["commit", "-qm", "fixture"].as_slice(), &source);
        let revision = String::from_utf8(
            Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(&source)
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .to_owned();
        (source, revision, staging)
    }

    fn tool(url: &str, revision: &str) -> ToolEntry {
        ToolEntry {
            id: "fixture".into(),
            kind: "agent".into(),
            development_only: true,
            official_source: OfficialSource {
                url: url.into(),
                version: "1".into(),
                revision: revision.into(),
            },
            platforms: vec![],
            alternatives: vec![],
            entrypoint: "fixture".into(),
            license: "MIT".into(),
            dependencies: vec![],
            build_recipes: vec![],
        }
    }

    #[test]
    fn catalog_identity_requires_https_and_full_revision() {
        assert!(matches!(
            CheckoutRequest::for_tool(
                &tool("ftp://example.invalid/x", &"a".repeat(40)),
                PathBuf::from("/tmp/x")
            ),
            Err(CheckoutError::InvalidRequest)
        ));
        assert!(
            CheckoutRequest::for_tool(
                &tool("https://example.invalid/x", &"a".repeat(40)),
                PathBuf::from("/tmp/x")
            )
            .is_ok()
        );
    }

    #[test]
    fn limits_and_cancel_are_typed_before_or_during_acquisition() {
        let mut request = CheckoutRequest::for_tool(
            &tool("https://example.invalid/x", &"a".repeat(40)),
            PathBuf::from("/tmp/x"),
        )
        .unwrap();
        request.limits.max_files = 0;
        assert_eq!(acquire(&request), Err(CheckoutError::InvalidRequest));
        request.limits = CheckoutLimits::default();
        request.cancel.store(true, Ordering::Release);
        assert!(matches!(
            acquire(&request),
            Err(CheckoutError::Unavailable | CheckoutError::Cancelled)
        ));
    }

    #[test]
    fn wrong_revision_and_credential_bearing_repository_fail_closed() {
        assert!(matches!(
            CheckoutRequest::for_tool(
                &tool("https://user:pass@example.invalid/repo", &"a".repeat(40)),
                PathBuf::from("/tmp/x")
            ),
            Err(CheckoutError::InvalidRequest)
        ));
        assert!(matches!(
            CheckoutRequest::for_tool(
                &tool("https://example.invalid/repo", "moving-ref"),
                PathBuf::from("/tmp/x")
            ),
            Err(CheckoutError::InvalidRequest)
        ));
    }

    #[test]
    fn deadline_and_live_tree_limits_are_enforced() {
        let request = CheckoutRequest::for_tool(
            &tool("https://example.invalid/repo", &"a".repeat(40)),
            PathBuf::from("/tmp/x"),
        )
        .unwrap();
        let mut command = Command::new("sleep");
        command.arg("1");
        assert_eq!(
            run_bounded_until(command, &request, Instant::now(), None),
            Err(CheckoutError::Offline)
        );
        let root = std::env::temp_dir().join(format!("asb-source-limit-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("large"), vec![0_u8; 8]).unwrap();
        let mut limited = request.limits.clone();
        limited.max_bytes = 4;
        assert_eq!(
            enforce_bounds(&root, &limited),
            Err(CheckoutError::ResourceLimit)
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn exact_fixture_checkout_is_returned_and_cleanup_is_explicit() {
        let (source, revision, staging) = fixture_repository();
        let mut request = CheckoutRequest::for_tool(
            &tool(&format!("file://{}", source.display()), &revision),
            staging.clone(),
        )
        .unwrap();
        request.repository_url = format!("file://{}", source.display());
        let checkout = acquire(&request).unwrap();
        assert_eq!(checkout.revision, revision);
        assert!(checkout.path.join("README").is_file());
        checkout.cleanup().unwrap();
        let _ = std::fs::remove_dir_all(source.parent().unwrap());
    }
}
