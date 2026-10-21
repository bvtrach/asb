use super::*;
use crate::sweep::{Options, SweepFile, campaigns, plan};
use std::{fs, path::PathBuf};

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn published(mode: crate::tape::Mode) -> (Fixture, SweepPlan, PathBuf) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/campaign-seal-fixtures")
        .join(crate::workloads::TaskIdentity::local("inputs", None).attempt_id);
    fs::create_dir_all(root.join(".asb")).unwrap();
    let root = root.canonicalize().unwrap();
    fs::write(
        root.join(".asb/project.toml"),
        toml::to_string(&crate::project::ProjectConfig::default()).unwrap(),
    )
    .unwrap();
    let mut refs = Vec::new();
    let tapes = root.join("tapes");
    for index in 0..2 {
        let reference = format!("benchmark-{index}.toml");
        fs::write(
            root.join(&reference),
            "agent='pi'\nprovider='local'\nmodel='fixture'\nworkload='bug-fix'\n",
        )
        .unwrap();
        let directory = tapes.join(format!("{index:03}-benchmark-{index}/cassettes"));
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("benchmark-pi-bug-fix-fixture.jsonl"),
            b"{}\n",
        )
        .unwrap();
        refs.push(reference);
    }
    let mut plan = plan::build(
        &root,
        &SweepFile {
            configs: refs,
            resources: None,
        },
    )
    .unwrap();
    plan.execution_mode = match mode {
        crate::tape::Mode::Record => plan::SavedExecutionMode::Record,
        crate::tape::Mode::Replay => plan::SavedExecutionMode::Replay,
    };
    let results = root.join("sweep-results");
    let prepared = campaigns::prepare(
        &root,
        &results,
        &root.join("history"),
        &mut plan,
        Options {
            mode: Some(mode),
            repeat: Some(2),
            cassette_root: Some(&tapes),
            ..Default::default()
        },
    )
    .unwrap()
    .unwrap();
    plan::fresh_results(&root, "sweep", &plan).unwrap();
    let pool = prepared.publish(&results).unwrap();
    drop(pool); // No provider, agent, environment or attempt has been started.
    (Fixture(root), plan, results)
}

#[test]
fn campaign_seal_roots_bind_immutable_inputs_not_outputs_or_original_replay_paths() {
    for mode in [crate::tape::Mode::Record, crate::tape::Mode::Replay] {
        let (fixture, plan, results) = published(mode);
        let roots = input_roots(&results, &plan).unwrap();
        let layouts = plan.read_campaigns(&results).unwrap();
        assert_eq!(
            roots.len(),
            if mode == crate::tape::Mode::Replay {
                6
            } else {
                4
            }
        );
        let snapshotter =
            super::super::Snapshotter::open(&results.join(".checkpoint"), "python3".into())
                .unwrap();
        let fingerprint = || {
            snapshotter
                .fingerprint(
                    &roots,
                    &serde_json::json!({"scope":"input-root-test-only"}),
                    false,
                    &[],
                )
                .unwrap()
        };
        let before = fingerprint();
        for layout in &layouts {
            let batch = layout.root.join("batch-1");
            fs::create_dir_all(batch.join("workspace.asb-agent-runtime")).unwrap();
            fs::write(batch.join("run.jsonl"), "new output").unwrap();
            fs::write(
                batch.join("workspace.asb-agent-runtime/session"),
                "new session",
            )
            .unwrap();
            if let Some(recording) = &layout.plan.recording {
                fs::write(recording, "growing recording output").unwrap();
            } else {
                fs::write(
                    layout.plan.replay_source.as_ref().unwrap(),
                    "original source replaced after private staging",
                )
                .unwrap();
            }
        }
        assert_eq!(fingerprint(), before);
        if mode == crate::tape::Mode::Replay {
            let input = layouts[1]
                .root
                .join(&layouts[1].plan.replay_inputs[0].private);
            fs::write(&input, "changed private bytes").unwrap();
            assert_ne!(fingerprint(), before);
            fs::write(input, b"{}\n").unwrap();
            assert_eq!(fingerprint(), before);
        }
        fs::write(fixture.0.join("benchmark-1.toml"), "changed configuration").unwrap();
        assert_ne!(fingerprint(), before);
    }
}

#[test]
fn later_invalid_campaign_reference_cannot_become_an_input_declaration() {
    let (_fixture, plan, results) = published(crate::tape::Mode::Replay);
    let layouts = plan.read_campaigns(&results).unwrap();
    let later = layouts[1].root.join("plan.json");
    let original = fs::read(&later).unwrap();
    fs::write(&later, b"malformed later plan").unwrap();
    assert!(input_roots(&results, &plan).is_err());
    assert!(!results.join(".checkpoint").exists());
    assert!(!layouts[0].root.join("batch-1").exists());
    fs::write(later, original).unwrap();
    assert!(input_roots(&results, &plan).is_ok());
}
