//! Headless workload-selector and keyboard contracts, without terminals/network.
use super::*;
use ratatui::backend::TestBackend;

struct Project(PathBuf);
impl Project {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "asb-tui-coverage-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(path.join("workloads")).unwrap();
        Self(path)
    }
    fn app(&self) -> WorkloadApp {
        WorkloadApp {
            root: self.0.clone(),
            project: project::ProjectConfig::default(),
            workloads: Pane::new(vec![choice("custom")]),
            trials: Pane::new(vec![choice("first"), choice("second")]),
            level: 0,
            selected_workload: None,
            selections: BTreeSet::new(),
            message: "Select a workload".into(),
            message_color: Color::Yellow,
            message_until: None,
        }
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn choice(label: &str) -> Choice {
    Choice {
        label: label.into(),
        provider: None,
        model: None,
        availability: Availability::Unknown,
    }
}

#[test]
fn workload_windows_render_both_levels_selected_rows_status_and_resizes() {
    let project = Project::new();
    let mut app = project.app();
    for level in 0..=1 {
        app.level = level;
        app.workloads.selected[0] = true;
        app.workloads.choices[0].availability = Availability::Missing;
        app.trials.selected[1] = true;
        app.trials.choices[1].availability = Availability::Available;
        for (width, height) in [(1, 1), (20, 5), (80, 24), (120, 40)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|frame| draw_workload(frame, &app)).unwrap();
            if width >= 80 {
                let rendered = terminal
                    .backend()
                    .buffer()
                    .content
                    .iter()
                    .map(|cell| cell.symbol())
                    .collect::<String>();
                assert!(rendered.contains("ASB workload trials"));
                assert!(rendered.contains("[x]"));
                assert!(rendered.contains(if level == 0 { "custom" } else { "second" }));
                assert!(rendered.contains("Space select/install"));
            }
        }
    }
}

#[test]
fn keyboard_navigation_handles_boundaries_empty_filters_and_unhandled_actions() {
    let mut pane = Pane::new((0..20).map(|i| choice(&format!("item-{i:02}"))).collect());
    for (key, expected) in [
        (KeyCode::End, 19),
        (KeyCode::PageUp, 11),
        (KeyCode::PageDown, 19),
        (KeyCode::Home, 0),
        (KeyCode::Up, 0),
        (KeyCode::Down, 1),
    ] {
        assert!(handle_pane_key(
            &mut pane,
            KeyEvent::new(key, KeyModifiers::NONE)
        ));
        assert_eq!(pane.cursor, expected);
    }
    assert!(!handle_pane_key(
        &mut pane,
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
    ));
    assert!(!handle_pane_key(
        &mut pane,
        KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT)
    ));
    assert!(handle_pane_key(
        &mut pane,
        KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)
    ));
    assert!(pane.selected[1]);
    pane.search = "no match".into();
    pane.move_cursor(1);
    assert_eq!(pane.cursor, 0);
    assert_eq!(pane.active_index(), None);
    pane.toggle_single();
    assert!(
        pane.selected[1],
        "empty search must preserve hidden selections"
    );
    let mut empty = Pane::new(vec![]);
    assert!(handle_pane_key(
        &mut empty,
        KeyEvent::new(KeyCode::End, KeyModifiers::NONE)
    ));
    assert_eq!(empty.active_index(), None);
}

#[test]
fn workload_catalog_finds_installed_custom_sources_and_refreshes_readiness() {
    let project = Project::new();
    let directory = project.0.join("workloads");
    fs::create_dir_all(directory.join("custom/source")).unwrap();
    fs::write(directory.join("custom.txt"), "fixture task").unwrap();
    fs::write(directory.join("ignored.bin"), "not a workload").unwrap();
    let choices = external_workload_choices(&directory);
    assert!(choices.iter().any(|item| item.label == "custom"));
    assert!(!choices.iter().any(|item| item.label == "ignored.bin"));
    let mut app = project.app();
    refresh_workload_availability(&mut app);
    assert_eq!(
        app.workloads.choices[0].availability,
        Availability::Available
    );
    fs::remove_file(directory.join("custom.txt")).unwrap();
    refresh_workload_availability(&mut app);
    assert_eq!(app.workloads.choices[0].availability, Availability::Missing);
}

#[test]
fn workload_selection_propagates_install_error_and_does_not_select_hidden_items() {
    let project = Project::new();
    let mut app = project.app();
    app.workloads.choices[0].availability = Availability::Missing;
    let result = toggle_workload_choice(&mut app, |_| Err(Error::Run("fixture failure".into())));
    assert!(result.unwrap_err().to_string().contains("fixture failure"));
    assert!(!app.workloads.selected[0]);
    app.workloads.search = "no match".into();
    toggle_workload_choice(&mut app, |_| panic!("hidden selection must not install")).unwrap();
    assert!(!app.workloads.selected[0]);
}

#[test]
fn empty_trial_selection_writes_nothing_and_write_failure_is_reported() {
    let project = Project::new();
    let mut app = project.app();
    app.selections.insert("not-a-tab-separated-trial".into());
    assert_eq!(write_trial_sets(&app).unwrap(), 0);
    assert_eq!(
        fs::read_dir(project.0.join("workloads")).unwrap().count(),
        0
    );
    app.selections.insert("custom\tfirst".into());
    fs::create_dir(project.0.join("workloads/custom.workload.toml")).unwrap();
    assert!(write_trial_sets(&app).is_err());
}
