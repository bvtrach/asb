//! Shared official engine for the three original SWE-bench populations.
mod catalog;
use super::WorkloadAdapter;
use crate::workloads::{InstallationContext, Trial};
use std::path::Path;

pub(super) struct SweBenchAdapter {
    pub id: &'static str,
    pub dataset: &'static str,
    pub revision: &'static str,
    pub expected_tasks: usize,
}

pub(super) static ADAPTER: SweBenchAdapter = SweBenchAdapter {
    id: "swe-bench",
    dataset: "SWE-bench/SWE-bench",
    revision: "c6fe717fd7a4c3ac1daa4055a4fd082c6a1d28a2",
    expected_tasks: 2294,
};

impl WorkloadAdapter for SweBenchAdapter {
    fn id(&self) -> &'static str {
        self.id
    }

    fn install(
        &self,
        context: &InstallationContext<'_>,
        output: &mut dyn std::io::Write,
        progress: &mut dyn FnMut(u8, &str),
    ) -> Result<bool, crate::Error> {
        catalog::install(self, context, output, progress)?;
        Ok(true)
    }

    fn installed_available(&self, directory: &Path) -> Option<bool> {
        Some(catalog::read(self, directory).is_ok())
    }

    fn installed_trials(&self, directory: &Path) -> Result<Vec<Trial>, String> {
        Ok(catalog::read(self, directory)?
            .tasks
            .into_iter()
            .map(|task| Trial {
                id: task.id,
                description: task.description,
            })
            .collect())
    }

    fn trial_ids(&self) -> Vec<Trial> {
        Vec::new()
    }

    fn prepare_task(
        &self,
        context: &crate::workloads::PreparationContext<'_>,
    ) -> Result<Box<dyn crate::workloads::WorkloadSession>, String> {
        catalog::read(self, context.workload_directory)?;
        Err(format!(
            "{} requires its official execution session; full benchmark runtime integration is still in development",
            self.id
        ))
    }

    fn prepare_source(&self, _workloads: &Path, _workspace: &Path) -> Result<usize, String> {
        Err(
            "SWE-bench instances require isolated task preparation, not a source archive copy"
                .into(),
        )
    }
}
