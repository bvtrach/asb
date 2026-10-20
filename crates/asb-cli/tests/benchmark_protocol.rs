//! Provider-free tests of the upstream workload protocol's pure data layer.

#[test]
fn swebench_catalog_materialization_is_bounded_private_and_reproducible() {
    python_protocol("swebench_catalog.py");
}

#[test]
fn docker_transport_is_scoped_and_cleans_its_own_processes() {
    python_protocol("benchmark_docker.py");
}

#[test]
fn managed_execution_pauses_for_collectors_and_bounds_each_attempt() {
    python_protocol("benchmark_execution.py");
}

#[test]
fn private_task_protocol_preserves_events_and_cleans_up_on_disconnect() {
    python_protocol("benchmark_task_server.py");
}

#[test]
fn guest_bootstrap_reuses_adapters_without_exposing_host_state() {
    python_protocol("benchmark_guest.py");
}

#[test]
fn terminal_catalogs_preserve_original_populations_and_private_verifiers() {
    python_protocol("terminal_catalog.py");
}

#[test]
fn terminal_execution_isolates_inputs_credentials_and_cleanup() {
    python_protocol("terminal_execution.py");
}

#[test]
fn harbor_execution_preserves_official_verification_and_scopes_resources() {
    python_protocol("harbor_execution.py");
}

fn python_protocol(test: &str) {
    let output = std::process::Command::new("python3")
        .arg(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests")
                .join(test),
        )
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .output()
        .expect("run standard-library-only catalog tests");
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
