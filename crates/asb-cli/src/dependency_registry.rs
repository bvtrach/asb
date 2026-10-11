// Copyright (C) Huawei Technologies Co., Ltd. 2026. All rights reserved.
// SPDX-License-Identifier: MIT
//! Typed command dependency inventory and side-effect-free preflight.
#![allow(missing_docs)]

use crate::tool_catalog;
use asb_config::{ProjectToolStatus, decode_project_config};
use serde::Serialize;
use serde_json::json;
use std::path::Path;

const VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyKind {
    Tool,
    Workload,
    Project,
    Host,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct Dependency {
    pub id: &'static str,
    pub kind: DependencyKind,
    pub optional: bool,
    pub remediation: &'static str,
}

/// The authoritative dependency map for public execution paths.
pub const DEPENDENCIES: &[Dependency] = &[
    Dependency {
        id: "aider",
        kind: DependencyKind::Tool,
        optional: false,
        remediation: "asb tool install aider --kind agent --source fixture://aider --version <pinned>",
    },
    Dependency {
        id: "codex",
        kind: DependencyKind::Tool,
        optional: false,
        remediation: "asb tool install codex --kind agent --source fixture://codex --version <pinned>",
    },
    Dependency {
        id: "gemini",
        kind: DependencyKind::Tool,
        optional: false,
        remediation: "asb tool install gemini --kind agent --source fixture://gemini --version <pinned>",
    },
    Dependency {
        id: "goose",
        kind: DependencyKind::Tool,
        optional: false,
        remediation: "asb tool install goose --kind agent --source fixture://goose --version <pinned>",
    },
    Dependency {
        id: "mini-swe",
        kind: DependencyKind::Tool,
        optional: false,
        remediation: "asb tool install mini-swe --kind agent --source fixture://mini-swe --version <pinned>",
    },
    Dependency {
        id: "opencode",
        kind: DependencyKind::Tool,
        optional: false,
        remediation: "asb tool install opencode --kind agent --source fixture://opencode --version <pinned>",
    },
    Dependency {
        id: "opendesk",
        kind: DependencyKind::Tool,
        optional: false,
        remediation: "asb tool install opendesk --kind agent --source fixture://opendesk --version <pinned>",
    },
    Dependency {
        id: "openhands",
        kind: DependencyKind::Tool,
        optional: false,
        remediation: "asb tool install openhands --kind agent --source fixture://openhands --version <pinned>",
    },
    Dependency {
        id: "openjiuwen",
        kind: DependencyKind::Tool,
        optional: false,
        remediation: "asb tool install openjiuwen --kind agent --source fixture://openjiuwen --version <pinned>",
    },
    Dependency {
        id: "qwen-code",
        kind: DependencyKind::Tool,
        optional: false,
        remediation: "asb tool install qwen-code --kind agent --source fixture://qwen-code --version <pinned>",
    },
    Dependency {
        id: "cli2key",
        kind: DependencyKind::Tool,
        optional: false,
        remediation: "asb tool install cli2key --kind support --source fixture://cli2key --version <pinned>",
    },
    Dependency {
        id: "perf",
        kind: DependencyKind::Host,
        optional: true,
        remediation: "install perf using the host distribution package manager",
    },
    Dependency {
        id: "bpftool",
        kind: DependencyKind::Host,
        optional: true,
        remediation: "install bpftool using the host distribution package manager",
    },
    Dependency {
        id: "linux",
        kind: DependencyKind::Host,
        optional: false,
        remediation: "run ASB on a Linux host",
    },
    Dependency {
        id: "cgroup-v2",
        kind: DependencyKind::Host,
        optional: false,
        remediation: "mount and delegate a cgroup v2 hierarchy to the user",
    },
    Dependency {
        id: "user-systemd",
        kind: DependencyKind::Host,
        optional: false,
        remediation: "enable user systemd delegation for the benchmark user",
    },
    Dependency {
        id: "bubblewrap",
        kind: DependencyKind::Host,
        optional: false,
        remediation: "install a distribution-provided bubblewrap executable",
    },
    Dependency {
        id: "kernel-btf",
        kind: DependencyKind::Host,
        optional: true,
        remediation: "boot a kernel exposing BTF at /sys/kernel/btf/vmlinux",
    },
    Dependency {
        id: "kernel-pmu",
        kind: DependencyKind::Host,
        optional: true,
        remediation: "grant the benchmark user permitted PMU access",
    },
];

/// Public execution scopes and the dependencies they may require. Keeping
/// this mapping separate from the inventory makes command coverage explicit
/// without allowing a command to silently inherit an ambient tool.
pub const DEPENDENCY_SCOPES: &[(&str, &[&str])] = &[
    (
        "run",
        &[
            "aider",
            "codex",
            "gemini",
            "goose",
            "mini-swe",
            "opencode",
            "opendesk",
            "openhands",
            "openjiuwen",
            "qwen-code",
            "cli2key",
            "linux",
            "cgroup-v2",
            "user-systemd",
            "bubblewrap",
            "kernel-btf",
            "kernel-pmu",
        ],
    ),
    (
        "sweep",
        &[
            "aider",
            "codex",
            "gemini",
            "goose",
            "mini-swe",
            "opencode",
            "opendesk",
            "openhands",
            "openjiuwen",
            "qwen-code",
            "cli2key",
            "linux",
            "cgroup-v2",
            "user-systemd",
            "bubblewrap",
            "kernel-btf",
            "kernel-pmu",
        ],
    ),
    (
        "benchmark-live",
        &[
            "aider",
            "codex",
            "gemini",
            "goose",
            "mini-swe",
            "opencode",
            "opendesk",
            "openhands",
            "openjiuwen",
            "qwen-code",
            "cli2key",
            "linux",
            "cgroup-v2",
            "user-systemd",
            "bubblewrap",
            "kernel-btf",
            "kernel-pmu",
        ],
    ),
    (
        "preflight",
        &[
            "aider",
            "codex",
            "gemini",
            "goose",
            "mini-swe",
            "opencode",
            "opendesk",
            "openhands",
            "openjiuwen",
            "qwen-code",
            "cli2key",
            "perf",
            "bpftool",
            "linux",
            "cgroup-v2",
            "user-systemd",
            "bubblewrap",
            "kernel-btf",
            "kernel-pmu",
        ],
    ),
];

#[derive(Serialize)]
pub struct ResultView<'a> {
    pub schema_version: u16,
    pub command: &'static str,
    pub ready: bool,
    pub dependencies: Vec<Check<'a>>,
}
#[derive(Serialize)]
pub struct Check<'a> {
    pub id: &'a str,
    pub kind: DependencyKind,
    pub optional: bool,
    pub available: bool,
    pub remediation: &'a str,
}

pub fn preflight(project: &Path) -> Result<ResultView<'static>, String> {
    preflight_for_selected("preflight", project, None, None)
}

pub fn preflight_for(command: &str, project: &Path) -> Result<ResultView<'static>, String> {
    preflight_for_selected(command, project, None, None)
}

/// Evaluate only the selected agent/runtime dependency paths.  An omitted
/// selector retains the complete command scope for compatibility; callers
/// which have a concrete execution selection must pass it so unrelated agent
/// IDs cannot make the readiness result fail.
pub fn preflight_for_selected(
    command: &str,
    project: &Path,
    agent: Option<&str>,
    runtime: Option<&str>,
) -> Result<ResultView<'static>, String> {
    let scope = DEPENDENCY_SCOPES
        .iter()
        .find(|(name, _)| *name == command)
        .map(|(_, ids)| *ids)
        .ok_or_else(|| "dependency scope is unknown".to_owned())?;
    let catalog = tool_catalog::catalog();
    tool_catalog::validate(&catalog).map_err(str::to_owned)?;
    let config_path = project.join(".asb/project.json");
    let config = if config_path.is_file() {
        let bytes = std::fs::read(&config_path)
            .map_err(|_| "project configuration is unreadable".to_owned())?;
        Some(
            decode_project_config(&bytes)
                .map_err(|_| "project configuration is invalid".to_owned())?,
        )
    } else {
        None
    };
    if let Some(agent) = agent
        && !scope.contains(&agent)
    {
        return Err("selected agent is not declared for this command".to_owned());
    }
    if let Some(runtime) = runtime
        && !matches!(runtime, "host" | "local")
    {
        return Err("selected runtime is not supported".to_owned());
    }
    let checks = DEPENDENCIES
        .iter()
        .filter(|d| scope.contains(&d.id))
        .filter(|d| agent.is_none_or(|selected| d.kind != DependencyKind::Tool || d.id == selected))
        .filter(|d| {
            runtime.is_none_or(|selected| {
                selected == "host"
                    && (d.kind == DependencyKind::Host
                        || agent.is_some_and(|selected_agent| d.id == selected_agent))
                    || selected == "local" && d.kind != DependencyKind::Host
            })
        })
        .map(|d| {
            let available = match d.kind {
                DependencyKind::Tool => {
                    catalog.entries.iter().any(|e| e.id == d.id)
                        && config.as_ref().is_some_and(|c| tool_available(c, d.id))
                }
                DependencyKind::Workload | DependencyKind::Project => true,
                DependencyKind::Host => host_available(d.id),
            };
            Check {
                id: d.id,
                kind: d.kind,
                optional: d.optional,
                available,
                remediation: d.remediation,
            }
        })
        .collect::<Vec<_>>();
    let ready = checks.iter().all(|c| c.available || c.optional);
    Ok(ResultView {
        schema_version: VERSION,
        command: "preflight",
        ready,
        dependencies: checks,
    })
}

fn tool_available(config: &asb_config::ProjectConfigV1, id: &str) -> bool {
    config
        .agents
        .get(id)
        .or_else(|| config.harnesses.get(id))
        .or_else(|| config.benchmarks.get(id))
        .or_else(|| config.workloads.get(id))
        .or_else(|| config.support_tools.get(id))
        .is_some_and(|record| record.status == ProjectToolStatus::Available)
}

fn host_available(id: &str) -> bool {
    match id {
        "linux" => std::env::consts::OS == "linux",
        "cgroup-v2" => Path::new("/sys/fs/cgroup/cgroup.controllers").is_file(),
        "user-systemd" => std::env::var_os("XDG_RUNTIME_DIR").is_some(),
        "bubblewrap" => ["/usr/bin/bwrap", "/bin/bwrap"]
            .iter()
            .any(|p| Path::new(p).is_file()),
        "perf" => ["/usr/bin/perf", "/bin/perf"]
            .iter()
            .any(|p| Path::new(p).is_file()),
        "bpftool" => ["/usr/sbin/bpftool", "/usr/bin/bpftool", "/bin/bpftool"]
            .iter()
            .any(|p| Path::new(p).is_file()),
        "kernel-btf" => Path::new("/sys/kernel/btf/vmlinux").is_file(),
        "kernel-pmu" => Path::new("/sys/bus/event_source/devices/cpu").is_dir(),
        _ => false,
    }
}

pub fn registry_json() -> serde_json::Value {
    json!({"schema_version": VERSION, "command": "dependency-registry", "dependencies": DEPENDENCIES})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn initial_ids_are_complete() {
        for id in [
            "aider",
            "codex",
            "gemini",
            "goose",
            "mini-swe",
            "opencode",
            "opendesk",
            "openhands",
            "openjiuwen",
            "qwen-code",
            "cli2key",
            "perf",
            "bpftool",
        ] {
            assert!(DEPENDENCIES.iter().any(|d| d.id == id));
        }
    }
    #[test]
    fn host_dependencies_are_not_tools() {
        assert_eq!(
            DEPENDENCIES.iter().find(|d| d.id == "linux").unwrap().kind,
            DependencyKind::Host
        );
        assert!(
            !DEPENDENCIES
                .iter()
                .filter(|d| d.kind == DependencyKind::Host)
                .any(|d| d.remediation.starts_with("asb tool install"))
        );
        assert_eq!(
            DEPENDENCIES.iter().find(|d| d.id == "perf").unwrap().kind,
            DependencyKind::Host
        );
        assert_eq!(
            DEPENDENCIES
                .iter()
                .find(|d| d.id == "bpftool")
                .unwrap()
                .kind,
            DependencyKind::Host
        );
    }
    #[test]
    fn tool_remediation_is_exactly_installable() {
        for dependency in DEPENDENCIES
            .iter()
            .filter(|d| d.kind == DependencyKind::Tool && !d.optional)
        {
            assert!(dependency.remediation.starts_with("asb tool install "));
            assert!(dependency.remediation.contains(dependency.id));
        }
    }
    #[test]
    fn preflight_is_bounded_and_side_effect_free() {
        let result = preflight(Path::new("/nonexistent")).unwrap();
        assert_eq!(result.schema_version, VERSION);
        assert!(!result.ready);
    }

    #[test]
    fn unknown_scope_is_rejected_and_known_scope_is_filtered() {
        assert!(preflight_for("not-a-command", Path::new("/nonexistent")).is_err());
        let result = preflight_for("run", Path::new("/nonexistent")).unwrap();
        for id in ["linux", "cgroup-v2", "user-systemd", "bubblewrap"] {
            assert!(result.dependencies.iter().any(|d| d.id == id));
        }
        assert!(result.dependencies.iter().any(|d| d.id == "aider"));
    }

    #[test]
    fn live_execution_scopes_declare_all_host_runtime_capabilities() {
        for command in ["run", "sweep", "benchmark-live"] {
            let (_, ids) = DEPENDENCY_SCOPES
                .iter()
                .find(|(name, _)| *name == command)
                .expect("live execution scope is declared");
            for id in ["linux", "cgroup-v2", "user-systemd", "bubblewrap"] {
                assert!(ids.contains(&id), "{command} omitted host dependency {id}");
            }
            let view = preflight_for_selected(
                command,
                Path::new("/nonexistent"),
                Some("codex"),
                Some("host"),
            )
            .expect("host live scope remains evaluable");
            for id in ["linux", "cgroup-v2", "user-systemd", "bubblewrap"] {
                assert!(view.dependencies.iter().any(|d| d.id == id));
            }
        }
    }

    #[test]
    fn selected_agent_and_runtime_do_not_inherit_unselected_scope() {
        let result = preflight_for_selected(
            "run",
            Path::new("/nonexistent"),
            Some("codex"),
            Some("local"),
        )
        .unwrap();
        assert_eq!(result.dependencies.len(), 1);
        assert_eq!(result.dependencies[0].id, "codex");
        let host = preflight_for_selected(
            "run",
            Path::new("/nonexistent"),
            Some("codex"),
            Some("host"),
        )
        .unwrap();
        assert!(host.dependencies.iter().any(|d| d.id == "codex"));
        assert!(
            host.dependencies
                .iter()
                .all(|d| d.id == "codex" || d.kind == DependencyKind::Host)
        );
        assert!(
            preflight_for_selected("run", Path::new("/nonexistent"), Some("aider"), None,).is_ok()
        );
        assert!(
            preflight_for_selected("run", Path::new("/nonexistent"), Some("not-an-agent"), None,)
                .is_err()
        );
    }

    fn project_with_agent(status: asb_config::ProjectToolStatus) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "asb-dependency-registry-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".asb")).unwrap();
        let mut config = asb_config::ProjectConfigV1::empty();
        config.agents.insert(
            "codex".into(),
            asb_config::ProjectToolRecordV1 {
                kind: asb_config::ProjectToolKind::Agent,
                source_ref: "fixture://codex".into(),
                version: "1.0.0".into(),
                platform: "linux-x86_64".into(),
                path: None,
                digest_sha256: None,
                capabilities: Vec::new(),
                status,
            },
        );
        std::fs::write(
            root.join(".asb/project.json"),
            serde_json::to_vec(&config).unwrap(),
        )
        .unwrap();
        root
    }

    #[test]
    fn project_inventory_and_selector_validation_are_enforced() {
        let root = project_with_agent(asb_config::ProjectToolStatus::Available);
        let local = preflight_for_selected("run", &root, Some("codex"), Some("local")).unwrap();
        assert!(local.ready);
        assert_eq!(local.dependencies.len(), 1);
        assert!(local.dependencies[0].available);

        let missing = preflight_for_selected("preflight", &root, None, Some("host")).unwrap();
        assert!(
            missing
                .dependencies
                .iter()
                .any(|check| check.kind == DependencyKind::Host)
        );
        assert!(preflight_for_selected("run", &root, Some("codex"), Some("remote")).is_err());

        let encoded = registry_json();
        assert_eq!(encoded["schema_version"], VERSION);
        assert!(encoded["dependencies"].as_array().unwrap().len() >= DEPENDENCIES.len());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn invalid_project_inventory_fails_closed_without_side_effects() {
        let root = std::env::temp_dir().join(format!(
            "asb-dependency-registry-invalid-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".asb")).unwrap();
        std::fs::write(root.join(".asb/project.json"), br#"{"unknown":true}"#).unwrap();
        assert!(preflight_for_selected("run", &root, None, None).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn unavailable_project_inventory_is_not_ready_and_filters_runtime_scope() {
        let root = project_with_agent(asb_config::ProjectToolStatus::Missing);
        let local = preflight_for_selected("run", &root, Some("codex"), Some("local"))
            .expect("selected local scope should remain evaluable");
        assert!(!local.ready);
        assert_eq!(local.dependencies.len(), 1);
        assert!(!local.dependencies[0].available);

        let host = preflight_for_selected("preflight", &root, None, Some("host"))
            .expect("host scope should remain evaluable");
        assert!(
            host.dependencies
                .iter()
                .any(|check| check.kind == DependencyKind::Host)
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn inventory_lookup_accepts_non_agent_project_records() {
        let root = std::env::temp_dir().join(format!(
            "asb-dependency-registry-records-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".asb")).unwrap();
        let mut config = asb_config::ProjectConfigV1::empty();
        let record = asb_config::ProjectToolRecordV1 {
            kind: asb_config::ProjectToolKind::SupportTool,
            source_ref: "fixture://codex".into(),
            version: "1.0.0".into(),
            platform: "linux-x86_64".into(),
            path: None,
            digest_sha256: None,
            capabilities: Vec::new(),
            status: asb_config::ProjectToolStatus::Available,
        };
        config.support_tools.insert("codex".into(), record);
        std::fs::write(
            root.join(".asb/project.json"),
            serde_json::to_vec(&config).unwrap(),
        )
        .unwrap();
        let result = preflight_for_selected("run", &root, Some("codex"), Some("local"))
            .expect("support-tool inventory should be accepted");
        assert!(result.ready);
        assert!(result.dependencies[0].available);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn inventory_lookup_covers_each_project_record_namespace() {
        for (index, namespace) in ["harnesses", "benchmarks", "workloads"]
            .into_iter()
            .enumerate()
        {
            let root = std::env::temp_dir().join(format!(
                "asb-dependency-registry-namespace-{index}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(root.join(".asb")).unwrap();
            let mut config = asb_config::ProjectConfigV1::empty();
            let record = asb_config::ProjectToolRecordV1 {
                kind: match namespace {
                    "harnesses" => asb_config::ProjectToolKind::Harness,
                    "benchmarks" => asb_config::ProjectToolKind::Benchmark,
                    "workloads" => asb_config::ProjectToolKind::Workload,
                    _ => unreachable!(),
                },
                source_ref: "fixture://codex".into(),
                version: "1.0.0".into(),
                platform: "linux-x86_64".into(),
                path: None,
                digest_sha256: None,
                capabilities: Vec::new(),
                status: asb_config::ProjectToolStatus::Available,
            };
            match namespace {
                "harnesses" => {
                    config.harnesses.insert("codex".into(), record);
                }
                "benchmarks" => {
                    config.benchmarks.insert("codex".into(), record);
                }
                "workloads" => {
                    config.workloads.insert("codex".into(), record);
                }
                _ => unreachable!(),
            }
            std::fs::write(
                root.join(".asb/project.json"),
                serde_json::to_vec(&config).unwrap(),
            )
            .unwrap();
            let result = preflight_for_selected("run", &root, Some("codex"), Some("local"))
                .expect("project namespace should be evaluable");
            assert!(result.ready);
            assert!(result.dependencies[0].available);
            let _ = std::fs::remove_dir_all(root);
        }
    }

    #[test]
    fn preflight_evaluates_every_declared_host_capability() {
        let result = preflight_for("preflight", Path::new("/nonexistent"))
            .expect("preflight host capability evaluation should be bounded");
        for id in [
            "linux",
            "cgroup-v2",
            "user-systemd",
            "bubblewrap",
            "perf",
            "bpftool",
            "kernel-btf",
            "kernel-pmu",
        ] {
            assert!(result.dependencies.iter().any(|check| check.id == id));
        }
        assert!(result.dependencies.iter().any(|check| check.optional));
    }
}
