use super::*;
use crate::runner::campaign::{MAX_EXECUTIONS, planned_slots};
use crate::runner::repeated::{Prepared as Campaign, PreparedReplay};

thread_local! {
    static GROW_AFTER_METADATA: std::cell::RefCell<Option<(PathBuf, u64)>> = const { std::cell::RefCell::new(None) };
}
pub(super) fn after_metadata_pass() {
    if let Some((path, length)) = GROW_AFTER_METADATA.with(|value| value.borrow_mut().take()) {
        OpenOptions::new()
            .write(true)
            .open(path)
            .unwrap()
            .set_len(length)
            .unwrap();
    }
}

struct Fixture {
    root: PathBuf,
    config: BenchmarkConfig,
    config_path: PathBuf,
    source: PathBuf,
}
impl Fixture {
    fn new(concurrency: usize) -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/pinned-legacy-fixtures")
            .join(crate::workloads::TaskIdentity::local("legacy", None).attempt_id);
        fs::create_dir_all(root.join(".asb")).unwrap();
        fs::create_dir(root.join("old-results")).unwrap();
        let root = root.canonicalize().unwrap();
        fs::write(
            root.join(".asb/project.toml"),
            toml::to_string(&crate::project::ProjectConfig::default()).unwrap(),
        )
        .unwrap();
        let config: BenchmarkConfig = toml::from_str(&format!("agent='pi'\nprovider='openrouter'\nmodel='fixture'\nworkload='bug-fix'\nconcurrency={concurrency}")).unwrap();
        let config_path = root.join("benchmark.toml");
        fs::write(&config_path, toml::to_string(&config).unwrap()).unwrap();
        let source = root.join("old-results/input.jsonl");
        Self {
            root,
            config,
            config_path,
            source,
        }
    }
    fn prepare(&self) -> Campaign {
        Campaign::prepare(
            &self.config_path,
            self.config.clone(),
            2,
            RunOptions {
                mode: Some(crate::tape::Mode::Replay),
                cassette: Some(self.source.to_str().unwrap()),
                ..Default::default()
            },
            None,
        )
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

#[test]
fn pinned_inputs_survive_result_rotation_replacement_and_unlink() {
    for change in ["rename", "unlink"] {
        let fixture = Fixture::new(2);
        fs::write(&fixture.source, b"original trace\n").unwrap();
        let campaign = fixture.prepare();
        let Some(PreparedReplay::Legacy(pinned)) = campaign.replay else {
            panic!("legacy owner")
        };
        assert_eq!(pinned.files.len(), 1);
        assert_eq!(campaign.plan.replay_inputs.len(), 1);
        assert!(campaign.plan.slots.len() < MAX_EXECUTIONS as usize);
        let old = fixture.root.join("old-results");
        if change == "rename" {
            fs::rename(&old, fixture.root.join("history")).unwrap();
        } else {
            fs::remove_file(&fixture.source).unwrap();
            fs::remove_dir(&old).unwrap();
        }
        fs::create_dir(&old).unwrap();
        fs::write(
            &fixture.source,
            b"replacement pathname must not be reopened\n",
        )
        .unwrap();
        fs::create_dir_all(&campaign.root).unwrap();
        let base = pinned
            .stage(&campaign.root, &campaign.plan)
            .unwrap()
            .unwrap();
        for replica in 1..=2 {
            let staged = crate::benchmark_paths::replica_path(Path::new(&base), replica);
            assert_eq!(fs::read(staged).unwrap(), b"original trace\n");
        }
        assert_eq!(
            fs::read(&fixture.source).unwrap(),
            b"replacement pathname must not be reopened\n"
        );
    }
}

#[test]
fn in_place_mutation_never_publishes_a_private_input_or_starts_a_client() {
    for changed in [
        b"changed!\n".as_slice(),
        b"short\n",
        b"longer than original\n",
    ] {
        let fixture = Fixture::new(1);
        fs::write(&fixture.source, b"original\n").unwrap();
        let campaign = fixture.prepare();
        let root = campaign.root.clone();
        let private = campaign.plan.replay_inputs[0].private.clone();
        fs::write(&fixture.source, changed).unwrap();
        let result = crate::runner::repeated::activate(
            campaign,
            &mut Vec::new(),
            &std::sync::Mutex::new(std::collections::BTreeSet::new()),
        );
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("changed after execution planning")
        );
        assert!(!root.join(private).exists());
        assert!(!root.join("batch-1").exists());
        assert!(!root.join("finished.json").exists());
    }
}

#[test]
fn legacy_discovery_keeps_base_precedence_and_sparse_64_fallback() {
    let fixture = Fixture::new(64);
    let suffix = crate::benchmark_paths::replica_path(&fixture.source, 64);
    fs::write(&suffix, b"sparse64\n").unwrap();
    fs::write(&fixture.source, b"base wins\n").unwrap();
    let campaign = fixture.prepare();
    let Some(PreparedReplay::Legacy(pinned)) = campaign.replay else {
        panic!("legacy owner")
    };
    assert_eq!(pinned.files.len(), 1);
    assert_eq!(campaign.plan.replay_inputs[0].source, fixture.source);
    drop(pinned);
    fs::remove_file(&fixture.source).unwrap();
    let campaign = fixture.prepare();
    let Some(PreparedReplay::Legacy(pinned)) = campaign.replay else {
        panic!("legacy owner")
    };
    assert_eq!(pinned.files.len(), 1);
    assert_eq!(campaign.plan.replay_inputs[0].source, suffix);
    assert!(
        campaign
            .plan
            .slots
            .iter()
            .all(|slot| slot.replay_input == Some(0))
    );
}

#[test]
fn aggregate_metadata_budget_is_checked_before_hashing_any_input() {
    let fixture = Fixture::new(33);
    for replica in 1..=33 {
        let path = crate::benchmark_paths::replica_path(&fixture.source, replica);
        File::create(path)
            .unwrap()
            .set_len(MAX_INPUT_BYTES)
            .unwrap();
    }
    let mut slots = planned_slots(&fixture.config, 1, false).unwrap();
    TRANSFERS.with(|count| count.set(0));
    let result = Prepared::open(
        &fixture.config_path,
        &fixture.config,
        RunOptions {
            mode: Some(crate::tape::Mode::Replay),
            cassette: Some(fixture.source.to_str().unwrap()),
            ..Default::default()
        },
        &mut slots,
    );
    assert!(result.err().unwrap().to_string().contains("aggregate"));
    TRANSFERS.with(|count| assert_eq!(count.get(), 0));
    assert!(!fixture.root.join("results").exists());
}

#[test]
fn descriptor_transfer_rewinds_and_refuses_growth_during_copy() {
    let fixture = Fixture::new(1);
    fs::write(&fixture.source, b"original\n").unwrap();
    let mut file = open_regular(&fixture.source).unwrap();
    for _ in 0..2 {
        let mut output = Vec::new();
        transfer(&mut file, 9, &mut output).unwrap();
        assert_eq!(output, b"original\n");
    }
    struct Grow<'a>(&'a Path);
    impl Write for Grow<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            OpenOptions::new()
                .append(true)
                .open(self.0)?
                .write_all(b"extra")?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    assert!(
        transfer(&mut file, 9, &mut Grow(&fixture.source))
            .unwrap_err()
            .to_string()
            .contains("changed while reading")
    );
    assert_eq!(file.stream_position().unwrap(), 10); // original plus one sentinel
    assert!(open_regular(&fixture.root).is_err());
    let large = fixture.root.join("large");
    File::create(&large)
        .unwrap()
        .set_len(MAX_INPUT_BYTES + 1)
        .unwrap();
    assert!(open_regular(&large).is_err());
}

#[test]
fn length_growth_before_observation_or_staging_reads_and_hashes_no_payload() {
    let fixture = Fixture::new(1);
    fs::write(&fixture.source, b"{}\n").unwrap();
    let mut slots = planned_slots(&fixture.config, 1, false).unwrap();
    GROW_AFTER_METADATA
        .with(|value| *value.borrow_mut() = Some((fixture.source.clone(), MAX_INPUT_BYTES)));
    TRANSFERS.with(|count| count.set(0));
    let result = Prepared::open(
        &fixture.config_path,
        &fixture.config,
        RunOptions {
            mode: Some(crate::tape::Mode::Replay),
            cassette: Some(fixture.source.to_str().unwrap()),
            ..Default::default()
        },
        &mut slots,
    );
    assert!(
        result
            .err()
            .unwrap()
            .to_string()
            .contains("length changed before reading")
    );
    TRANSFERS.with(|count| assert_eq!(count.get(), 0));
    GROW_AFTER_METADATA.with(|value| assert!(value.borrow().is_none()));

    fs::write(&fixture.source, b"{}\n").unwrap();
    let campaign = fixture.prepare();
    let Some(PreparedReplay::Legacy(pinned)) = campaign.replay else {
        panic!("legacy owner")
    };
    OpenOptions::new()
        .write(true)
        .open(&fixture.source)
        .unwrap()
        .set_len(MAX_INPUT_BYTES)
        .unwrap();
    TRANSFERS.with(|count| count.set(0));
    fs::create_dir_all(&campaign.root).unwrap();
    let result = pinned.stage(&campaign.root, &campaign.plan);
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("length changed before reading")
    );
    TRANSFERS.with(|count| assert_eq!(count.get(), 0));
    let private = campaign.root.join(&campaign.plan.replay_inputs[0].private);
    assert!(!private.exists());
    assert_eq!(
        fs::metadata(private.with_extension("partial"))
            .unwrap()
            .len(),
        0
    );
    assert!(!campaign.root.join("batch-1").exists());
}
