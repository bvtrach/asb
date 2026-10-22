//! Shared process isolation for offline fixtures, retaining only instrumentation.

use std::process::Command;

#[cfg(unix)]
pub mod fixture_writer;

/// Clear inherited credentials/state without losing subprocess LLVM coverage.
pub trait IsolatedEnvironment {
    fn isolated_env(&mut self) -> &mut Self;
}

impl IsolatedEnvironment for Command {
    fn isolated_env(&mut self) -> &mut Self {
        self.env_clear();
        if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
            self.env("LLVM_PROFILE_FILE", profile);
        }
        self
    }
}

#[test]
fn isolated_environment_preserves_only_coverage_instrumentation() {
    let mut command = Command::new("fixture-not-executed");
    command.env("OPENROUTER_API_KEY", "must-not-be-inherited");
    command.env("ASB_GUEST_AGENT_REQUEST", "must-not-be-inherited");
    command.isolated_env();
    let actual = command.get_envs().collect::<Vec<_>>();
    let expected = std::env::var_os("LLVM_PROFILE_FILE");
    match expected.as_ref() {
        Some(value) => {
            assert_eq!(actual.len(), 1);
            assert_eq!(actual[0].0, "LLVM_PROFILE_FILE");
            assert_eq!(actual[0].1, Some(value.as_os_str()));
        }
        None => assert!(actual.is_empty()),
    }
}
