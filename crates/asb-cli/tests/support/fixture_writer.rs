//! Test-only executable creation outside the shared parallel harness process.
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    Arc, Condvar, Mutex, OnceLock,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

const REQUEST: &str = "ASB_TEST_EXECUTABLE_WRITER_BYTES";
const DESTINATION: &str = "ASB_TEST_EXECUTABLE_WRITER_PATH";
const MODE: &str = "ASB_TEST_EXECUTABLE_WRITER_MODE";
const ROOT: &str = "ASB_TEST_EXECUTABLE_WRITER_ROOT";
const BEHAVIOR: &str = "ASB_TEST_EXECUTABLE_WRITER_BEHAVIOR";
const MAX_BYTES: usize = 48 * 1024;
const PENDING: &str = ".asb-test-executable-writer.pending";
const CUSTODY: &str = ".asb-test-executable-writer-custody-";
static NEXT: AtomicU64 = AtomicU64::new(0);
static RETAINED: OnceLock<Mutex<BTreeSet<PathBuf>>> = OnceLock::new();

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    None,
    ChildFailure,
    Timeout,
    Unwind,
    UnknownCleanup,
    MarkerReplacement,
    MarkerMissing,
}
std::thread_local! { static FAULT: std::cell::Cell<Fault> = const { std::cell::Cell::new(Fault::None) }; }
pub fn with_fault<T>(fault: Fault, body: impl FnOnce() -> T) -> T {
    struct Reset(Fault);
    impl Drop for Reset {
        fn drop(&mut self) {
            FAULT.set(self.0);
        }
    }
    let _reset = Reset(FAULT.replace(fault));
    body()
}

fn retained() -> &'static Mutex<BTreeSet<PathBuf>> {
    RETAINED.get_or_init(Mutex::default)
}

#[test]
fn local_fault_scope_restores_the_unmodified_writer_default() {
    assert!(FAULT.get() == Fault::None);
    for fault in [
        Fault::ChildFailure,
        Fault::Timeout,
        Fault::Unwind,
        Fault::UnknownCleanup,
        Fault::MarkerReplacement,
        Fault::MarkerMissing,
    ] {
        with_fault(fault, || assert!(FAULT.get() == fault));
        assert!(FAULT.get() == Fault::None);
    }
}
fn retain(root: &Path) {
    retained()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .insert(root.into());
}

/// Only one keeper owns the actual child after a caller relinquishes custody.
pub trait Keeper {
    fn adopt(&self, child: Child);
}

#[derive(Default)]
struct Slot {
    child: Option<Child>,
    closed: bool,
}
/// Integration harness fallback; library callers supply their existing reaper.
pub struct OneSlotKeeper(Arc<(Mutex<Slot>, Condvar)>);
impl OneSlotKeeper {
    pub fn new() -> io::Result<Self> {
        Self::with_receipt().map(|(keeper, _)| keeper)
    }
    pub fn with_receipt() -> io::Result<(Self, Arc<AtomicBool>)> {
        let state = Arc::new((Mutex::new(Slot::default()), Condvar::new()));
        let worker = Arc::clone(&state);
        let completed = Arc::new(AtomicBool::new(false));
        let completion = Arc::clone(&completed);
        std::thread::Builder::new()
            .name("asb-test-writer-keeper".into())
            .spawn(move || {
                let (slot, ready) = &*worker;
                let mut state = slot.lock().unwrap_or_else(|error| error.into_inner());
                while !state.closed {
                    state = ready.wait(state).unwrap_or_else(|error| error.into_inner());
                }
                let child = state.child.take();
                drop(state);
                if let Some(mut child) = child {
                    // Worker-only custody. Neither a signal result nor a wait
                    // error proves exit: retain this actual Child until confirmed.
                    let mut signaled = false;
                    loop {
                        match child.try_wait() {
                            Ok(Some(_)) => break,
                            Ok(None) if !signaled => {
                                signaled = child.kill().is_ok();
                            }
                            Ok(None) | Err(_) => {}
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
                // Only actual successful try_wait(Some), or no transferred
                // child, permits the pre-established worker's receipt.
                completion.store(true, Ordering::Release);
            })?;
        Ok((Self(state), completed))
    }
}
impl Keeper for OneSlotKeeper {
    fn adopt(&self, child: Child) {
        let mut state = self.0.0.lock().unwrap_or_else(|error| error.into_inner());
        // A private guard transfers at most its one directly admitted child.
        assert!(state.child.is_none());
        state.child = Some(child);
    }
}
impl Drop for OneSlotKeeper {
    fn drop(&mut self) {
        self.0
            .0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .closed = true;
        self.0.1.notify_one();
    }
}

fn identity(metadata: &fs::Metadata) -> (u64, u64) {
    (metadata.dev(), metadata.ino())
}
struct Marker {
    root: PathBuf,
    root_identity: (u64, u64),
    file: File,
    pending: PathBuf,
    custody: PathBuf,
}
impl Marker {
    fn new(root: &Path) -> io::Result<Self> {
        let metadata = fs::symlink_metadata(root)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(io::Error::other(
                "fixture namespace is not its owned directory",
            ));
        }
        let custody = root.join(format!(
            "{CUSTODY}{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&custody)?;
        retain(root);
        let pending = root.join(PENDING);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&pending)?;
        file.write_all(b"pending test executable writer\n")?;
        Ok(Self {
            root: root.into(),
            root_identity: identity(&metadata),
            file,
            pending,
            custody,
        })
    }
    fn finish(self) -> io::Result<()> {
        if identity(&fs::symlink_metadata(&self.root)?) != self.root_identity {
            return Err(io::Error::other(
                "fixture namespace changed; writer evidence retained",
            ));
        }
        // Consume the exposed name into quiet private custody BEFORE checking
        // identity and deleting. Never check then unlink the exposed pathname.
        let staged = self.custody.join("settled-marker");
        fs::rename(&self.pending, &staged)?;
        let metadata = fs::symlink_metadata(&staged)?;
        if !metadata.is_file() || identity(&metadata) != identity(&self.file.metadata()?) {
            return Err(io::Error::other(
                "fixture writer marker changed; candidate retained",
            ));
        }
        fs::remove_file(staged)?;
        fs::remove_dir(&self.custody)?;
        retained()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&self.root);
        Ok(())
    }
}

/// Failure/unwind retention is shared by explicit cleanup and owning Drop.
/// Absence of the public marker alone never proves an uncertain write settled.
pub fn cleanup(root: &Path) -> io::Result<bool> {
    if std::thread::panicking()
        || retained()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .any(|path| path.starts_with(root))
    {
        return Ok(false);
    }
    // A different process may own a failed nested writer. Bounded conservative
    // traversal, never symlink following, protects enclosing fixture cleanup.
    let mut directories = vec![(root.to_path_buf(), 0)];
    let mut entries_seen = 0;
    let mut path_bytes = root.as_os_str().as_bytes().len();
    while let Some((directory, depth)) = directories.pop() {
        let metadata = match fs::symlink_metadata(&directory) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound && directory == root => {
                return Ok(true);
            }
            Err(_) => return Ok(false),
        };
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Ok(false);
        }
        let Ok(entries) = fs::read_dir(&directory) else {
            return Ok(false);
        };
        for entry in entries {
            entries_seen += 1;
            if entries_seen > 16_384 {
                return Ok(false);
            }
            let Ok(entry) = entry else { return Ok(false) };
            let name = entry.file_name();
            if name == PENDING || name.as_bytes().starts_with(CUSTODY.as_bytes()) {
                return Ok(false);
            }
            let Ok(kind) = entry.file_type() else {
                return Ok(false);
            };
            if kind.is_dir() {
                if depth >= 64 {
                    return Ok(false);
                }
                let path = entry.path();
                path_bytes += path.as_os_str().as_bytes().len();
                if path_bytes > 4 * 1024 * 1024 {
                    return Ok(false);
                }
                directories.push((path, depth + 1));
            }
        }
    }
    fs::remove_dir_all(root).map(|()| true)
}

struct Owned<K: Keeper> {
    child: Option<Child>,
    keeper: K,
    root: PathBuf,
}
impl<K: Keeper> Owned<K> {
    fn abort(&mut self) -> io::Result<()> {
        let Some(child) = self.child.as_mut() else {
            return Ok(());
        };
        if child.try_wait()?.is_none() {
            child.kill()?;
        }
        let end = Instant::now() + Duration::from_secs(2);
        loop {
            if child.try_wait()?.is_some() {
                self.child = None;
                return Ok(());
            }
            if Instant::now() >= end {
                return Err(io::Error::other(
                    "fixture writer cleanup unconfirmed; custody retained",
                ));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
impl<K: Keeper> Drop for Owned<K> {
    fn drop(&mut self) {
        if let Some(child) = self.child.take() {
            retain(&self.root);
            self.keeper.adopt(child);
        }
    }
}

/// Shared integration entry; the library wrapper instead supplies OwnedReaper.
pub fn write_file(root: &Path, path: &Path, bytes: impl AsRef<[u8]>) -> io::Result<()> {
    write(root, path, bytes.as_ref(), None)
}

/// Shared integration entry; the library wrapper instead supplies OwnedReaper.
pub fn write(root: &Path, path: &Path, bytes: &[u8], mode: Option<u32>) -> io::Result<()> {
    write_with(root, path, bytes, mode, OneSlotKeeper::new)
}

/// Parent never opens the executable for writing. The one exact child does.
pub fn write_with<K: Keeper>(
    root: &Path,
    path: &Path,
    bytes: &[u8],
    mode: Option<u32>,
    keeper: impl FnOnce() -> io::Result<K>,
) -> io::Result<()> {
    if bytes.len() > MAX_BYTES
        || path.as_os_str().as_bytes().len() > 16 * 1024
        || mode.is_some_and(|mode| mode > 0o7777)
    {
        return Err(io::Error::other(
            "fixture writer request exceeds its bounds",
        ));
    }
    if retained()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .any(|path| path.starts_with(root) || root.starts_with(path))
    {
        return Err(io::Error::other(
            "fixture namespace retained after an earlier writer failure",
        ));
    }
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        write!(&mut encoded, "{byte:02x}").unwrap();
    }
    let name = format!(
        "{}::writer_child",
        module_path!().split_once("::").unwrap().1
    );
    let mut command = Command::new(std::env::current_exe()?);
    let fault = FAULT.get();
    let held = matches!(
        fault,
        Fault::Timeout | Fault::Unwind | Fault::UnknownCleanup
    );
    command
        .env_clear()
        .args(["--exact", &name])
        .env(REQUEST, encoded)
        .env(DESTINATION, path)
        .env(ROOT, root)
        .env(
            BEHAVIOR,
            if held {
                "held"
            } else if fault == Fault::ChildFailure {
                "failure"
            } else {
                "normal"
            },
        )
        .env(
            MODE,
            mode.map_or_else(|| "preserve".into(), |value| value.to_string()),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
        command.env("LLVM_PROFILE_FILE", profile);
    }
    let end = Instant::now()
        + if fault == Fault::Timeout {
            Duration::from_millis(150)
        } else {
            Duration::from_secs(5)
        };
    let keeper = keeper()?;
    let marker = Marker::new(root)?;
    if Instant::now() >= end {
        return Err(io::Error::other("fixture writer expired before admission"));
    }
    let child = command.spawn()?;
    let mut owned = Owned {
        child: Some(child),
        keeper,
        root: root.into(),
    };
    if matches!(fault, Fault::Unwind | Fault::UnknownCleanup) {
        while !root.join("writer-child-started").exists() && Instant::now() < end {
            if owned.child.as_mut().unwrap().try_wait()?.is_some() {
                return Err(io::Error::other("fixture fault child exited before hold"));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        if !root.join("writer-child-started").exists() {
            return Err(io::Error::other("fixture fault child never held"));
        }
        if fault == Fault::Unwind {
            panic!("fixture writer unwind after actual admission")
        }
        return Err(io::Error::other(
            "fixture writer cleanup deliberately unconfirmed; custody retained",
        ));
    }
    let result = loop {
        match owned.child.as_mut().unwrap().try_wait() {
            Ok(Some(status)) => {
                owned.child = None;
                break if status.success() {
                    Ok(())
                } else {
                    Err(io::Error::other(
                        "fixture writer failed; namespace retained",
                    ))
                };
            }
            Err(_) => {
                break Err(io::Error::other(
                    "fixture writer exit unconfirmed; namespace retained",
                ));
            }
            Ok(None) => {}
        }
        if Instant::now() >= end {
            break Err(io::Error::other(
                "fixture writer deadline exceeded; namespace retained",
            ));
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    if let Err(error) = result {
        let _cleanup = owned.abort(); // Guard retains the actual child on uncertainty.
        return Err(error);
    }
    if fault == Fault::MarkerReplacement {
        fs::rename(&marker.pending, root.join("original-marker-retained"))?;
        fs::write(&marker.pending, b"foreign marker bytes")?;
    } else if fault == Fault::MarkerMissing {
        fs::remove_file(&marker.pending)?;
    }
    marker.finish()
}

#[test]
fn writer_child() {
    let Some(encoded) = std::env::var_os(REQUEST) else {
        return;
    };
    let encoded = encoded.into_string().expect("bounded hex fixture request");
    assert!(encoded.len() <= MAX_BYTES * 2 && encoded.len().is_multiple_of(2));
    let bytes = encoded
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let digit = |byte: u8| (byte as char).to_digit(16).expect("hex fixture byte") as u8;
            digit(pair[0]) * 16 + digit(pair[1])
        })
        .collect::<Vec<_>>();
    let mode = std::env::var(MODE).unwrap();
    let mode = if mode == "preserve" {
        None
    } else {
        Some(mode.parse::<u32>().unwrap())
    };
    assert!(mode.is_none_or(|mode| mode <= 0o7777));
    let path = std::env::var_os(DESTINATION).unwrap();
    let behavior = std::env::var(BEHAVIOR).unwrap();
    if behavior == "held" {
        let root = PathBuf::from(std::env::var_os(ROOT).unwrap());
        fs::write(
            root.join("writer-child-started"),
            std::process::id().to_string(),
        )
        .unwrap();
        let end = Instant::now() + Duration::from_secs(10);
        while !root.join("writer-child-release").exists() {
            assert!(Instant::now() < end, "held writer fixture expired");
            std::thread::sleep(Duration::from_millis(5));
        }
    } else if behavior == "failure" {
        panic!("fixture child write failure")
    }
    fs::write(&path, bytes).expect("fixture child write");
    if let Some(mode) = mode {
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("fixture child mode")
    }
}
