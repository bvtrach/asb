use super::StaticExternalAdapter;
mod catalog;

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
    fn id(&self) -> &'static str {
        self.id
    }
    fn source(&self) -> Option<&'static str> {
        Some(self.archive)
    }
    fn trial_ids(&self) -> Vec<crate::workloads::Trial> {
        Vec::new()
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
        let (root, config) =
            crate::project::load(context.workload_directory).map_err(|error| error.to_string())?;
        let tool = crate::project::resolve(&root, &config.tools_dir).join(self.tool);
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
