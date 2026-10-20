//! Provider-free tests of the upstream workload protocol's pure data layer.

#[test]
fn swebench_catalog_materialization_is_bounded_private_and_reproducible() {
    python_protocol("swebench_catalog.py");
}

#[test]
fn docker_transport_is_scoped_and_cleans_its_own_processes() {
    python_protocol("benchmark_docker.py");
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
