//! Exclusive test-only namespace allocation; a clock sample is not ownership.
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT: AtomicU64 = AtomicU64::new(0);
const ATTEMPTS: usize = 64;

pub fn allocate(parent: &Path, prefix: &str) -> io::Result<PathBuf> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_nanos();
    allocate_at(parent, prefix, stamp, &NEXT)
}

fn candidate(parent: &Path, prefix: &str, stamp: u128, sequence: u64) -> PathBuf {
    parent.join(format!("{prefix}{}-{stamp}-{sequence}", std::process::id()))
}

fn allocate_at(parent: &Path, prefix: &str, stamp: u128, next: &AtomicU64) -> io::Result<PathBuf> {
    for _ in 0..ATTEMPTS {
        let path = candidate(parent, prefix, stamp, next.fetch_add(1, Ordering::Relaxed));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "exclusive fixture namespace candidates exhausted",
    ))
}

#[cfg(unix)]
#[test]
fn same_clock_concurrent_allocations_never_adopt_existing_directories() {
    use std::os::unix::fs::MetadataExt;
    use std::sync::Arc;
    let parent = allocate(&std::env::temp_dir(), "asb-fixture-allocation-").unwrap();
    let stamp = 17;
    let prefix = "same-clock-";
    let foreign = candidate(&parent, prefix, stamp, 0);
    fs::create_dir(&foreign).unwrap();
    fs::write(foreign.join("sentinel"), b"preserve preexisting directory").unwrap();
    let original = fs::metadata(&foreign).unwrap();
    let next = Arc::new(AtomicU64::new(0));
    let workers = (0..2)
        .map(|_| {
            let parent = parent.clone();
            let next = Arc::clone(&next);
            std::thread::spawn(move || allocate_at(&parent, prefix, stamp, &next).unwrap())
        })
        .collect::<Vec<_>>();
    let roots = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect::<Vec<_>>();
    let identities = roots
        .iter()
        .map(|root| {
            let metadata = fs::metadata(root).unwrap();
            (metadata.dev(), metadata.ino())
        })
        .collect::<Vec<_>>();
    assert_ne!(roots[0], roots[1]);
    assert_ne!(identities[0], identities[1]);
    for identity in identities {
        assert_ne!(identity, (original.dev(), original.ino()));
    }
    assert_eq!(
        fs::read(foreign.join("sentinel")).unwrap(),
        b"preserve preexisting directory"
    );
    let after = fs::metadata(&foreign).unwrap();
    assert_eq!((original.dev(), original.ino()), (after.dev(), after.ino()));
    let exhausted = parent.join("exhausted");
    fs::create_dir(&exhausted).unwrap();
    for sequence in 0..ATTEMPTS as u64 {
        let existing = candidate(&exhausted, prefix, stamp, sequence);
        fs::create_dir(&existing).unwrap();
        fs::write(existing.join("sentinel"), sequence.to_string()).unwrap();
    }
    assert_eq!(
        allocate_at(&exhausted, prefix, stamp, &AtomicU64::new(0))
            .unwrap_err()
            .kind(),
        io::ErrorKind::AlreadyExists
    );
    for sequence in 0..ATTEMPTS as u64 {
        assert_eq!(
            fs::read_to_string(candidate(&exhausted, prefix, stamp, sequence).join("sentinel"))
                .unwrap(),
            sequence.to_string()
        );
    }
    let blocked = parent.join("not-a-directory");
    fs::write(&blocked, b"preserve blocking file").unwrap();
    let attempts = AtomicU64::new(0);
    assert_eq!(
        allocate_at(&blocked, prefix, stamp, &attempts)
            .unwrap_err()
            .kind(),
        io::ErrorKind::NotADirectory
    );
    assert_eq!(attempts.load(Ordering::Relaxed), 1);
    assert_eq!(fs::read(blocked).unwrap(), b"preserve blocking file");
    // Only the exact successfully allocated parent is removed, after both
    // bounded allocator workers have joined and the sentinel was checked.
    fs::remove_dir_all(parent).unwrap();
}
