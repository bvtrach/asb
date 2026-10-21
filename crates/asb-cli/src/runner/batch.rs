//! Cancellable preparation rendezvous; never moves adapter sessions between threads.
use super::replica::Identity;
use crate::{Error, cassette::Compatibility};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

pub(crate) struct Gate {
    state: Mutex<State>,
    changed: Condvar,
    deadline: Instant,
}

struct State {
    registered: Vec<bool>,
    ready: Vec<bool>,
    identity: Option<Compatibility>,
    aborted: Option<String>,
    released: bool,
}

pub(crate) struct Arrival<'a> {
    gate: &'a Gate,
    index: usize,
    settled: bool,
}

impl Gate {
    pub(crate) fn new(
        total: usize,
        expected: Option<Compatibility>,
        timeout: Duration,
    ) -> Result<Self, Error> {
        if !(1..=64).contains(&total) || timeout.is_zero() {
            return Err(Error::Input(
                "invalid preparation gate population or timeout".into(),
            ));
        }
        if let Some(identity) = &expected {
            identity
                .validate()
                .map_err(|error| Error::Input(error.to_string()))?;
        }
        Ok(Self {
            state: Mutex::new(State {
                registered: vec![false; total],
                ready: vec![false; total],
                identity: expected,
                aborted: None,
                released: false,
            }),
            changed: Condvar::new(),
            deadline: Instant::now()
                .checked_add(timeout)
                .ok_or_else(|| Error::Input("preparation deadline overflows".into()))?,
        })
    }

    pub(crate) fn register(&self, replica: Identity) -> Result<Arrival<'_>, Error> {
        let mut state = self.state.lock().map_err(|_| unavailable())?;
        if !replica.valid()
            || replica.total != state.registered.len()
            || state.registered[replica.index - 1]
            || state.released
        {
            state.aborted = Some("invalid or duplicate preparation slot".into());
            self.changed.notify_all();
            return Err(Error::Run("invalid or duplicate preparation slot".into()));
        }
        if let Some(reason) = &state.aborted {
            return Err(Error::Run(reason.clone()));
        }
        state.registered[replica.index - 1] = true;
        Ok(Arrival {
            gate: self,
            index: replica.index - 1,
            settled: false,
        })
    }
}

impl Arrival<'_> {
    pub(crate) fn abort(&mut self, reason: &str) {
        let mut state = self
            .gate
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !state.released && state.aborted.is_none() {
            state.aborted = Some(reason.into());
        }
        self.gate.changed.notify_all();
    }

    pub(crate) fn ready(&mut self, identity: Compatibility) -> Result<(), Error> {
        let mut state = self.gate.state.lock().map_err(|_| unavailable())?;
        if Instant::now() >= self.gate.deadline && !state.released {
            state.aborted = Some("batch preparation timed out; no agent was admitted".into());
        } else if self.settled || state.ready[self.index] {
            state.aborted = Some("preparation slot arrived more than once".into());
        } else if let Err(error) = identity.validate() {
            state.aborted = Some(error.to_string());
        } else if state
            .identity
            .as_ref()
            .is_some_and(|expected| expected != &identity)
        {
            state.aborted =
                Some("prepared workload material or prompt differs within/across batches".into());
        } else {
            state.identity.get_or_insert(identity);
            state.ready[self.index] = true;
            if state.ready.iter().all(|ready| *ready) && state.aborted.is_none() {
                state.released = true;
            }
        }
        self.gate.changed.notify_all();
        loop {
            if let Some(reason) = &state.aborted {
                return Err(Error::Run(reason.clone()));
            }
            if state.released {
                self.settled = true;
                return Ok(());
            }
            let remaining = self.gate.deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                state.aborted = Some("batch preparation timed out; no agent was admitted".into());
                self.gate.changed.notify_all();
                continue;
            }
            let (next, _) = self
                .gate
                .changed
                .wait_timeout(state, remaining.min(Duration::from_secs(1)))
                .map_err(|_| unavailable())?;
            state = next;
        }
    }
}

impl Drop for Arrival<'_> {
    fn drop(&mut self) {
        if !self.settled {
            self.abort(&format!(
                "replica {} left before batch preparation completed",
                self.index + 1
            ));
        }
    }
}

fn unavailable() -> Error {
    Error::Run("batch preparation coordination was interrupted".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn material() -> Compatibility {
        Compatibility {
            agent: "agent".into(),
            provider: "provider".into(),
            model: "model".into(),
            workload: "workload".into(),
            task: "task".into(),
            dataset_sha256: None,
            prompt_sha256: "a".repeat(64),
            material_sha256: "b".repeat(64),
        }
    }

    #[test]
    fn no_agent_admitted_until_all_material_matches() {
        let gate = Gate::new(2, None, Duration::from_secs(3)).unwrap();
        std::thread::scope(|scope| {
            let (tx, rx) = mpsc::channel();
            let mut first = gate.register(Identity { index: 1, total: 2 }).unwrap();
            scope.spawn(move || {
                first.ready(material()).unwrap();
                tx.send(()).unwrap();
            });
            assert!(rx.recv_timeout(Duration::from_millis(30)).is_err());
            gate.register(Identity { index: 2, total: 2 })
                .unwrap()
                .ready(material())
                .unwrap();
            rx.recv_timeout(Duration::from_secs(1)).unwrap();
        });
    }

    #[test]
    fn early_return_panic_mismatch_duplicate_and_timeout_abort_waiters() {
        for fault in ["return", "panic", "mismatch", "duplicate", "timeout"] {
            let gate = Gate::new(2, None, Duration::from_millis(200)).unwrap();
            std::thread::scope(|scope| {
                let mut first = gate.register(Identity { index: 1, total: 2 }).unwrap();
                let waiter = scope.spawn(move || first.ready(material()));
                if fault != "timeout" {
                    let mut second = gate.register(Identity { index: 2, total: 2 }).unwrap();
                    match fault {
                        "return" => drop(second),
                        "panic" => {
                            assert!(
                                scope
                                    .spawn(move || {
                                        let _guard = second;
                                        panic!("fixture");
                                    })
                                    .join()
                                    .is_err()
                            );
                        }
                        "mismatch" => {
                            let mut value = material();
                            value.material_sha256 = "c".repeat(64);
                            assert!(second.ready(value).is_err());
                        }
                        "duplicate" => {
                            assert!(gate.register(Identity { index: 2, total: 2 }).is_err());
                            drop(second);
                        }
                        _ => unreachable!(),
                    }
                }
                assert!(waiter.join().unwrap().is_err(), "{fault}");
            });
            assert!(gate.register(Identity { index: 2, total: 2 }).is_err());
        }
    }

    #[test]
    fn reference_from_earlier_batch_rejects_changed_material() {
        let gate = Gate::new(1, Some(material()), Duration::from_secs(1)).unwrap();
        let mut changed = material();
        changed.prompt_sha256 = "d".repeat(64);
        assert!(
            gate.register(Identity::default())
                .unwrap()
                .ready(changed)
                .is_err()
        );
        assert!(Gate::new(0, None, Duration::from_secs(1)).is_err());
        assert!(Gate::new(65, None, Duration::from_secs(1)).is_err());
        assert!(Gate::new(1, None, Duration::ZERO).is_err());
    }

    #[test]
    fn expired_gate_never_releases_single_or_all_late_arrivals() {
        for total in [1, 2] {
            let gate = Gate::new(total, None, Duration::from_millis(1)).unwrap();
            let mut arrivals: Vec<_> = (1..=total)
                .map(|index| gate.register(Identity { index, total }).unwrap())
                .collect();
            std::thread::sleep(Duration::from_millis(5));
            for arrival in &mut arrivals {
                assert!(
                    arrival
                        .ready(material())
                        .unwrap_err()
                        .to_string()
                        .contains("timed out")
                );
            }
        }
    }

    #[test]
    fn runner_abort_notifies_peers_before_slow_cancellation_and_preserves_cleanup_error() {
        struct Session(std::sync::mpsc::Receiver<()>);
        impl crate::workloads::WorkloadSession for Session {
            fn invocation(&self) -> &crate::workloads::session::TaskInvocation {
                unreachable!()
            }
            fn evaluation_kind(&self) -> &'static str {
                "fixture"
            }
            fn baseline_passed(&self) -> Option<bool> {
                None
            }
            fn evaluate(&mut self, _: &str) -> Result<crate::workloads::Evaluation, String> {
                unreachable!()
            }
            fn cancel_prepared(&mut self) -> Result<(), String> {
                self.0
                    .recv_timeout(Duration::from_secs(1))
                    .expect("peer must wake before cleanup completes");
                Err("fixture preservation failure".into())
            }
        }
        let gate = Gate::new(2, None, Duration::from_secs(3)).unwrap();
        std::thread::scope(|scope| {
            let mut first = gate.register(Identity { index: 1, total: 2 }).unwrap();
            let mut second = gate.register(Identity { index: 2, total: 2 }).unwrap();
            let (tx, rx) = mpsc::channel();
            let peer = scope.spawn(move || {
                assert!(first.ready(material()).is_err());
                tx.send(()).unwrap();
            });
            let error = super::super::abort_prepared(
                Error::Run("identity unavailable".into()),
                &mut second,
                &mut Session(rx),
            );
            assert!(error.to_string().contains("identity unavailable"));
            assert!(error.to_string().contains("fixture preservation failure"));
            peer.join().unwrap();
        });
    }

    #[cfg(unix)]
    #[test]
    fn runner_single_unsupported_and_multi_mismatch_stop_before_proxy_or_agent_launch() {
        use std::fs;
        use std::os::unix::fs::symlink;
        for concurrency in [1, 2] {
            let root = std::env::temp_dir()
                .join(crate::workloads::TaskIdentity::local("batch-runner", None).attempt_id);
            fs::create_dir_all(root.join(".asb")).unwrap();
            fs::write(
                root.join(".asb/project.toml"),
                toml::to_string(&crate::project::ProjectConfig::default()).unwrap(),
            )
            .unwrap();
            fs::create_dir(root.join("workloads")).unwrap();
            crate::workloads::populate(&root.join("workloads")).unwrap();
            let workspace = root.join(if concurrency == 1 {
                "workspace"
            } else {
                "workspace-concurrency-1"
            });
            fs::create_dir(&workspace).unwrap();
            if concurrency == 1 {
                symlink("/does/not/exist", workspace.join("unsupported")).unwrap();
            } else {
                fs::write(
                    workspace.join("different-initial-content"),
                    "preserve user file",
                )
                .unwrap();
            }
            let config = root.join("benchmark.toml");
            fs::write(&config, format!("agent='/must-not-launch-fixture-agent'\nprovider='fixture'\nmodel='fixture'\nworkload='bug-fix'\nworkspace='workspace'\nlog_file='run.log'\nconcurrency={concurrency}\n")).unwrap();
            let cassette = root.join("input.jsonl");
            // Satisfies preflight; intentionally not a valid proxy trace. If the
            // gate is bypassed the error changes, so an unrelated proxy failure
            // cannot make this no-launch regression pass accidentally.
            fs::write(
                &cassette,
                "{\"request\":{\"body\":{\"model\":\"fixture\",\"tools\":[{}]}}}\n",
            )
            .unwrap();
            let error = super::super::run_with_tape(
                config.to_str().unwrap(),
                super::super::RunOptions {
                    verify_initial_material: true,
                    mode: Some(crate::tape::Mode::Replay),
                    cassette: cassette.to_str(),
                    json: true,
                    ..Default::default()
                },
                &mut Vec::new(),
            )
            .unwrap_err()
            .to_string();
            let expected = if concurrency == 1 {
                "unsupported symlink"
            } else {
                "differs within/across batches"
            };
            assert!(error.contains(expected), "{concurrency}: {error}");
            assert!(!root.join("run.log").exists());
            assert!(!workspace.with_extension("asb-agent-runtime").exists());
            if concurrency == 2 {
                assert_eq!(
                    fs::read_to_string(workspace.join("different-initial-content")).unwrap(),
                    "preserve user file"
                );
            }
            fs::remove_dir_all(root).unwrap();
        }
    }
}
