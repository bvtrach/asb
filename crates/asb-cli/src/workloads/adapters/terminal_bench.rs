use super::StaticExternalAdapter;
mod catalog;

#[cfg(test)]
#[path = "terminal_bench/adapter_tests.rs"]
mod adapter_tests;

pub(super) struct TerminalBenchAdapter {
    pub id: &'static str,
    pub source: &'static str,
    pub revision: &'static str,
    pub split: &'static str,
    pub archive: &'static str,
    pub tool: &'static str,
    pub version: &'static str,
    pub expected_tasks: usize,
    pub runtime: &'static str,
}

impl super::WorkloadAdapter for TerminalBenchAdapter {
    fn process_lifetime(&self) -> crate::workloads::session::ProcessLifetime {
        crate::workloads::session::ProcessLifetime::ThroughEvaluation
    }
    fn grade_retained(
        &self,
        context: &crate::workloads::recovery::RecoveryContext<'_>,
    ) -> Result<serde_json::Value, crate::Error> {
        let workloads =
            crate::project::resolve(context.project_root, &context.project_config.workloads_dir);
        let installed = catalog::read(self, &workloads).map_err(crate::Error::Input)?;
        if installed.dataset != context.reference.dataset {
            return Err(crate::Error::Input(
                "retained task dataset differs from its installed grading catalog".into(),
            ));
        }
        let runtime = crate::workloads::remote::InspectionRuntime::new(context.project_root)?;
        catalog::ensure_helpers(&runtime.0)?;
        crate::workloads::remote::grade_retained(
            self.tool,
            self.runtime.trim_end_matches(".py"),
            context,
            &runtime.0,
            &workloads.join(self.id).join("catalog.json"),
        )
    }
    fn inspect_grade(
        &self,
        context: &crate::workloads::recovery::RecoveryContext<'_>,
    ) -> Result<serde_json::Value, crate::Error> {
        let tool = crate::project::resolve(context.project_root, &context.project_config.tools_dir)
            .join(self.tool);
        context.reference.validate_location(context.project_root)?;
        crate::workloads::remote::require_sdk(&tool, self.id)?;
        let runtime = crate::workloads::remote::InspectionRuntime::new(context.project_root)?;
        catalog::ensure_helpers(&runtime.0)?;
        crate::workloads::remote::inspect_grade(
            self.tool,
            self.runtime.trim_end_matches(".py"),
            context,
            &runtime.0,
        )
    }
    fn verify_retained(
        &self,
        context: &crate::workloads::recovery::RetainedOutcomeContext<'_>,
    ) -> Result<(), crate::Error> {
        crate::workloads::remote::verify_retained(
            self.tool,
            self.runtime.trim_end_matches(".py"),
            context,
        )
    }
    fn recover(
        &self,
        context: &crate::workloads::recovery::RecoveryContext<'_>,
    ) -> Result<serde_json::Value, crate::Error> {
        crate::workloads::remote::recover(self.tool, context)
    }
    fn id(&self) -> &'static str {
        self.id
    }
    fn preflight(
        &self,
        context: &crate::workloads::PreflightContext<'_>,
    ) -> Result<(), crate::Error> {
        crate::workloads::remote::require_sdk(&context.tool_root(self.tool), self.id)?;
        catalog::ensure_helpers(&context.tool_root(self.tool))?;
        crate::workloads::remote::prefetch(context)
    }
    fn runtime_inputs(
        &self,
        context: &crate::runtime_inputs::Context<'_>,
    ) -> Result<Vec<crate::runtime_inputs::Dependency>, crate::Error> {
        crate::workloads::remote::runtime_inputs(self.tool, context)
    }
    fn source(&self) -> Option<&'static str> {
        Some(self.archive)
    }
    fn trial_ids(&self) -> Vec<crate::workloads::Trial> {
        Vec::new()
    }
    fn planning_catalog(
        &self,
        directory: &std::path::Path,
    ) -> Result<Option<crate::workloads::PlanningCatalog>, String> {
        let catalog = catalog::read(self, directory)?;
        Ok(Some(crate::workloads::PlanningCatalog {
            dataset: catalog.dataset,
            task_ids: catalog.tasks.into_iter().map(|task| task.id).collect(),
            evaluation_kind: "upstream-harness",
        }))
    }
    fn installed_available(&self, directory: &std::path::Path) -> Option<bool> {
        Some(catalog::read(self, directory).is_ok())
    }
    fn installed_trials(
        &self,
        directory: &std::path::Path,
    ) -> Result<Vec<crate::workloads::Trial>, String> {
        Ok(catalog::read(self, directory)?
            .tasks
            .into_iter()
            .map(|task| crate::workloads::Trial {
                id: task.id,
                description: task.description,
            })
            .collect())
    }
    fn install(
        &self,
        context: &crate::workloads::InstallationContext<'_>,
        output: &mut dyn std::io::Write,
        progress: &mut dyn FnMut(u8, &str),
    ) -> Result<bool, crate::Error> {
        catalog::install(self, context, output, progress)?;
        Ok(true)
    }
    fn prepare_task(
        &self,
        context: &crate::workloads::PreparationContext<'_>,
    ) -> Result<Box<dyn crate::workloads::WorkloadSession>, String> {
        let installed = catalog::read(self, context.workload_directory)?;
        let task = context
            .trial
            .ok_or("select an official installed Terminal-Bench instance before execution")?;
        if !installed.tasks.iter().any(|item| item.id == task) {
            return Err("selected instance is absent from the installed dataset".into());
        }
        let tool = context.tool_root(self.tool);
        catalog::ensure_helpers(&tool).map_err(|error| error.to_string())?;
        if !catalog::runtime_available(self, &tool).map_err(|error| error.to_string())? {
            return Err(format!(
                "installed evaluator runtime is incomplete; run `asb tool install {}` before starting the benchmark (details in .asb/install.log)",
                self.tool
            ));
        }
        crate::workloads::remote::launch(
            self.tool,
            self.runtime,
            None,
            self.id,
            installed.dataset,
            context,
        )
    }
    fn prepare_source(&self, _: &std::path::Path, _: &std::path::Path) -> Result<usize, String> {
        Err("official Terminal-Bench instances need their workload-owned environment, not a source-copy smoke trial".into())
    }
}

/// Legacy reference/smoke alias; never relabel old smoke results as official.
pub(super) static ADAPTER: StaticExternalAdapter = StaticExternalAdapter::new(
    "terminal-bench",
    &[
        (
            "smoke",
            "Short terminal task suitable for a local smoke run.",
        ),
        (
            "shell",
            "Shell and filesystem task from the local Terminal-Bench checkout.",
        ),
        (
            "debug",
            "Debugging task from the local Terminal-Bench checkout.",
        ),
    ],
);

#[cfg(test)]
mod tests {
    use super::super::WorkloadAdapter;
    use serde_json::json;

    #[test]
    fn both_original_terminal_adapters_describe_public_selection_before_execution() {
        for adapter in [
            &super::super::terminal_bench_1_0::ADAPTER,
            &super::super::terminal_bench_2_1::ADAPTER,
        ] {
            let root = std::env::temp_dir()
                .join(crate::workloads::TaskIdentity::local("terminal-planning", None).attempt_id);
            let installed = root.join(adapter.id);
            let tasks = (0..adapter.expected_tasks).map(|index| {
                let id = format!("fixture-{index}");
                std::fs::create_dir_all(installed.join(format!("source/tasks/{id}"))).unwrap();
                json!({"id":id,"description":"selection fixture","instruction":"not exported to the plan","definition":format!("source/tasks/fixture-{index}")})
            }).collect::<Vec<_>>();
            std::fs::write(installed.join("source-manifest.json"), "{}").unwrap();
            std::fs::write(installed.join("catalog.json"), serde_json::to_vec(&json!({
                "schema_version":1,"workload":adapter.id,
                "dataset":{"source":adapter.source,"revision":adapter.revision,"split":adapter.split},
                "sdk":{"package":adapter.tool,"version":adapter.version},
                "source_sha256":"a".repeat(64),"archive_sha256":"b".repeat(64),"tasks":tasks,
            })).unwrap()).unwrap();
            let planning = adapter.planning_catalog(&root).unwrap().unwrap();
            assert_eq!(planning.task_ids.len(), adapter.expected_tasks);
            assert_eq!(planning.dataset.source, adapter.source);
            assert_eq!(planning.dataset.revision, adapter.revision);
            assert_eq!(planning.evaluation_kind, "upstream-harness");
            let config: crate::benchmark::BenchmarkConfig = toml::from_str(&format!("agent='pi'\nprovider='openrouter'\nmodel='fixture'\nworkload='{}'\ntrial='fixture-0'\n", adapter.id)).unwrap();
            let selected =
                crate::workloads::describe_selection(&config, &root, Some(&planning)).unwrap();
            assert!(
                !serde_json::to_string(&selected)
                    .unwrap()
                    .contains("not exported")
            );
            std::fs::remove_dir_all(root).unwrap();
        }
    }
}
