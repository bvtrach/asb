//! Immutable engine routing, not a benchmark identity or evaluation policy.
use crate::Error;
use crate::workloads::{PreflightContext, recovery, remote};
use std::path::Path;

#[derive(Clone, Copy)]
pub(super) struct EngineBinding {
    tool: &'static str,
    module: &'static str,
    publish_helpers: fn(&Path) -> Result<(), Error>,
}

impl EngineBinding {
    pub(super) const fn new(
        tool: &'static str,
        module: &'static str,
        publish_helpers: fn(&Path) -> Result<(), Error>,
    ) -> Self {
        Self {
            tool,
            module,
            publish_helpers,
        }
    }

    pub(super) const fn tool(self) -> &'static str {
        self.tool
    }

    pub(super) fn script(self) -> String {
        format!("{}.py", self.module)
    }

    pub(super) fn publish(self, root: &Path) -> Result<(), Error> {
        (self.publish_helpers)(root)
    }

    /// The owner must first validate its installed catalog and selected dataset.
    pub(super) fn grade_retained(
        self,
        context: &recovery::RecoveryContext<'_>,
        validated_catalog: &Path,
    ) -> Result<serde_json::Value, Error> {
        let runtime = remote::InspectionRuntime::new(context.project_root)?;
        self.publish(&runtime.0)?;
        remote::grade_retained(
            self.tool,
            self.module,
            context,
            &runtime.0,
            validated_catalog,
        )
    }

    pub(super) fn inspect_grade(
        self,
        workload: &str,
        context: &recovery::RecoveryContext<'_>,
    ) -> Result<serde_json::Value, Error> {
        context.reference.validate_location(context.project_root)?;
        let tool = crate::project::resolve(context.project_root, &context.project_config.tools_dir)
            .join(self.tool);
        remote::require_sdk(&tool, workload)?;
        let runtime = remote::InspectionRuntime::new(context.project_root)?;
        self.publish(&runtime.0)?;
        remote::inspect_grade(self.tool, self.module, context, &runtime.0)
    }

    pub(super) fn verify_retained(
        self,
        context: &recovery::RetainedOutcomeContext<'_>,
    ) -> Result<(), Error> {
        remote::verify_retained(self.tool, self.module, context)
    }

    pub(super) fn recover(
        self,
        context: &recovery::RecoveryContext<'_>,
    ) -> Result<serde_json::Value, Error> {
        remote::recover(self.tool, context)
    }

    pub(super) fn preflight(
        self,
        workload: &str,
        context: &PreflightContext<'_>,
    ) -> Result<(), Error> {
        let tool = context.tool_root(self.tool);
        remote::require_sdk(&tool, workload)?;
        self.publish(&tool)?;
        remote::prefetch(context)
    }

    pub(super) fn runtime_inputs(
        self,
        context: &crate::runtime_inputs::Context<'_>,
    ) -> Result<Vec<crate::runtime_inputs::Dependency>, Error> {
        remote::runtime_inputs(self.tool, context)
    }
}

#[cfg(test)]
mod tests;
