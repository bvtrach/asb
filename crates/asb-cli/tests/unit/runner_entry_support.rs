//! Private per-thread observation of a real runner attempt; no factory bypass.
use super::*;
use std::cell::RefCell;
use std::rc::Rc;

struct Hook {
    root: PathBuf,
    event: Option<LifecycleEvent>,
    calls: Rc<RefCell<Vec<&'static str>>>,
    provider_finish: Option<bool>,
    native_reward: bool,
}
thread_local! { static HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) }; }

pub(crate) struct Scope;
impl Scope {
    pub fn new(root: &Path, event: LifecycleEvent, calls: Rc<RefCell<Vec<&'static str>>>) -> Self {
        HOOK.with(|slot| {
            assert!(slot.borrow().is_none());
            *slot.borrow_mut() = Some(Hook {
                root: root.canonicalize().unwrap(),
                event: Some(event),
                calls,
                provider_finish: None,
                native_reward: false,
            });
        });
        Self
    }

    pub fn provider(
        root: &Path,
        calls: Rc<RefCell<Vec<&'static str>>>,
        fail: bool,
        native_reward: bool,
    ) -> Self {
        HOOK.with(|slot| {
            assert!(slot.borrow().is_none());
            *slot.borrow_mut() = Some(Hook {
                root: root.canonicalize().unwrap(),
                event: None,
                calls,
                provider_finish: Some(fail),
                native_reward,
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
            session.test_event_failure = hook.event;
        }
    });
}

/// A private replay-only session double exercises generic finish obligations.
pub(crate) fn observe_provider(
    root: &Path,
    original: Option<Box<dyn providers::ProviderSession>>,
) -> Option<Box<dyn providers::ProviderSession>> {
    let shared = SHARED_PROVIDERS.lock().unwrap().get(root).cloned();
    if let Some(finish) = shared {
        assert!(
            original.is_none(),
            "shared provider fixture requires replay"
        );
        return Some(Box::new(SharedProvider { finish }));
    }
    HOOK.with(|slot| {
        let slot = slot.borrow();
        let Some(hook) = slot
            .as_ref()
            .filter(|hook| hook.root == root && hook.provider_finish.is_some())
        else {
            return original;
        };
        assert!(
            original.is_none(),
            "provider fixture requires replay, never a live bridge"
        );
        Some(Box::new(Provider {
            calls: hook.calls.clone(),
            fail: hook.provider_finish.unwrap(),
            finished: false,
        }))
    })
}
type SharedFinish = dyn Fn() -> Result<(), Error> + Send + Sync;
static SHARED_PROVIDERS: std::sync::Mutex<
    std::collections::BTreeMap<PathBuf, std::sync::Arc<SharedFinish>>,
> = std::sync::Mutex::new(std::collections::BTreeMap::new());

/// Root-scoped fixture shared with actual concurrent runner threads. No runtime
/// environment switches or production provider registry changes are involved.
pub(crate) struct SharedProviderScope(PathBuf);
impl SharedProviderScope {
    pub fn new(root: &Path, finish: std::sync::Arc<SharedFinish>) -> Self {
        let root = root.canonicalize().unwrap();
        assert!(
            SHARED_PROVIDERS
                .lock()
                .unwrap()
                .insert(root.clone(), finish)
                .is_none()
        );
        Self(root)
    }
}
impl Drop for SharedProviderScope {
    fn drop(&mut self) {
        SHARED_PROVIDERS.lock().unwrap().remove(&self.0);
    }
}
struct SharedProvider {
    finish: std::sync::Arc<SharedFinish>,
}
impl providers::ProviderSession for SharedProvider {
    fn api_key(&self) -> &str {
        "fixture-key"
    }
    fn base_url(&self) -> &str {
        "http://127.0.0.1:1/v1"
    }
    fn finish(&mut self) -> Result<(), Error> {
        (self.finish)()
    }
}
struct Provider {
    calls: Rc<RefCell<Vec<&'static str>>>,
    fail: bool,
    finished: bool,
}
impl providers::ProviderSession for Provider {
    fn api_key(&self) -> &str {
        "fixture-key"
    }
    fn base_url(&self) -> &str {
        "http://127.0.0.1:1/v1"
    }
    fn finish(&mut self) -> Result<(), Error> {
        assert!(!self.finished, "finish called twice");
        self.finished = true;
        self.calls.borrow_mut().push("provider-finish");
        if self.fail {
            Err(Error::Run("fixture provider cleanup uncertain".into()))
        } else {
            Ok(())
        }
    }
}
impl Drop for Provider {
    fn drop(&mut self) {
        if !self.finished {
            self.calls.borrow_mut().push("provider-fallback");
        }
    }
}
pub(crate) fn observe_workload(
    root: &Path,
    workload: Box<dyn workloads::WorkloadSession>,
) -> Box<dyn workloads::WorkloadSession> {
    let calls = HOOK.with(|slot| {
        slot.borrow()
            .as_ref()
            .filter(|hook| hook.root == root)
            .map(|hook| (hook.calls.clone(), hook.native_reward))
    });
    match calls {
        Some((calls, native_reward)) => Box::new(Observed {
            inner: workload,
            calls,
            native_reward,
        }),
        None => workload,
    }
}
struct Observed {
    inner: Box<dyn workloads::WorkloadSession>,
    calls: Rc<RefCell<Vec<&'static str>>>,
    native_reward: bool,
}
impl workloads::WorkloadSession for Observed {
    fn invocation(&self) -> &workloads::session::TaskInvocation {
        self.inner.invocation()
    }
    fn effective_budgets(&self) -> crate::planning::EffectiveBudgets {
        self.inner.effective_budgets()
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
    fn evaluate_outcome(&mut self, response: &str) -> workloads::outcome::EvaluationOutcome {
        self.calls.borrow_mut().push("evaluate");
        if self.native_reward {
            std::fs::write(
                self.invocation().workspace.join("native-grade.json"),
                "fixture numeric evaluator evidence",
            )
            .unwrap();
            workloads::outcome::EvaluationOutcome::Completed {
                resolution: None,
                rewards: std::collections::BTreeMap::from([("accuracy".into(), 0.5)]),
                reason: None,
            }
        } else {
            self.inner.evaluate_outcome(response)
        }
    }
    fn metric_policy(&self) -> workloads::outcome::MetricPolicy {
        if !self.native_reward {
            return self.inner.metric_policy();
        }
        workloads::outcome::MetricPolicy {
            id: "accuracy".into(),
            reward: "accuracy".into(),
            rewards: std::collections::BTreeMap::from([(
                "accuracy".into(),
                workloads::outcome::RewardSpec {
                    unit: "ratio".into(),
                    range: Some([0.0, 1.0]),
                },
            )]),
            ..Default::default()
        }
    }
    fn module_descriptor(&self) -> Option<workloads::contracts::ModuleDescriptor> {
        self.inner.module_descriptor().map(|mut module| {
            if self.native_reward {
                module.protocol.score_policy = "metric:accuracy@1".into();
            }
            module
        })
    }
    fn finish(&mut self) -> Result<workloads::session::ResourceSettlement, String> {
        self.calls.borrow_mut().push("finish");
        self.inner.finish()
    }
    fn checkpoint_evidence(&self) -> Vec<PathBuf> {
        if self.native_reward {
            vec![self.invocation().workspace.join("native-grade.json")]
        } else {
            self.inner.checkpoint_evidence()
        }
    }
    fn execute(
        &mut self,
        context: &mut workloads::ExecutionContext<'_>,
        output: &mut dyn Write,
    ) -> Option<Result<workloads::session::ExecutionResult, String>> {
        self.inner.execute(context, output)
    }
}
