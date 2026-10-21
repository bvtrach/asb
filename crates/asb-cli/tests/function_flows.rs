//! Build-time diagram regressions; no agent, provider or host collector.
#[test]
fn clustered_function_diagrams_render_without_graphviz_crashes() {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/test_function_flows.py");
    if !script.exists() {
        // Build-time workspace scripts are not part of the published CLI crate.
        eprintln!("workspace diagram test is not applicable to the packaged CLI");
        return;
    }
    let output = std::process::Command::new("python3")
        .arg(script)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .output()
        .expect("run diagram regressions");
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
