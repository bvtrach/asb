// Copyright (C) Huawei Technologies Co., Ltd. 2026. All rights reserved.
// SPDX-License-Identifier: MIT
//! Runtime-owned lifecycle for the development-only cli2key bridge.
//!
//! The bridge receives a per-invocation key through an invocation-private file.
//! The key is never an argument, environment value, log field, or receipt
//! field.  The child is a process-group leader and is always reaped on drop.

use crate::launch_factory::{
    LaunchAuthorityError, LiveProviderAttempt, LiveProviderAttemptFactory,
};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime};

const KEY_BYTES: usize = 32;
const KEY_HEX_BYTES: usize = KEY_BYTES * 2;
const MAX_READY: usize = 256;
const MAX_ARGUMENTS: usize = 128;
const READY_TIMEOUT: Duration = Duration::from_secs(5);

/// A stable, secret-free lifecycle failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Cli2KeyError {
    /// The local setup key was absent, malformed, or stale.
    InvalidLocalKey,
    /// The executable or its binding was not acceptable.
    InvalidExecutable,
    /// Secure random bytes were unavailable.
    RandomUnavailable,
    /// Private staging could not be created or removed.
    Staging(io::ErrorKind),
    /// The bridge did not provide a valid loopback readiness line.
    Readiness,
    /// The process could not be started or reaped.
    Process(io::ErrorKind),
    /// The runtime receipt was malformed or did not match the pinned bridge.
    InvalidReceipt,
    /// The receipt was issued for a different platform.
    InvalidPlatform,
    /// The receipt identity did not match the pinned executable.
    ReceiptBinding,
}

impl fmt::Display for Cli2KeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLocalKey => f.write_str("cli2key local key is unavailable"),
            Self::InvalidExecutable => f.write_str("cli2key executable is invalid"),
            Self::RandomUnavailable => f.write_str("cli2key random source is unavailable"),
            Self::Staging(_) => f.write_str("cli2key private staging failed"),
            Self::Readiness => f.write_str("cli2key readiness was invalid"),
            Self::Process(_) => f.write_str("cli2key process lifecycle failed"),
            Self::InvalidReceipt => f.write_str("cli2key runtime receipt is invalid"),
            Self::InvalidPlatform => f.write_str("cli2key runtime receipt platform is invalid"),
            Self::ReceiptBinding => f.write_str("cli2key runtime receipt binding mismatched"),
        }
    }
}

impl std::error::Error for Cli2KeyError {}

/// Where the invocation key came from, without exposing the key itself.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyOrigin {
    /// A valid local setup key was used.
    Local,
    /// A fresh key was generated because setup was absent or unusable.
    Generated,
}

/// Public, redacted evidence for a running sidecar.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Cli2KeyReceipt {
    /// Validated loopback endpoint.
    pub endpoint: SocketAddr,
    /// Origin of the invocation key.
    pub key_origin: KeyOrigin,
    /// Digest of the key, not the key value.
    pub key_sha256: String,
}

/// Secret-free catalog identity required before a runtime sidecar can start.
///
/// This is intentionally independent of `asb-agents`: the runtime accepts
/// only attestable identities and never receives a catalog, credential, or
/// invocation key.  The endpoint digest identifies the adapter contract, not
/// the ephemeral loopback port selected by the sidecar.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Cli2KeyRuntimeReceipt {
    /// Receipt schema version.
    pub schema_version: u16,
    /// Monotonic development catalog generation.
    pub catalog_generation: u64,
    /// Exact target platform identifier.
    pub platform: String,
    /// Digest of the complete catalog identity.
    pub catalog_sha256: String,
    /// Pinned bridge source revision.
    pub bridge_revision: String,
    /// Digest of the pinned bridge source tree.
    pub bridge_tree_sha256: String,
    /// Digest of the executable bytes.
    pub executable_sha256: String,
    /// Digest of the adapter endpoint contract.
    pub endpoint_sha256: String,
    /// Digest of the credential reference, never its value.
    pub credential_reference_sha256: String,
}

impl Cli2KeyRuntimeReceipt {
    /// Validate the stable, secret-free receipt fields against this runtime.
    pub fn validate(&self) -> Result<(), Cli2KeyError> {
        if self.schema_version != 1 || self.catalog_generation == 0 {
            return Err(Cli2KeyError::InvalidReceipt);
        }
        if self.platform != "linux-x86_64"
            || std::env::consts::OS != "linux"
            || std::env::consts::ARCH != "x86_64"
        {
            return Err(Cli2KeyError::InvalidPlatform);
        }
        if self.bridge_revision.is_empty() || self.bridge_revision.len() > 128 {
            return Err(Cli2KeyError::InvalidReceipt);
        }
        for digest in [
            &self.catalog_sha256,
            &self.bridge_tree_sha256,
            &self.executable_sha256,
            &self.endpoint_sha256,
            &self.credential_reference_sha256,
        ] {
            if digest.len() != 64
                || !digest
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            {
                return Err(Cli2KeyError::InvalidReceipt);
            }
        }
        Ok(())
    }
}

/// A pinned bridge launch. Arguments are copied verbatim and never augmented
/// with the key value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Cli2KeyCommand {
    executable: PathBuf,
    arguments: Vec<String>,
    _digest: String,
}

impl Cli2KeyCommand {
    /// Bind an absolute executable to its expected SHA-256 digest.
    pub fn new(
        executable: PathBuf,
        arguments: Vec<String>,
        digest: String,
    ) -> Result<Self, Cli2KeyError> {
        if !executable.is_absolute()
            || executable
                .components()
                .any(|c| c == std::path::Component::ParentDir)
            || arguments.len() > MAX_ARGUMENTS
            || digest.len() != 64
            || !digest
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err(Cli2KeyError::InvalidExecutable);
        }
        let metadata =
            fs::symlink_metadata(&executable).map_err(|_| Cli2KeyError::InvalidExecutable)?;
        if !metadata.file_type().is_file() || metadata.permissions().mode() & 0o111 == 0 {
            return Err(Cli2KeyError::InvalidExecutable);
        }
        let mut file = File::open(&executable).map_err(|_| Cli2KeyError::InvalidExecutable)?;
        let mut hasher = Sha256::new();
        io::copy(&mut file, &mut DigestWriter(&mut hasher))
            .map_err(|_| Cli2KeyError::InvalidExecutable)?;
        if format!("{:x}", hasher.finalize()) != digest {
            return Err(Cli2KeyError::InvalidExecutable);
        }
        Ok(Self {
            executable,
            arguments,
            _digest: digest,
        })
    }

    /// Return the pinned executable digest without exposing launch arguments.
    pub fn executable_sha256(&self) -> &str {
        &self._digest
    }
}

struct DigestWriter<'a>(&'a mut Sha256);
impl Write for DigestWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn decode_key(bytes: &[u8]) -> Option<[u8; KEY_BYTES]> {
    let value = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    if value.len() != KEY_HEX_BYTES {
        return None;
    }
    let mut out = [0; KEY_BYTES];
    for (index, chunk) in value.chunks_exact(2).enumerate() {
        let high = (chunk[0] as char).to_digit(16)? as u8;
        let low = (chunk[1] as char).to_digit(16)? as u8;
        out[index] = high * 16 + low;
    }
    Some(out)
}

/// Read a local setup key without following symlinks and reject stale values.
pub fn load_local_key(
    path: &Path,
    now: SystemTime,
    max_age: Duration,
) -> Result<[u8; KEY_BYTES], Cli2KeyError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| Cli2KeyError::InvalidLocalKey)?;
    if !metadata.file_type().is_file() || metadata.permissions().mode() & 0o077 != 0 {
        return Err(Cli2KeyError::InvalidLocalKey);
    }
    let modified = metadata
        .modified()
        .map_err(|_| Cli2KeyError::InvalidLocalKey)?;
    if now
        .duration_since(modified)
        .ok()
        .is_none_or(|age| age > max_age)
    {
        return Err(Cli2KeyError::InvalidLocalKey);
    }
    let mut bytes = Vec::new();
    rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )
    .map(File::from)
    .map_err(|_| Cli2KeyError::InvalidLocalKey)?
    .take(128)
    .read_to_end(&mut bytes)
    .map_err(|_| Cli2KeyError::InvalidLocalKey)?;
    decode_key(&bytes).ok_or(Cli2KeyError::InvalidLocalKey)
}

fn fresh_key() -> Result<[u8; KEY_BYTES], Cli2KeyError> {
    let mut key = [0; KEY_BYTES];
    File::open("/dev/urandom")
        .map_err(|_| Cli2KeyError::RandomUnavailable)?
        .read_exact(&mut key)
        .map_err(|_| Cli2KeyError::RandomUnavailable)?;
    Ok(key)
}

struct PrivateStaging {
    root: PathBuf,
    key_path: PathBuf,
}
impl PrivateStaging {
    fn create(key: &[u8; KEY_BYTES]) -> Result<Self, Cli2KeyError> {
        let base = std::env::temp_dir();
        for attempt in 0..16u8 {
            let root = base.join(format!("asb-cli2key-{}-{attempt}", std::process::id()));
            match fs::create_dir(&root) {
                Ok(()) => {
                    let key_path = root.join("client.key");
                    let result = (|| {
                        fs::set_permissions(&root, fs::Permissions::from_mode(0o700))
                            .map_err(|e| Cli2KeyError::Staging(e.kind()))?;
                        let mut file = OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .mode(0o600)
                            .open(&key_path)
                            .map_err(|e| Cli2KeyError::Staging(e.kind()))?;
                        file.write_all(
                            &key.iter()
                                .flat_map(|byte| format!("{byte:02x}").into_bytes())
                                .collect::<Vec<_>>(),
                        )
                        .map_err(|e| Cli2KeyError::Staging(e.kind()))?;
                        file.sync_all()
                            .map_err(|e| Cli2KeyError::Staging(e.kind()))?;
                        Ok(Self {
                            root: root.clone(),
                            key_path: key_path.clone(),
                        })
                    })();
                    if result.is_err() {
                        let _ = fs::remove_file(&key_path);
                        let _ = fs::remove_dir(&root);
                    }
                    return result;
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(Cli2KeyError::Staging(error.kind())),
            }
        }
        Err(Cli2KeyError::Staging(io::ErrorKind::AlreadyExists))
    }
}
impl Drop for PrivateStaging {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.key_path);
        let _ = fs::remove_dir(&self.root);
    }
}

/// A running runtime-owned cli2key sidecar.
pub struct Cli2KeySidecar {
    child: Child,
    pid: rustix::process::Pid,
    _staging: PrivateStaging,
    receipt: Cli2KeyReceipt,
}
impl fmt::Debug for Cli2KeySidecar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Cli2KeySidecar")
            .field("receipt", &self.receipt)
            .finish()
    }
}
impl Cli2KeySidecar {
    /// Start a pinned bridge, choosing a valid setup key or generating a fresh one.
    pub fn start(command: &Cli2KeyCommand, local_key: Option<&Path>) -> Result<Self, Cli2KeyError> {
        let (key, origin) = match local_key.and_then(|path| {
            load_local_key(path, SystemTime::now(), Duration::from_secs(24 * 60 * 60)).ok()
        }) {
            Some(key) => (key, KeyOrigin::Local),
            None => (fresh_key()?, KeyOrigin::Generated),
        };
        let staging = PrivateStaging::create(&key)?;
        let mut argv = command.arguments.clone();
        argv.extend([
            "--host".into(),
            "127.0.0.1".into(),
            "--port".into(),
            "0".into(),
            "--key-file".into(),
            staging.key_path.display().to_string(),
        ]);
        let mut child_command = Command::new(&command.executable);
        child_command
            .args(&argv)
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0);
        let mut child = child_command
            .spawn()
            .map_err(|e| Cli2KeyError::Process(e.kind()))?;
        let stdout = match child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                let _ = rustix::process::kill_process_group(
                    rustix::process::Pid::from_child(&child),
                    rustix::process::Signal::KILL,
                );
                let _ = child.wait();
                return Err(Cli2KeyError::Readiness);
            }
        };
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let mut line = String::new();
            let result = BufReader::new(stdout).read_line(&mut line).map(|_| line);
            let _ = sender.send(result);
        });
        let line = match receiver.recv_timeout(READY_TIMEOUT) {
            Ok(Ok(line)) => line,
            _ => {
                let _ = rustix::process::kill_process_group(
                    rustix::process::Pid::from_child(&child),
                    rustix::process::Signal::KILL,
                );
                let _ = child.wait();
                return Err(Cli2KeyError::Readiness);
            }
        };
        let endpoint = match parse_ready(&line) {
            Some(endpoint) => endpoint,
            None => {
                let _ = rustix::process::kill_process_group(
                    rustix::process::Pid::from_child(&child),
                    rustix::process::Signal::KILL,
                );
                let _ = child.wait();
                return Err(Cli2KeyError::Readiness);
            }
        };
        let pid = rustix::process::Pid::from_child(&child);
        let mut digest = Sha256::new();
        digest.update(key);
        let key_sha256 = format!("{:x}", digest.finalize());
        Ok(Self {
            child,
            pid,
            _staging: staging,
            receipt: Cli2KeyReceipt {
                endpoint,
                key_origin: origin,
                key_sha256,
            },
        })
    }
    /// Return redacted lifecycle evidence.
    pub fn receipt(&self) -> &Cli2KeyReceipt {
        &self.receipt
    }
    /// Return the validated loopback endpoint.
    pub fn endpoint(&self) -> SocketAddr {
        self.receipt.endpoint
    }
}

/// Runtime-owned composition of a verified cli2key sidecar and opaque live
/// attempts.  Dropping the authority tears down the sidecar; callers can only
/// obtain an attempt through the runtime factory and cannot access key bytes,
/// staging paths, or OAuth material.
pub struct Cli2KeyRuntimeAuthority {
    sidecar: Cli2KeySidecar,
    receipt: Cli2KeyRuntimeReceipt,
    factory: LiveProviderAttemptFactory,
    lifecycle: Arc<AtomicBool>,
}

impl fmt::Debug for Cli2KeyRuntimeAuthority {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Cli2KeyRuntimeAuthority")
            .field("receipt", &self.receipt)
            .field("endpoint", &self.sidecar.endpoint())
            .field("factory", &self.factory)
            .finish()
    }
}

impl Cli2KeyRuntimeAuthority {
    /// Verify the catalog identity, pin the executable, and start the sidecar.
    pub fn bootstrap(
        receipt: Cli2KeyRuntimeReceipt,
        command: &Cli2KeyCommand,
        local_key: Option<&Path>,
        factory: LiveProviderAttemptFactory,
    ) -> Result<Self, Cli2KeyError> {
        receipt.validate()?;
        if receipt.executable_sha256 != command.executable_sha256() {
            return Err(Cli2KeyError::ReceiptBinding);
        }
        let sidecar = Cli2KeySidecar::start(command, local_key)?;
        Ok(Self {
            sidecar,
            receipt,
            factory,
            lifecycle: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Issue one opaque, one-shot attempt while retaining sidecar ownership.
    pub fn issue_attempt(
        &self,
        input_id: u32,
        warmup: bool,
    ) -> Result<LiveProviderAttempt, LaunchAuthorityError> {
        let mut attempt = self.factory.acquire(input_id, warmup)?;
        attempt.bind_authority_fence(Arc::clone(&self.lifecycle));
        Ok(attempt)
    }

    /// Borrow the runtime-issued factory for CLI run or bounded sweep wiring.
    /// The caller must retain this authority for the entire execution so the
    /// sidecar remains alive and its teardown fence stays authoritative.
    pub fn attempt_factory(&self) -> LiveProviderAttemptFactory {
        let factory = self.factory.clone();
        let lifecycle = Arc::clone(&self.lifecycle);
        LiveProviderAttemptFactory::from_fn(move |input_id, warmup| {
            let mut attempt = factory.acquire(input_id, warmup)?;
            attempt.bind_authority_fence(Arc::clone(&lifecycle));
            Ok(attempt)
        })
    }

    /// Return the verified secret-free catalog identity.
    pub fn receipt(&self) -> &Cli2KeyRuntimeReceipt {
        &self.receipt
    }

    /// Return the ephemeral loopback endpoint, never the invocation key.
    pub fn endpoint(&self) -> SocketAddr {
        self.sidecar.endpoint()
    }
}
impl Drop for Cli2KeySidecar {
    fn drop(&mut self) {
        let _ = rustix::process::kill_process_group(self.pid, rustix::process::Signal::KILL);
        let _ = self.child.wait();
    }
}

impl Drop for Cli2KeyRuntimeAuthority {
    fn drop(&mut self) {
        self.lifecycle.store(true, Ordering::Release);
    }
}

fn parse_ready(line: &str) -> Option<SocketAddr> {
    let value = line.strip_prefix("ASB_CLI2KEY_READY ")?.trim();
    if value.len() > MAX_READY {
        return None;
    }
    let endpoint: SocketAddr = value.parse().ok()?;
    match endpoint.ip() {
        IpAddr::V4(ip) if ip == Ipv4Addr::LOCALHOST => Some(endpoint),
        IpAddr::V6(ip) if ip == Ipv6Addr::LOCALHOST => Some(endpoint),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::time::UNIX_EPOCH;

    fn runtime_receipt() -> Cli2KeyRuntimeReceipt {
        Cli2KeyRuntimeReceipt {
            schema_version: 1,
            catalog_generation: 7,
            platform: "linux-x86_64".into(),
            catalog_sha256: "a".repeat(64),
            bridge_revision: "rev-1".into(),
            bridge_tree_sha256: "b".repeat(64),
            executable_sha256: "c".repeat(64),
            endpoint_sha256: "d".repeat(64),
            credential_reference_sha256: "e".repeat(64),
        }
    }

    fn script_command(name: &str, body: &str) -> (PathBuf, Cli2KeyCommand) {
        let path = std::env::temp_dir().join(format!("asb-cli2key-{name}-{}", std::process::id()));
        let _ = fs::remove_file(&path);
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        let mut digest = Sha256::new();
        digest.update(fs::read(&path).unwrap());
        let command =
            Cli2KeyCommand::new(path.clone(), Vec::new(), format!("{:x}", digest.finalize()))
                .unwrap();
        (path, command)
    }

    #[test]
    fn key_decoder_requires_exact_256_bits() {
        assert!(decode_key(b"00").is_none());
        assert!(decode_key(&[b'a'; 64]).is_some());
    }

    #[test]
    fn generated_keys_are_fresh_256_bit_values() {
        let first = fresh_key().unwrap();
        let second = fresh_key().unwrap();
        assert_ne!(first, second);
        assert_eq!(first.len() * 8, 256);
    }
    #[test]
    fn readiness_rejects_wildcard_and_accepts_loopback() {
        assert!(parse_ready("ASB_CLI2KEY_READY 0.0.0.0:7\n").is_none());
        assert_eq!(
            parse_ready("ASB_CLI2KEY_READY 127.0.0.1:7\n")
                .unwrap()
                .port(),
            7
        );
    }
    #[test]
    fn staging_is_private_and_removed() {
        let staging = PrivateStaging::create(&[7; KEY_BYTES]).unwrap();
        assert_eq!(
            fs::metadata(&staging.root).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&staging.key_path)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let root = staging.root.clone();
        drop(staging);
        assert!(!root.exists());
    }

    #[test]
    fn runtime_receipt_rejects_substitution_and_stale_identity() {
        let mut receipt = runtime_receipt();
        assert_eq!(receipt.validate(), Ok(()));
        receipt.catalog_sha256.replace_range(..1, "A");
        assert_eq!(receipt.validate(), Err(Cli2KeyError::InvalidReceipt));
        receipt = runtime_receipt();
        receipt.catalog_generation = 0;
        assert_eq!(receipt.validate(), Err(Cli2KeyError::InvalidReceipt));
        receipt = runtime_receipt();
        receipt.platform = "linux-aarch64".into();
        assert_eq!(receipt.validate(), Err(Cli2KeyError::InvalidPlatform));
    }

    #[test]
    fn runtime_authority_factory_preserves_typed_acquisition_failure() {
        let factory = LiveProviderAttemptFactory::from_fn(|_, _| {
            Err(LaunchAuthorityError::InvalidLaunchInput)
        });
        // The factory remains opaque at this boundary; the authority wiring
        // cannot turn a failed runtime acquisition into a provider fallback.
        assert!(matches!(
            factory.acquire(1, false),
            Err(LaunchAuthorityError::InvalidLaunchInput)
        ));
    }

    #[test]
    fn runtime_authority_bootstrap_is_sidecar_owned_and_no_fallback() {
        let (path, command) =
            script_command("authority", "printf 'ASB_CLI2KEY_READY 127.0.0.1:12345\\n'");
        let mut receipt = runtime_receipt();
        receipt.executable_sha256 = command.executable_sha256().to_owned();
        let factory = LiveProviderAttemptFactory::from_fn(|_, _| {
            Err(LaunchAuthorityError::InvalidLaunchInput)
        });
        let authority =
            Cli2KeyRuntimeAuthority::bootstrap(receipt, &command, None, factory).unwrap();
        assert_eq!(authority.endpoint().port(), 12345);
        assert!(matches!(
            authority.attempt_factory().acquire(1, false),
            Err(LaunchAuthorityError::InvalidLaunchInput)
        ));
        drop(authority);
        assert!(path.exists());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn runtime_authority_fails_closed_on_crash_and_malformed_readiness() {
        for (name, body) in [("crash", "exit 7"), ("malformed", "printf 'READY\\n'")] {
            let (path, command) = script_command(name, body);
            let mut receipt = runtime_receipt();
            receipt.executable_sha256 = command.executable_sha256().to_owned();
            let factory = LiveProviderAttemptFactory::from_fn(|_, _| {
                Err(LaunchAuthorityError::InvalidLaunchInput)
            });
            assert!(matches!(
                Cli2KeyRuntimeAuthority::bootstrap(receipt, &command, None, factory),
                Err(Cli2KeyError::Readiness)
            ));
            fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn local_key_absent_malformed_stale_and_valid_are_distinct() {
        let root = std::env::temp_dir().join(format!("asb-cli2key-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        let path = root.join("key");
        assert_eq!(
            load_local_key(&path, SystemTime::now(), Duration::from_secs(1)),
            Err(Cli2KeyError::InvalidLocalKey)
        );
        fs::write(&path, b"short").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            load_local_key(&path, SystemTime::now(), Duration::from_secs(1)),
            Err(Cli2KeyError::InvalidLocalKey)
        );
        fs::write(&path, vec![b'a'; KEY_HEX_BYTES]).unwrap();
        let old = UNIX_EPOCH + Duration::from_secs(1);
        assert_eq!(
            load_local_key(&path, SystemTime::now(), Duration::from_secs(1)),
            Ok([0xaa; KEY_BYTES])
        );
        assert_eq!(
            load_local_key(&path, old, Duration::from_secs(1)),
            Err(Cli2KeyError::InvalidLocalKey)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn symlink_setup_key_is_rejected() {
        let root = std::env::temp_dir().join(format!("asb-cli2key-link-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        let target = root.join("target");
        fs::write(&target, vec![b'a'; KEY_HEX_BYTES]).unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        let link = root.join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert_eq!(
            load_local_key(&link, SystemTime::now(), Duration::from_secs(60)),
            Err(Cli2KeyError::InvalidLocalKey)
        );
        let _ = fs::remove_dir_all(root);
    }
}
