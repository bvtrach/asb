//! Guest admission failures must stop before process execution or credentials.
use super::*;
use crate::endpoint::{Authentication, Interface};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/guest-admission-fixtures")
            .join(crate::workloads::TaskIdentity::local("fixture", None).attempt_id);
        fs::create_dir_all(&root).unwrap();
        Self(root.canonicalize().unwrap())
    }
    fn request(&self) -> GuestRequest {
        let config = toml::from_str::<BenchmarkConfig>(
            "agent='codex'\nprovider='openrouter'\nmodel='fixture'\nworkload='bug-fix'\n",
        )
        .unwrap();
        GuestRequest {
            process_lifetime: Default::default(),
            workload_owner: None,
            config: BenchmarkConfig {
                workspace: self.0.join("workspace").display().to_string(),
                ..config
            },
            connection: Connection {
                interface: Interface::OpenAiResponses,
                base_url: "http://must-never-contact.invalid".into(),
                credential: None,
                authentication: Authentication::None,
            },
            prompt: "offline admission fixture".into(),
            project_root: self.0.join("project"),
            runtime_dir: self.0.join("runtime"),
            run_dir: self.0.join("runtime/run"),
            replay: false,
            timeout_seconds: 1.0,
            namespace_collectors: Vec::new(),
        }
    }
    fn initialize(&self) {
        fs::create_dir_all(self.0.join("project/.asb")).unwrap();
        fs::write(
            self.0.join("project/.asb/project.toml"),
            toml::to_string(&project::ProjectConfig {
                tools_dir: "tools".into(),
                ..Default::default()
            })
            .unwrap(),
        )
        .unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn guest_prepare_rejects_unbounded_mislocated_unregistered_and_uninstalled_requests() {
    let fixture = Fixture::new();
    for timeout in [f64::NAN, f64::INFINITY, -1.0, 0.0, 0.0001, 86400.1] {
        let mut request = fixture.request();
        request.timeout_seconds = timeout;
        assert!(
            prepare(&request)
                .unwrap_err()
                .to_string()
                .contains("time budget must be")
        );
    }
    for prompt in [false, true] {
        let mut request = fixture.request();
        if prompt {
            request.prompt = "x".repeat(65537);
        } else {
            request.config.concurrency = 2;
        }
        assert!(
            prepare(&request)
                .unwrap_err()
                .to_string()
                .contains("one bounded task invocation")
        );
    }
    for case in 0..7 {
        let mut request = fixture.request();
        match case {
            0 => request.config.workspace = "relative".into(),
            1 => request.project_root = "relative".into(),
            2 => request.runtime_dir = "relative".into(),
            3 => request.run_dir = fixture.0.join("runtime/../escape"),
            4 => request.runtime_dir = fixture.0.join("workspace/runtime"),
            5 => request.run_dir = fixture.0.join("workspace/run"),
            6 => request.run_dir = fixture.0.join("outside-runtime"),
            _ => unreachable!(),
        }
        assert!(
            prepare(&request)
                .unwrap_err()
                .to_string()
                .contains("absolute, separate and owned")
        );
    }
    assert!(
        prepare(&fixture.request())
            .unwrap_err()
            .to_string()
            .contains("project is not initialized")
    );
    fixture.initialize();
    let mut request = fixture.request();
    request.config.agent = "unknown-fixture".into();
    assert!(
        prepare(&request)
            .unwrap_err()
            .to_string()
            .contains("registered agent adapter")
    );
    let mut request = fixture.request();
    request.connection.interface = Interface::Gemini;
    assert!(
        prepare(&request)
            .unwrap_err()
            .to_string()
            .contains("endpoint is not supported")
    );
    let mut request = fixture.request();
    request.connection.authentication = Authentication::ApiKey;
    assert!(
        prepare(&request)
            .unwrap_err()
            .to_string()
            .contains("endpoint is not supported")
    );
    assert!(
        prepare(&fixture.request())
            .unwrap_err()
            .to_string()
            .contains("not installed in the guest tool project")
    );
    assert!(
        !fixture.0.join("runtime").exists(),
        "admission must not publish runtime artifacts"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn retained_guest_owner_validation_precedes_all_adapters_and_process_launch() {
    use crate::workloads::session::ProcessLifetime;
    let fixture = Fixture::new();
    let current = namespace_id().unwrap();
    let mut request = fixture.request();
    request.process_lifetime = ProcessLifetime::ThroughEvaluation;
    assert!(
        run(request)
            .unwrap_err()
            .to_string()
            .contains("no namespace owner")
    );
    for (controller_namespace, guest_namespace, token) in [
        (1, current, "a".repeat(32)),
        (current + 1, current + 2, "a".repeat(32)),
        (current, current, "a".repeat(32)),
        (current + 1, current, "a".repeat(31)),
        (current + 1, current, "g".repeat(32)),
    ] {
        let mut request = fixture.request();
        request.process_lifetime = ProcessLifetime::ThroughEvaluation;
        request.workload_owner = Some(WorkloadOwner {
            controller_namespace,
            guest_namespace,
            token,
        });
        assert!(
            run(request)
                .unwrap_err()
                .to_string()
                .contains("distinct owned PID namespace")
        );
    }
    let mut request = fixture.request();
    request.workload_owner = Some(WorkloadOwner {
        controller_namespace: current + 1,
        guest_namespace: current,
        token: "a".repeat(32),
    });
    assert!(
        run(request)
            .unwrap_err()
            .to_string()
            .contains("quiescent guest execution cannot declare retained ownership")
    );
    assert!(!fixture.0.join("runtime").exists());
}
