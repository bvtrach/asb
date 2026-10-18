use super::{CommandContext, CommandSpec, PreparationContext, StaticAdapter};
use crate::{Error, install::Recipe};
pub(super) static ADAPTER: StaticAdapter = StaticAdapter::new(
    "openjiuwen",
    "openjiuwen",
    Recipe::Python {
        package: "openjiuwen[cli]==0.1.19.post2",
        // filelock 4.2.2 stalls while the pinned OpenJiuwen client acquires
        // its filesystem locks. 3.32.7 satisfies its >=3.25 requirement and
        // is verified by the opt-in real-client fixture test.
        dependencies: &["filelock==3.32.7"],
        executable: "openjiuwen",
    },
    &[crate::endpoint::MESSAGES_KEY],
    command,
    prepare_adapter,
)
.with_replay_credential_vars(&["OPENJIUWEN_API_KEY"]);
fn command(context: &CommandContext<'_>) -> CommandSpec {
    CommandSpec {
        program: "openjiuwen".into(),
        args: [
            "-v",
            "--model",
            context.config.model.as_str(),
            "--provider",
            "anthropic",
            "--api-base",
            context.endpoint.unwrap_or_default(),
            "run",
            "-f",
            "json",
            &format!(
                "{}\n\nDo not use todo/planning tools for this run. Execute the requested work directly and return a final response when complete.",
                context.prompt
            ),
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        append_prompt: false,
    }
}

#[cfg(test)]
mod tests {
    use super::command;
    use crate::agents::CommandContext;
    use crate::benchmark::BenchmarkConfig;

    #[test]
    fn installer_pins_a_tested_compatible_filelock() {
        let crate::install::Recipe::Python { dependencies, .. } = super::ADAPTER.recipe else {
            panic!("OpenJiuwen must use a Python recipe");
        };
        assert!(dependencies.contains(&"filelock==3.32.7"));
    }

    #[test]
    fn enables_verbose_mode_for_deterministic_noninteractive_runs() {
        let config = BenchmarkConfig {
            agent: "openjiuwen".into(),
            workload: "bug-fix".into(),
            provider: "cli2key".into(),
            model: "test-model".into(),
            concurrency: 1,
            trial_set: None,
            trial: None,
            workspace: "workspace".into(),
            log_file: "run.log".into(),
            measurements: None,
        };
        let spec = command(&CommandContext {
            config: &config,
            endpoint: Some("http://127.0.0.1:3000/v1"),
            interface: Some(crate::endpoint::Interface::AnthropicMessages),
            prompt: "repair",
        });
        assert_eq!(spec.args.first().map(String::as_str), Some("-v"));
        assert!(!spec.args.iter().any(|arg| arg == "-w"));
    }
}

fn prepare_adapter(context: &mut PreparationContext<'_>) -> Result<(), Error> {
    let connection = context.connection;
    context.command.env(
        "OPENJIUWEN_API_KEY",
        connection.credential.as_deref().unwrap_or("asb-local"),
    );
    Ok(())
}
