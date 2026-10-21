//! Failure cases must not publish an incomplete matrix.
use super::*;

#[test]
fn invalid_or_oversized_setup_selections_never_publish_configs() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/tui-generation-errors")
        .join(crate::workloads::TaskIdentity::local("fixture", None).attempt_id);
    std::fs::create_dir_all(root.join("workloads")).unwrap();
    let mut app = App::new(root.clone(), project::ProjectConfig::default());
    let choice = |label: &str| Choice {
        label: label.into(),
        key: ChoiceKey::Prompt(label.into()),
        availability: Availability::Unknown,
    };
    app.panes[PaneKind::Agents.index()] = Pane::new(vec![Choice {
        key: ChoiceKey::Agent("pi".into()),
        ..choice("pi")
    }]);
    app.panes[PaneKind::Providers.index()] = Pane::new(vec![Choice {
        key: ChoiceKey::Provider("openrouter".into()),
        ..choice("openrouter")
    }]);
    app.panes[PaneKind::Models.index()] = Pane::new(vec![Choice {
        label: "fixture-model".into(),
        key: ChoiceKey::Model {
            provider: "other-provider".into(),
            model: "fixture-model".into(),
        },
        availability: Availability::Unknown,
    }]);
    app.panes[PaneKind::Workloads.index()] = Pane::new(vec![choice("smoke")]);
    for pane in &mut app.panes[..4] {
        pane.selected[0] = true;
    }
    let error = generate_configs(&app).err().unwrap().to_string();
    assert!(error.contains("no compatible provider/model"), "{error}");
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    app.panes[PaneKind::Workloads.index()] = Pane::new(vec![Choice {
        key: ChoiceKey::TrialSet {
            workload: "fixture".into(),
            file: "fixture.workload.toml".into(),
        },
        ..choice("fixture [trial set]")
    }]);
    app.panes[PaneKind::Workloads.index()].selected[0] = true;
    let path = root.join("workloads/fixture.workload.toml");
    let error = generate_configs(&app).err().unwrap().to_string();
    assert!(error.contains("fixture.workload.toml"), "{error}");
    std::fs::write(&path, "workload='fixture'\ntrials=[]\n").unwrap();
    let error = selected_trials(
        &app,
        &ChoiceKey::TrialSet {
            workload: "fixture".into(),
            file: "fixture.workload.toml".into(),
        },
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("must contain a workload and at least one trial"),
        "{error}"
    );
    assert!(error.contains(&path.display().to_string()));
    app.panes[PaneKind::Models.index()].choices[0].key = ChoiceKey::Model {
        provider: "openrouter".into(),
        model: "fixture-model".into(),
    };
    app.panes[PaneKind::Models.index()].choices = (0..257)
        .map(|_| app.panes[PaneKind::Models.index()].choices[0].clone())
        .collect();
    app.panes[PaneKind::Models.index()].selected = vec![true; 257];
    app.panes[PaneKind::Agents.index()] = Pane::new(
        (0..256)
            .map(|index| Choice {
                key: ChoiceKey::Agent(format!("fixture-agent-{index}")),
                ..choice("pi")
            })
            .collect(),
    );
    app.panes[PaneKind::Agents.index()].selected.fill(true);
    app.panes[PaneKind::Workloads.index()] = Pane::new(vec![choice("smoke")]);
    app.panes[PaneKind::Workloads.index()].selected[0] = true;
    let error = generate_configs(&app).err().unwrap().to_string();
    assert!(error.contains("65536-configuration sweep limit"), "{error}");
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    std::fs::remove_dir_all(root).unwrap();
}
