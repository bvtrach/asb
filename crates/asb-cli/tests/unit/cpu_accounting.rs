//! Synthetic cgroup-file contracts, not qualification of host delegation.
use super::*;

#[test]
fn final_counter_validation_never_claims_populated_or_unreadable_totals() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/cpu-accounting-fixtures")
        .join(crate::workloads::TaskIdentity::local("fixture", None).attempt_id);
    fs::create_dir_all(&root).unwrap();
    for (case, events, counters, expected_status, reason) in [
        (
            "stopped",
            Some("populated 0\n"),
            Some("user_usec 1500\nsystem_usec 2500\n"),
            "completed",
            "",
        ),
        (
            "populated",
            Some("populated 1\n"),
            Some("user_usec 1500\nsystem_usec 2500\n"),
            "unavailable",
            "still has live tasks",
        ),
        (
            "missing-events",
            None,
            Some("user_usec 1500\nsystem_usec 2500\n"),
            "unavailable",
            "still has live tasks",
        ),
        (
            "bad-counters",
            Some("populated 0\n"),
            Some("user_usec invalid\nsystem_usec 1\n"),
            "unavailable",
            "lacks valid",
        ),
        (
            "missing-counters",
            Some("populated 0\n"),
            None,
            "unavailable",
            "cannot read",
        ),
    ] {
        let directory = root.join(case);
        let leaf = directory.join("leaf");
        fs::create_dir_all(&leaf).unwrap();
        if let Some(events) = events {
            fs::write(leaf.join("cgroup.events"), events).unwrap();
        }
        if let Some(counters) = counters {
            fs::write(leaf.join("cpu.stat"), counters).unwrap();
        }
        let mut session =
            Session::start_with_collectors(&directory.join("measurements"), &[], None).unwrap();
        session.state::<State>().borrow_mut().leaf = Some(Leaf { path: leaf.clone() });
        ADAPTER.finish(&mut session).unwrap();
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(session.dir.join("cpu-accounting.json")).unwrap())
                .unwrap();
        assert_eq!(value["status"], expected_status, "{case}");
        assert_eq!(value["cleanup"], "retained_busy_or_inaccessible");
        assert!(
            leaf.exists(),
            "nonempty synthetic evidence must be preserved"
        );
        if expected_status == "completed" {
            assert_eq!(value["source"], "cgroup-v2");
            assert_eq!(value["metrics"]["cpu_user_ms"], 1.5);
            assert_eq!(value["metrics"]["cpu_system_ms"], 2.5);
        } else {
            assert!(value["metrics"].is_null());
            assert!(value["reason"].as_str().unwrap().contains(reason));
        }
        drop(session);
    }
    let oversized = root.join("oversized");
    fs::write(&oversized, vec![b' '; 4097]).unwrap();
    assert!(
        read_cpu_stat(&oversized)
            .unwrap_err()
            .contains("exceeds 4096")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn lifecycle_counter_reset_records_reason_without_stale_totals() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/cpu-accounting-fixtures")
        .join(crate::workloads::TaskIdentity::local("fixture", None).attempt_id);
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("cpu.stat"), "user_usec 100\nsystem_usec 10\n").unwrap();
    let mut session =
        Session::start_with_collectors(&root.join("measurements"), &[], None).unwrap();
    session.state::<State>().borrow_mut().external = Some(ExternalCounter {
        path: root.clone(),
        initial: (200, 20),
        final_delta: Some((1, 1)),
    });
    ADAPTER
        .lifecycle_event(&mut session, LifecycleEvent::AgentExecuteEnd)
        .unwrap();
    let state = session.state::<State>();
    let state = state.borrow();
    assert!(state.external.as_ref().unwrap().final_delta.is_none());
    assert!(
        state
            .external_reason
            .as_ref()
            .unwrap()
            .contains("counter decreased")
    );
    drop(state);
    drop(session);
    fs::remove_dir_all(root).unwrap();
}
