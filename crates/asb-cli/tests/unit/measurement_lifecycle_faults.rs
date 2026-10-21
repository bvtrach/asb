//! A failing lifecycle adapter cannot prevent later hooks or scoped shutdown.
use super::*;
use crate::collectors::{CollectorAdapter, LifecycleEvent};
use std::process::Command;

struct Failing;
impl CollectorAdapter for Failing {
    fn id(&self) -> &'static str {
        "fixture-failing"
    }
    fn lifecycle_event(&self, _: &mut Session, event: LifecycleEvent) -> Result<(), String> {
        if event == LifecycleEvent::AgentExecuteEnd {
            Err("fixture marker write failed".into())
        } else {
            Ok(())
        }
    }
}
struct Scoped;
impl CollectorAdapter for Scoped {
    fn id(&self) -> &'static str {
        "fixture-scoped"
    }
    fn requires_pid(&self) -> bool {
        true
    }
    fn lifecycle_event(&self, session: &mut Session, event: LifecycleEvent) -> Result<(), String> {
        if event == LifecycleEvent::AgentExecuteEnd {
            session.set_artifact(
                collectors::SharedArtifact::StackSamples,
                session.dir.join("hook-delivered"),
            );
        }
        Ok(())
    }
}
static FAILING: Failing = Failing;
static SCOPED: Scoped = Scoped;

#[test]
fn failing_adapter_and_full_journal_still_stop_process_collectors_before_evaluation() {
    for full in [false, true] {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/lifecycle-fault-fixtures")
            .join(crate::workloads::TaskIdentity::local("collector", None).attempt_id);
        fs::create_dir_all(&root).unwrap();
        let mut session =
            Session::start_with_adapters(&root, &[], None, None, vec![&FAILING, &SCOPED], false)
                .unwrap();
        if full {
            for _ in 0..256 {
                session.event(LifecycleEvent::RunStarted).unwrap();
            }
        }
        let mut sentinel = Command::new("/bin/sleep").arg("20").spawn().unwrap();
        let child = Command::new("/bin/sleep").arg("20").spawn().unwrap();
        session.collectors.push(Collector {
            name: "fixture-scoped".into(),
            adapter: &SCOPED,
            child,
            started: Instant::now(),
            stdout_path: root.join("stdout"),
            stderr_path: root.join("stderr"),
        });
        let error = session.event(LifecycleEvent::AgentExecuteEnd).unwrap_err();
        assert!(error.contains("fixture marker write failed"));
        assert_eq!(error.contains("too many benchmark lifecycle events"), full);
        assert_eq!(
            session.artifact(collectors::SharedArtifact::StackSamples),
            Some(root.join("hook-delivered").as_path())
        );
        assert!(session.collectors.is_empty());
        assert_eq!(session.stopped_collectors.len(), 1);
        assert!(
            session.stopped_collectors[0]
                .collector
                .child
                .try_wait()
                .unwrap()
                .is_some()
        );
        assert!(
            sentinel.try_wait().unwrap().is_none(),
            "unrelated child must survive"
        );
        assert_eq!(session.lifecycle.len(), if full { 256 } else { 1 });
        sentinel.kill().unwrap();
        sentinel.wait().unwrap();
        drop(session);
        fs::remove_dir_all(root).unwrap();
    }
}
