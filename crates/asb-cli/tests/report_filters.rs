//! Execute the embedded offline report's JavaScript without a provider or GUI.
#[test]
fn filters_update_graphs_and_table_for_every_dimension() {
    let script = include_str!("../src/compare/report_filters.js");
    let program = format!(
        "const reportScript = {};\n{}",
        serde_json::to_string(script).unwrap(),
        include_str!("report_filters_fixture.js")
    );
    let output = std::process::Command::new("node")
        .args(["-e", &program])
        .output()
        .expect("Node.js is required by the offline report tests");
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
