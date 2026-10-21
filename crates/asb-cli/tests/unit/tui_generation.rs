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
        provider: None,
        model: None,
        availability: Availability::Unknown,
    };
    app.panes[0] = Pane::new(vec![choice("pi")]);
    app.panes[1] = Pane::new(vec![choice("openrouter")]);
    app.panes[2] = Pane::new(vec![Choice {
        label: "fixture-model".into(),
        provider: Some("other-provider".into()),
        model: Some("fixture-model".into()),
        availability: Availability::Unknown,
    }]);
    app.panes[3] = Pane::new(vec![choice("smoke")]);
    for pane in &mut app.panes[..4] {
        pane.selected[0] = true;
    }
    let error = generate_configs(&app).err().unwrap().to_string();
    assert!(error.contains("no compatible provider/model"), "{error}");
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    app.panes[3] = Pane::new(vec![choice("fixture [trial set]")]);
    app.panes[3].selected[0] = true;
    let path = root.join("workloads/fixture.workload.toml");
    let error = generate_configs(&app).err().unwrap().to_string();
    assert!(error.contains("fixture.workload.toml"), "{error}");
    std::fs::write(&path, "workload='fixture'\ntrials=[]\n").unwrap();
    let error = selected_trials(&app, "fixture [trial set]")
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("must contain a workload and at least one trial"),
        "{error}"
    );
    assert!(error.contains(&path.display().to_string()));
    app.panes[2].choices[0].provider = Some("openrouter".into());
    app.panes[2].choices = (0..257).map(|_| app.panes[2].choices[0].clone()).collect();
    app.panes[2].selected = vec![true; 257];
    app.panes[0] = Pane::new((0..256).map(|_| choice("pi")).collect());
    app.panes[0].selected.fill(true);
    app.panes[3] = Pane::new(vec![choice("smoke")]);
    app.panes[3].selected[0] = true;
    let error = generate_configs(&app).err().unwrap().to_string();
    assert!(error.contains("65536-configuration sweep limit"), "{error}");
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    std::fs::remove_dir_all(root).unwrap();
}
