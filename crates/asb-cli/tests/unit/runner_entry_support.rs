//! Private per-thread observation of a real runner attempt; no factory bypass.
use super::*;
use std::cell::RefCell;
use std::rc::Rc;

struct Hook {
    root: PathBuf,
    event: LifecycleEvent,
    calls: Rc<RefCell<Vec<&'static str>>>,
}
thread_local! { static HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) }; }

pub(crate) struct Scope;
impl Scope {
    pub fn new(root: &Path, event: LifecycleEvent, calls: Rc<RefCell<Vec<&'static str>>>) -> Self {
        HOOK.with(|slot| {
            assert!(slot.borrow().is_none());
            *slot.borrow_mut() = Some(Hook {
                root: root.canonicalize().unwrap(),
                event,
                calls,
            });
        });
        Self
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        HOOK.with(|slot| {
            slot.borrow_mut().take();
        });
    }
}
pub(crate) fn configure_measurements(root: &Path, session: Option<&mut measurements::Session>) {
    HOOK.with(|slot| {
        if let Some(hook) = slot.borrow().as_ref().filter(|hook| hook.root == root)
            && let Some(session) = session
        {
            session.test_event_failure = Some(hook.event);
        }
    });
}
pub(crate) fn observe_workload(
    root: &Path,
    workload: Box<dyn workloads::WorkloadSession>,
) -> Box<dyn workloads::WorkloadSession> {
    let calls = HOOK.with(|slot| {
        slot.borrow()
            .as_ref()
            .filter(|hook| hook.root == root)
            .map(|hook| hook.calls.clone())
    });
    match calls {
        Some(calls) => Box::new(Observed {
            inner: workload,
            calls,
        }),
        None => workload,
    }
}
struct Observed {
    inner: Box<dyn workloads::WorkloadSession>,
    calls: Rc<RefCell<Vec<&'static str>>>,
}
impl workloads::WorkloadSession for Observed {
    fn invocation(&self) -> &workloads::session::TaskInvocation {
        self.inner.invocation()
    }
    fn evaluation_kind(&self) -> &'static str {
        self.inner.evaluation_kind()
    }
    fn baseline_passed(&self) -> Option<bool> {
        self.inner.baseline_passed()
    }
    fn baseline_evidence(&self) -> workloads::BaselineEvidence {
        self.inner.baseline_evidence()
    }
    fn evaluate(&mut self, response: &str) -> Result<workloads::Evaluation, String> {
        self.calls.borrow_mut().push("evaluate");
        self.inner.evaluate(response)
    }
    fn finish(&mut self) -> Result<workloads::session::ResourceSettlement, String> {
        self.calls.borrow_mut().push("finish");
        self.inner.finish()
    }
    fn checkpoint_evidence(&self) -> Vec<PathBuf> {
        self.inner.checkpoint_evidence()
    }
    fn execute(
        &mut self,
        context: &mut workloads::ExecutionContext<'_>,
        output: &mut dyn Write,
    ) -> Option<Result<workloads::session::ExecutionResult, String>> {
        self.inner.execute(context, output)
    }
}
