//! Real CLI rejection before providers, collectors, environments or output moves.
mod support;
use std::{path::PathBuf, process::Command};
use support::IsolatedEnvironment;

struct Project(PathBuf);
impl Project {
    fn new() -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/planning-fixtures")
            .join(format!("{}-{unique}", std::process::id()));
        std::fs::create_dir_all(root.join(".asb")).unwrap();
        std::fs::write(
            root.join(".asb/project.toml"),
            "tools_dir='.asb-tools'\nworkloads_dir='workloads'\nworkspace_dir='asb-workspace'\n",
        )
        .unwrap();
        Self(root)
    }
    fn write(&self, text: &str) {
        std::fs::write(
            self.0.join("benchmark.toml"),
            format!(
                "agent='pi'\nworkload='bug-fix'\nprovider='openrouter'\nmodel='fixture'\n{text}"
            ),
        )
        .unwrap();
    }
    fn reject(&self, args: &[&str], reason: &str) {
        self.reject_with_environment(args, reason, &[]);
    }
    fn reject_with_environment(&self, args: &[&str], reason: &str, environment: &[(&str, &str)]) {
        let mut command = Command::new(env!("CARGO_BIN_EXE_asb"));
        let output = command
            .isolated_env()
            .envs(environment.iter().copied())
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success(), "unexpected success");
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains(reason), "expected {reason:?}: {error}");
        for path in [
            "asb-workspace",
            ".asb-tools",
            ".asb/attempts",
            ".asb/execution-plans",
            "measurements",
            "benchmark.log",
        ] {
            assert!(!self.0.join(path).exists(), "preflight created {path}");
        }
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn backend_and_full_preparation_deadlines_fail_before_side_effects() {
    let project = Project::new();
    for (config, reason) in [
        ("[planning]\nbackend='remote'", "backend is unsupported"),
        (
            "[planning]\nrequired_policies=['network-denied']",
            "cannot be enforced",
        ),
        ("[planning.budgets]\npreparation_seconds=1", "unknown field"),
        (
            "[planning.budgets]\nagent_preparation_seconds=1",
            "end-to-end deadline",
        ),
        (
            "[planning.repetition]\nindex=0\ntotal=2",
            "repetition index",
        ),
    ] {
        project.write(config);
        project.reject(&["run", "benchmark.toml"], reason);
    }
}

#[test]
fn measurement_override_negotiates_collectors_even_when_configuration_disables_collection() {
    let project = Project::new();
    project.write(
        "[measurements]\nenabled=false\ndirectory='measurements'\ncollectors=['unknown-collector']",
    );
    project.reject(
        &["run", "benchmark.toml", "--measurements", "measurements"],
        "unknown measurement collector",
    );
    let config = "agent='pi'\nworkload='terminal-bench-2.1'\nprovider='openrouter'\nmodel='fixture'\n[planning]\nrequired_policies=['verified-local-target']\n";
    std::fs::write(project.0.join("benchmark.toml"), config).unwrap();
    project.reject(&["run", "benchmark.toml"], "cannot be enforced");
    for variable in [
        ("DOCKER_HOST", "ssh://remote"),
        ("DOCKER_CONTEXT", "remote"),
    ] {
        project.reject_with_environment(
            &["run", "benchmark.toml"],
            "remote Docker endpoints/contexts",
            &[variable],
        );
    }
}

#[test]
fn impossible_or_unknown_replica_resources_preserve_previous_results() {
    let project = Project::new();
    std::fs::create_dir_all(project.0.join("sweep-results")).unwrap();
    std::fs::write(project.0.join("sweep-results/keep"), "previous results").unwrap();
    std::fs::write(project.0.join("sweep.toml"), "configs=['benchmark.toml']\n[resources]\ncpu_millis=2000\nmemory_mib=8192\ndisk_mib=4096\n").unwrap();
    for (config, reason) in [
        (
            "concurrency=3\n[planning.reservation]\ncpu_millis=1000\nmemory_mib=4096\ndisk_mib=2048",
            "exceeds sweep resources",
        ),
        ("", "explicit planning.reservation"),
        (
            "[planning.reservation]\ncpu_millis=0\nmemory_mib=0\ndisk_mib=0",
            "empty reservations",
        ),
    ] {
        project.write(config);
        project.reject(&["sweep", "sweep.toml"], reason);
        assert_eq!(
            std::fs::read_to_string(project.0.join("sweep-results/keep")).unwrap(),
            "previous results"
        );
        assert!(!project.0.join("sweep-history").exists());
    }
}
