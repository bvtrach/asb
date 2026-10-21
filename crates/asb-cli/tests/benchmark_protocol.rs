//! Provider-free tests of the upstream workload protocol's pure data layer.

#[test]
fn owned_root_material_composes_complete_rounds_without_definition_fallbacks() {
    python_protocol("benchmark_material_root.py");
}

#[test]
fn material_helper_transport_is_bounded_cancellable_and_provider_free() {
    python_protocol("benchmark_material_exec.py");
}

#[test]
fn prepared_material_archive_identity_is_bounded_canonical_and_provider_free() {
    python_protocol("benchmark_material.py");
}

#[test]
fn terminal_sdk_return_reconstruction_never_repeats_verifiers_or_invents_lost_metrics() {
    python_protocol("benchmark_verifier_reconstruction.py");
}

#[test]
fn retained_harbor_grading_uses_original_verifier_without_recreating_environments() {
    python_protocol("benchmark_harbor_grading.py");
}

#[test]
fn original_terminal_grading_preserves_resources_and_never_repeats_lost_shell_tests() {
    python_protocol("benchmark_terminal_grading.py");
}

#[test]
fn retained_submissions_use_official_grading_without_rerunning_agents_or_replacing_grades() {
    python_protocol("benchmark_grading.py");
}

#[test]
fn retained_grades_import_into_sweep_reports_without_execution_metrics_or_resume_receipts() {
    python_protocol("benchmark_import.py");
}

#[test]
fn retained_original_grades_are_checked_without_executing_agents_or_graders() {
    python_protocol("benchmark_retained.py");
}

#[test]
fn resource_recovery_preserves_evidence_and_only_quiesces_declared_owners() {
    python_protocol("benchmark_resources.py");
}

#[test]
fn interrupted_original_adapters_preserve_partial_work_without_inventing_a_grade() {
    python_protocol("benchmark_interrupted.py");
}

#[test]
fn swebench_catalog_materialization_is_bounded_private_and_reproducible() {
    python_protocol("swebench_catalog.py");
}

#[test]
fn swebench_source_fallback_is_pinned_private_and_attempt_owned() {
    python_protocol("swebench_sources.py");
}

#[test]
fn benchmark_fingerprints_bind_content_without_exposing_secrets() {
    python_protocol("benchmark_fingerprint.py");
}

#[test]
fn declared_runtime_dependencies_are_bounded_and_do_not_start_apps() {
    python_protocol("benchmark_runtime_inputs.py");
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
fn runtime_bundle_executes_the_same_asb_without_a_guest_compiler() {
    python_protocol("benchmark_runtime.py");
}

#[test]
fn guest_relay_is_bounded_binary_transparent_and_closes_owned_sockets() {
    python_protocol("benchmark_relay.py");
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
        .env("ASB_TEST_HOST_ASB", env!("CARGO_BIN_EXE_asb"))
        .output()
        .expect("run standard-library-only catalog tests");
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
