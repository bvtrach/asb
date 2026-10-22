//! Actual local producer regressions. No collector, provider or host settings.
//! Initially red: qualify these before implementing collector custody changes.
use super::*;

struct Fixture {
    directory: PathBuf,
    target: Child,
}
impl Fixture {
    fn new() -> Self {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/collector-custody-fixtures")
            .join(crate::workloads::TaskIdentity::local("collector-custody", None).attempt_id);
        fs::create_dir_all(&directory).unwrap();
        let target = Command::new("/bin/sleep").arg("30").spawn().unwrap();
        Self { directory, target }
    }
    fn identity(&self) -> ProcessIdentity {
        ProcessIdentity::read(self.target.id()).unwrap()
    }
    fn wait_for(&self, path: &Path) -> String {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Ok(value) = fs::read_to_string(path) {
                if !value.is_empty() {
                    return value;
                }
            }
            assert!(
                Instant::now() < deadline,
                "fixture producer never became ready"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    fn snapshot(&self, script: String, stdout: File) -> ProcessTreeSnapshot {
        ProcessTreeSnapshot::start(
            self.identity(),
            PathBuf::from("python3"),
            Box::new(move |_| SnapshotCommand {
                runtime: Duration::from_secs(3),
                stream_bytes: 1024 * 1024,
                arguments: vec![
                    "-I".into(),
                    "-S".into(),
                    "-B".into(),
                    "-u".into(),
                    "-c".into(),
                    script.clone(),
                ],
            }),
            stdout,
            File::create(self.directory.join("stderr")).unwrap(),
        )
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.target.kill();
        let _ = self.target.wait();
        let _ = fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn stopping_snapshot_cancels_an_active_owned_command_instead_of_joining_its_duration() {
    let fixture = Fixture::new();
    let ready = fixture.directory.join("ready");
    let script = format!(
        "import pathlib,time\npathlib.Path({ready:?}).write_text('ready')\ntime.sleep(1.5)\n"
    );
    let snapshot = fixture.snapshot(
        script,
        File::create(fixture.directory.join("stdout")).unwrap(),
    );
    fixture.wait_for(&ready);
    let started = Instant::now();
    let _ = snapshot.stop();
    let elapsed = started.elapsed();
    assert!(fixture.target.id() > 1);
    assert!(
        elapsed < Duration::from_millis(600),
        "snapshot stop waited for its command: {elapsed:?}"
    );
}

#[test]
fn snapshot_stop_is_not_held_by_a_duplicate_writer_after_its_command_exits() {
    let fixture = Fixture::new();
    let ready = fixture.directory.join("ready");
    let release = fixture.directory.join("release");
    let script = format!(
        "import os,pathlib,time\npathlib.Path({ready:?}).write_text(str(os.getpid()))\nwhile not pathlib.Path({release:?}).exists(): time.sleep(.005)\nos._exit(0)\n"
    );
    let snapshot = fixture.snapshot(
        script,
        File::create(fixture.directory.join("stdout")).unwrap(),
    );
    let producer = fixture.wait_for(&ready).parse::<u32>().unwrap();
    // Retain only this fixture's stdout writer. No orphan descendant or global
    // subreaper is needed to reproduce inherited-pipe EOF pressure.
    let writer = fs::OpenOptions::new()
        .write(true)
        .open(format!("/proc/{producer}/fd/1"))
        .unwrap();
    let holder = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1500));
        drop(writer);
    });
    fs::write(&release, "release").unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match fs::read_to_string(format!("/proc/{producer}/stat")) {
            Ok(stat) if stat.contains(") Z ") => break,
            Err(_) => break,
            _ => {}
        }
        assert!(Instant::now() < deadline, "fixture command did not exit");
        std::thread::sleep(Duration::from_millis(5));
    }
    let started = Instant::now();
    let _ = snapshot.stop();
    let elapsed = started.elapsed();
    holder.join().unwrap();
    assert!(
        elapsed < Duration::from_millis(600),
        "retained pipe writer extended stop: {elapsed:?}"
    );
}

#[test]
fn actual_enospc_is_reported_as_producer_failure_instead_of_silent_completion() {
    let fixture = Fixture::new();
    let ready = fixture.directory.join("ready");
    let script = format!(
        "import pathlib,sys\nsys.stdout.write('CPU %usr %system %CPU\\nall 7 3 10\\n')\nsys.stdout.flush()\npathlib.Path({ready:?}).write_text('ready')\n"
    );
    let snapshot = fixture.snapshot(
        script,
        fs::OpenOptions::new()
            .write(true)
            .open("/dev/full")
            .unwrap(),
    );
    fixture.wait_for(&ready);
    // Observe the completed write attempt, rather than racing an intentional
    // cancellation that correctly discards an unfinished observation.
    let deadline = Instant::now() + Duration::from_secs(3);
    while !snapshot.is_finished() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        snapshot.is_finished(),
        "producer did not settle its write attempt"
    );
    let completion = format!("{:?}", snapshot.stop());
    let diagnostic = fs::read_to_string(fixture.directory.join("stderr")).unwrap();
    assert!(
        completion.contains("write") || diagnostic.contains("write"),
        "ENOSPC was not reported; completion={completion}, diagnostic={diagnostic:?}"
    );
}

#[test]
fn worker_creation_failure_admits_no_command_or_observation() {
    let fixture = Fixture::new();
    let sampler = ProcessTreeSnapshot::start_with_spawn(
        fixture.identity(),
        "/must-not-execute".into(),
        Box::new(|_| panic!("failed worker must not inspect or execute a command")),
        File::create(fixture.directory.join("stdout")).unwrap(),
        File::create(fixture.directory.join("stderr")).unwrap(),
        |_| Err(std::io::Error::other("fixture worker creation failed")),
    );
    assert!(sampler.is_none());
    assert!(
        fixture.identity().matches(),
        "unrelated observed target must survive"
    );
    assert_eq!(fs::read(fixture.directory.join("stdout")).unwrap(), b"");
}

#[test]
fn worker_panic_is_reported_without_joining_a_live_worker_or_killing_observed_target() {
    let fixture = Fixture::new();
    let sampler = ProcessTreeSnapshot::start(
        fixture.identity(),
        "/must-not-execute".into(),
        Box::new(|_| panic!("fixture worker panic before producer launch")),
        File::create(fixture.directory.join("stdout")).unwrap(),
        File::create(fixture.directory.join("stderr")).unwrap(),
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !sampler.is_finished() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(sampler.is_finished());
    assert!(sampler.stop().unwrap_err().contains("worker failed"));
    assert!(
        fixture.identity().matches(),
        "unrelated observed target must survive"
    );
}
