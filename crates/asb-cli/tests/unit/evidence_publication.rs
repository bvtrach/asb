use super::*;
use serde::ser::{Error as _, SerializeMap};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use std::cell::RefCell;
use std::collections::VecDeque;
#[derive(Clone, Copy)]
pub(crate) enum WriteAction {
    Short(usize),
    Interrupted,
    Zero,
    NoSpace,
}
struct Injection {
    path: PathBuf,
    point: Option<FaultPoint>,
    writes: VecDeque<WriteAction>,
    hits: usize,
    replace_capture: bool,
}
thread_local! {
    static FAULT: RefCell<Option<Injection>> = const { RefCell::new(None) };
    static ACTIVE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}
pub(crate) struct Scope;
impl Scope {
    pub(crate) fn new(path: PathBuf, point: Option<FaultPoint>, writes: Vec<WriteAction>) -> Self {
        FAULT.with(|fault| {
            assert!(fault.borrow().is_none());
            *fault.borrow_mut() = Some(Injection {
                path,
                point,
                writes: writes.into(),
                hits: 0,
                replace_capture: false,
            });
        });
        Self
    }
    pub(crate) fn hits(&self) -> usize {
        FAULT.with(|fault| fault.borrow().as_ref().unwrap().hits)
    }
    pub(crate) fn replace_on_capture_move(path: PathBuf) -> Self {
        let scope = Self::new(path, None, vec![]);
        FAULT.with(|fault| fault.borrow_mut().as_mut().unwrap().replace_capture = true);
        scope
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        FAULT.with(|fault| fault.borrow_mut().take());
    }
}
pub(super) struct Active(Option<PathBuf>);
impl Active {
    pub(super) fn new(path: &Path) -> Self {
        Self(ACTIVE.with(|active| active.replace(Some(path.into()))))
    }
}
impl Drop for Active {
    fn drop(&mut self) {
        ACTIVE.with(|active| active.replace(self.0.take()));
    }
}
fn inject<T>(action: impl FnOnce(&mut Injection) -> Option<T>) -> Option<T> {
    ACTIVE.with(|active| {
        FAULT.with(|fault| {
            let mut fault = fault.borrow_mut();
            let fault = fault.as_mut()?;
            if active.borrow().as_ref() == Some(&fault.path) {
                action(fault)
            } else {
                None
            }
        })
    })
}
pub(super) fn checkpoint(point: FaultPoint) -> io::Result<()> {
    if point == FaultPoint::CaptureMove
        && let Some(path) = inject(|fault| {
            fault.replace_capture.then(|| {
                fault.replace_capture = false;
                fault.hits += 1;
                fault.path.clone()
            })
        })
    {
        fs::rename(&path, path.with_extension("original")).unwrap();
        fs::write(&path, b"racing foreign capture").unwrap();
    }
    if point == FaultPoint::Prepared && std::env::var_os("ASB_EVIDENCE_HOLD_COMMIT").is_some() {
        ACTIVE.with(|active| {
            let root = active
                .borrow()
                .as_ref()
                .unwrap()
                .parent()
                .unwrap()
                .to_path_buf();
            fs::write(root.join("stage-owned"), b"ready").unwrap();
            wait(&root.join("release-stage"));
        });
    }
    if inject(|fault| {
        (fault.point == Some(point)).then(|| {
            fault.point.take();
            fault.hits += 1;
        })
    })
    .is_some()
    {
        Err(io::Error::other("injected publication boundary failure"))
    } else {
        Ok(())
    }
}
pub(super) fn write_stage(file: &mut File, bytes: &[u8]) -> io::Result<()> {
    struct Writer<'a>(&'a mut File);
    impl Write for Writer<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            match inject(|fault| {
                let action = fault.writes.pop_front()?;
                fault.hits += 1;
                Some(action)
            }) {
                Some(WriteAction::Short(count)) => self.0.write(&bytes[..count.min(bytes.len())]),
                Some(WriteAction::Interrupted) => Err(io::ErrorKind::Interrupted.into()),
                Some(WriteAction::Zero) => Ok(0),
                Some(WriteAction::NoSpace) => Err(io::Error::from_raw_os_error(libc::ENOSPC)),
                None => self.0.write(bytes),
            }
        }
        fn flush(&mut self) -> io::Result<()> {
            self.0.flush()
        }
    }
    Writer(file).write_all(bytes)
}

pub(super) struct Fixture(pub PathBuf);
impl Fixture {
    pub fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "asb-evidence-{}",
            crate::tools::runtime::unique_id()
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

pub(super) struct BrokenSerialization;
impl Serialize for BrokenSerialization {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("partial", &true)?;
        Err(S::Error::custom("deliberate encoding failure"))
    }
}

#[test]
fn partial_log_serialization_preserves_all_previous_bytes() {
    let fixture = Fixture::new();
    let path = fixture.0.join("run.jsonl");
    let previous = b"{\"previous\":true}\n";
    fs::write(&path, previous).unwrap();
    assert!(append_json(&path, &BrokenSerialization).is_err());
    assert_eq!(
        fs::read(&path).unwrap(),
        previous,
        "partial serialization changed published evidence"
    );
}

fn wait(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "missing fixture handshake {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn two_process_log_writers_publish_complete_records() {
    let Ok(root) = std::env::var("ASB_EVIDENCE_WRITER_FIXTURE") else {
        let fixture = Fixture::new();
        let executable = std::env::current_exe().unwrap();
        let mut first = std::process::Command::new(&executable)
            .args([
                "--exact",
                "evidence::tests::two_process_log_writers_publish_complete_records",
                "--nocapture",
            ])
            .env("ASB_EVIDENCE_WRITER_FIXTURE", &fixture.0)
            .env("ASB_EVIDENCE_FIRST", "1")
            .spawn()
            .unwrap();
        wait(&fixture.0.join("first-prefix"));
        let second = std::process::Command::new(&executable)
            .args([
                "--exact",
                "evidence::tests::two_process_log_writers_publish_complete_records",
                "--nocapture",
            ])
            .env("ASB_EVIDENCE_WRITER_FIXTURE", &fixture.0)
            .output()
            .unwrap();
        assert!(
            second.status.success(),
            "{}",
            String::from_utf8_lossy(&second.stderr)
        );
        fs::write(fixture.0.join("second-finished"), b"ready").unwrap();
        assert!(first.wait().unwrap().success());
        let bytes = fs::read_to_string(fixture.0.join("run.jsonl")).unwrap();
        let records: Vec<serde_json::Value> = bytes
            .lines()
            .map(|line| serde_json::from_str(line).expect("writers interleaved a JSONL record"))
            .collect();
        assert_eq!(records.len(), 2);
        return;
    };
    struct Coordinated(PathBuf);
    impl Serialize for Coordinated {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let mut map = serializer.serialize_map(Some(1))?;
            fs::write(self.0.join("first-prefix"), b"ready").unwrap();
            wait(&self.0.join("second-finished"));
            map.serialize_entry("writer", &1)?;
            map.end()
        }
    }
    let root = PathBuf::from(root);
    if std::env::var_os("ASB_EVIDENCE_FIRST").is_some() {
        append_json(&root.join("run.jsonl"), &Coordinated(root.clone())).unwrap();
    } else {
        append_json(&root.join("run.jsonl"), &serde_json::json!({"writer":2})).unwrap();
    }
}

#[test]
fn staging_write_and_commit_faults_preserve_old_bytes_and_report_confirmation() {
    for (point, actions, committed) in [
        (Some(FaultPoint::CreateFile), vec![], false),
        (
            None,
            vec![WriteAction::Short(3), WriteAction::NoSpace],
            false,
        ),
        (None, vec![WriteAction::Zero], false),
        (Some(FaultPoint::FileSync), vec![], false),
        (Some(FaultPoint::Rename), vec![], false),
        (Some(FaultPoint::DirectorySync), vec![], true),
    ] {
        for append in [false, true] {
            let fixture = Fixture::new();
            let path = fixture.0.join("record.json");
            let old = b"{\"old\":true}\n";
            fs::write(&path, old).unwrap();
            let scope = Scope::new(path.clone(), point, actions.clone());
            let value = serde_json::json!({"grade":false,"unknown":{"future":17}});
            let error = if append {
                append_json(&path, &value)
            } else {
                write_json(&path, &value)
            }
            .unwrap_err();
            assert!(scope.hits() > 0);
            let bytes = fs::read(&path).unwrap();
            if committed {
                assert!(error.contains("was published, but durability is unconfirmed"));
                assert_ne!(bytes, old);
                if append {
                    assert!(bytes.starts_with(old));
                }
            } else {
                assert!(error.contains("not published"));
                assert_eq!(bytes, old);
            }
            assert!(!fs::read_dir(&fixture.0).unwrap().any(|entry| {
                entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".asb-evidence-stage-")
            }));
        }
    }
}

#[test]
fn interrupted_and_short_writes_retry_only_in_private_stage() {
    let fixture = Fixture::new();
    let path = fixture.0.join("run.jsonl");
    let scope = Scope::new(
        path.clone(),
        None,
        vec![
            WriteAction::Interrupted,
            WriteAction::Short(1),
            WriteAction::Interrupted,
        ],
    );
    append_json(&path, &serde_json::json!({"grade":true})).unwrap();
    assert_eq!(scope.hits(), 3);
    assert_eq!(fs::read(&path).unwrap(), b"{\"grade\":true}\n");
}

#[cfg(unix)]
#[test]
fn unsafe_nonregular_destinations_and_locks_are_rejected_without_following() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let sentinel = fixture.0.join("foreign");
    fs::write(&sentinel, b"foreign sentinel").unwrap();
    let link = fixture.0.join("symlink.json");
    symlink(&sentinel, &link).unwrap();
    let fifo = fixture.0.join("fifo.json");
    // Invoke the existing fixture utility, not a host path or collector.
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    let directory = fixture.0.join("directory.json");
    fs::create_dir(&directory).unwrap();
    for path in [&link, &fifo, &directory] {
        let started = Instant::now();
        assert!(
            write_json(path, &true)
                .unwrap_err()
                .contains("regular file")
        );
        assert!(started.elapsed() < Duration::from_secs(1));
    }
    assert_eq!(fs::read(&sentinel).unwrap(), b"foreign sentinel");
    let path = fixture.0.join("locked.json");
    let key = format!("{:x}", Sha256::digest(b"locked.json"));
    symlink(
        &sentinel,
        fixture.0.join(format!(".asb-evidence-{key}.lock")),
    )
    .unwrap();
    assert!(write_json(&path, &true).is_err());
    assert!(!path.exists());
    assert_eq!(fs::read(&sentinel).unwrap(), b"foreign sentinel");
}

#[test]
fn complete_historical_log_bytes_and_unknown_metadata_survive_append() {
    let fixture = Fixture::new();
    let path = fixture.0.join("run.jsonl");
    let old = b"{ \"future\" : [1,null], \"grade\":false }\n\n{\"native\":0.75}";
    fs::write(&path, old).unwrap();
    append_json(&path, &serde_json::json!({"next":true})).unwrap();
    let bytes = fs::read(&path).unwrap();
    assert!(bytes.starts_with(old));
    assert!(bytes.ends_with(b"\n{\"next\":true}\n"));
    fs::write(&path, b"{broken").unwrap();
    assert!(append_json(&path, &true).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"{broken");
}

#[cfg(unix)]
#[test]
fn publication_preserves_existing_permissions_and_persistent_lock_inode() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let fixture = Fixture::new();
    let path = fixture.0.join("run.jsonl");
    fs::write(&path, b"{\"prior\":false}\n").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    append_json(&path, &true).unwrap();
    let key = format!("{:x}", Sha256::digest(b"run.jsonl"));
    let lock = fixture.0.join(format!(".asb-evidence-{key}.lock"));
    let original = fs::metadata(&lock).unwrap();
    append_json(&path, &false).unwrap();
    assert_eq!(
        fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o640
    );
    assert_eq!(fs::metadata(lock).unwrap().ino(), original.ino());
}

#[test]
fn accepted_exact_record_limit_is_still_appendable() {
    let fixture = Fixture::new();
    let path = fixture.0.join("run.jsonl");
    // JSON string quotes account for two bytes of the serialized record.
    append_json(&path, &"x".repeat(MAX_RECORD - 3)).unwrap();
    append_json(&path, &false).unwrap();
    assert_eq!(fs::metadata(path).unwrap().len(), (MAX_RECORD + 6) as u64);
}

#[test]
fn unterminated_historical_record_cannot_grow_beyond_consumer_window() {
    let fixture = Fixture::new();
    let path = fixture.0.join("run.jsonl");
    let old = serde_json::to_vec(&"x".repeat(MAX_RECORD - 2)).unwrap();
    assert_eq!(old.len(), MAX_RECORD);
    fs::write(&path, &old).unwrap();
    assert!(
        append_json(&path, &false).is_err(),
        "adding newline would make historical record unreadable"
    );
    assert_eq!(fs::read(&path).unwrap(), old);
}

#[test]
fn encoded_bound_prevents_unreadable_fresh_and_updated_json() {
    let fixture = Fixture::new();
    let path = fixture.0.join("record.json");
    fs::write(&path, b"{\"old\":true}").unwrap();
    let huge = "x".repeat(MAX_RECORD);
    assert!(
        write_json(&path, &huge)
            .unwrap_err()
            .contains("exceeds 16 MiB")
    );
    assert_eq!(fs::read(&path).unwrap(), b"{\"old\":true}");
    assert!(
        update_json(&path, |_| Ok(serde_json::json!({"expanded":huge})))
            .unwrap_err()
            .contains("exceeds 16 MiB")
    );
    assert_eq!(fs::read(&path).unwrap(), b"{\"old\":true}");
    let fresh = fixture.0.join("fresh.json");
    assert!(write_json(&fresh, &"x".repeat(MAX_RECORD)).is_err());
    assert!(!fresh.exists());
    let metadata = fs::metadata(&path).unwrap();
    assert!(
        portable_snapshot_identity(&metadata, &fs::metadata(&path).unwrap()),
        "portable replacement must accept an unchanged existing snapshot"
    );
}

#[test]
fn retained_staging_cleanup_reports_primary_and_truthful_commit_state() {
    for committed in [false, true] {
        let fixture = Fixture::new();
        let path = fixture.0.join("record.json");
        fs::write(&path, b"{\"old\":true}").unwrap();
        let writes = if committed {
            vec![]
        } else {
            vec![WriteAction::Short(2), WriteAction::NoSpace]
        };
        let scope = Scope::new(path.clone(), Some(FaultPoint::Cleanup), writes);
        let error = write_json(&path, &serde_json::json!({"new":false})).unwrap_err();
        let stage = fs::read_dir(&fixture.0)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(".asb-evidence-stage-")
            })
            .expect("normal-path cleanup refusal must preserve its stage");
        assert!(error.contains(stage.to_str().unwrap()), "{error}");
        assert!(scope.hits() >= 1);
        if committed {
            assert!(error.contains("was published and synchronized"), "{error}");
            assert!(!error.contains("durability is unconfirmed"), "{error}");
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&fs::read(&path).unwrap()).unwrap(),
                serde_json::json!({"new":false})
            );
        } else {
            assert!(error.contains("not published"), "{error}");
            assert!(error.contains("No space left"), "{error}");
            assert_eq!(fs::read(&path).unwrap(), b"{\"old\":true}");
            assert!(stage.join("content").is_file());
        }
    }
}

#[test]
fn real_process_contention_holds_persistent_lock_through_commit() {
    const TEST: &str =
        "evidence::tests::real_process_contention_holds_persistent_lock_through_commit";
    if let Some(root) = std::env::var_os("ASB_EVIDENCE_LOCK_FIXTURE") {
        let root = PathBuf::from(root);
        let first = std::env::var_os("ASB_EVIDENCE_HOLD_COMMIT").is_some();
        fs::write(
            root.join(if first {
                "first-entered"
            } else {
                "second-entered"
            }),
            b"ready",
        )
        .unwrap();
        append_json(&root.join("run.jsonl"), &serde_json::json!({"first":first})).unwrap();
        return;
    }
    let fixture = Fixture::new();
    let executable = std::env::current_exe().unwrap();
    let mut first = std::process::Command::new(&executable)
        .args(["--exact", TEST])
        .env("ASB_EVIDENCE_LOCK_FIXTURE", &fixture.0)
        .env("ASB_EVIDENCE_HOLD_COMMIT", "1")
        .spawn()
        .unwrap();
    wait(&fixture.0.join("stage-owned"));
    assert!(!fixture.0.join("run.jsonl").exists());
    let mut second = std::process::Command::new(&executable)
        .args(["--exact", TEST])
        .env("ASB_EVIDENCE_LOCK_FIXTURE", &fixture.0)
        .spawn()
        .unwrap();
    wait(&fixture.0.join("second-entered"));
    assert!(second.try_wait().unwrap().is_none());
    fs::write(fixture.0.join("release-stage"), b"ready").unwrap();
    assert!(first.wait().unwrap().success());
    assert!(second.wait().unwrap().success());
    let text = fs::read_to_string(fixture.0.join("run.jsonl")).unwrap();
    let rows: Vec<serde_json::Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        rows,
        vec![
            serde_json::json!({"first":true}),
            serde_json::json!({"first":false})
        ]
    );
}
