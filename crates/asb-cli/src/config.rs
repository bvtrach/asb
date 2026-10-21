//! The setup command and its small file format.

use super::{Error, io_error};
use crate::agents;
use crate::benchmark::{BenchmarkConfig, MeasurementConfig};
use crate::measurements;
use crate::project;
use crate::providers;
use crate::suggestions;
use std::io::Write;

pub struct SetupOptions {
    pub agent: Option<String>,
    pub workload: Option<String>,
    pub trial: Option<String>,
    pub trial_set: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub concurrency: Option<usize>,
    pub workspace: Option<String>,
    pub output: Option<String>,
}

/// Write a TOML benchmark configuration from parsed setup options.
pub fn setup(options: SetupOptions, output: &mut dyn Write) -> Result<(), Error> {
    let SetupOptions {
        agent,
        workload,
        trial,
        trial_set,
        provider,
        model,
        concurrency,
        workspace,
        output: destination,
    } = options;
    let (root, project_config) = project::load(std::path::Path::new("."))?;
    let workload_directory = std::env::var_os("ASB_WORKLOAD_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| project::resolve(&root, &project_config.workloads_dir));
    let agent = required(agent)?;
    let known_agents = agents::all()
        .iter()
        .map(|adapter| adapter.id())
        .collect::<Vec<_>>();
    if let Some(candidate) = suggestions::closest(&agent, &known_agents) {
        return Err(Error::Input(format!(
            "unknown agent `{agent}`; did you mean `{candidate}`?"
        )));
    }
    let provider = required(provider)?;
    if let Some(candidate) = suggestions::closest(&provider, providers::names()) {
        return Err(Error::Input(format!(
            "unknown provider `{provider}`; did you mean `{candidate}`?"
        )));
    }
    let model = required(model)?;
    let concurrency = concurrency.unwrap_or(1);
    if concurrency == 0 {
        return Err(Error::Input("concurrency must be at least 1".into()));
    }
    if concurrency > crate::benchmark::MAX_CONCURRENCY {
        return Err(Error::Input(format!(
            "concurrency must not exceed {}; split larger selections into bounded sweeps",
            crate::benchmark::MAX_CONCURRENCY
        )));
    }
    let config = BenchmarkConfig {
        agent,
        workload: required(workload)?,
        provider,
        model,
        concurrency,
        trial_set,
        trial,
        workspace: workspace.unwrap_or(project_config.workspace_dir),
        log_file: destination
            .as_deref()
            .map(default_log_name)
            .unwrap_or_else(|| "benchmark.log".into()),
        measurements: Some(MeasurementConfig {
            enabled: true,
            directory: "measurements".into(),
            collectors: measurements::available_collectors(),
        }),
    };
    let catalog = crate::workloads::planning_catalog(&config.workload, &workload_directory)
        .map_err(Error::Input)?;
    if config.trial_set.is_some() && config.trial.is_none() {
        return Err(Error::Input(
            "--trial-set requires --trial; use `asb setup tui` to select multiple instances".into(),
        ));
    }
    if catalog.is_some() && config.trial.is_none() {
        return Err(Error::Input(format!(
            "workload `{}` requires --trial INSTANCE; choose an installed instance with `asb workload tui` or use `asb setup tui`",
            config.workload
        )));
    }
    if catalog.is_none() && config.trial.is_some() && config.trial_set.is_none() {
        return Err(Error::Input("--trial requires --trial-set for a workload without an installed instance catalog; select trials with `asb workload tui` first".into()));
    }
    crate::workloads::describe_selection(&config, &workload_directory, catalog.as_ref())
        .map_err(Error::Input)?;
    let text = toml::to_string_pretty(&config)
        .map_err(|error| Error::Input(format!("cannot render configuration: {error}")))?;
    if let Some(path) = destination {
        std::fs::write(&path, text.as_bytes())
            .map_err(|error| Error::Io(format!("cannot write {path}: {error}")))?;
        writeln!(output, "Wrote {path}").map_err(io_error)?;
        writeln!(output, "{}", run_command_message(&path)).map_err(io_error)?;
    } else {
        output.write_all(text.as_bytes()).map_err(io_error)?;
    }
    Ok(())
}

fn default_log_name(destination: &str) -> String {
    std::path::Path::new(destination)
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map(|value| format!("{value}.log"))
        .unwrap_or_else(|| "benchmark.log".into())
}

fn run_command_message(path: &str) -> String {
    format!("Run it with: asb run {path}")
}

fn required(value: Option<String>) -> Result<String, Error> {
    let value = value.ok_or(Error::Usage(
        "setup requires agent, workload, provider, and model",
    ))?;
    if value.trim().is_empty() {
        return Err(Error::Input("setup values cannot be empty".into()));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::{SetupOptions, default_log_name, run_command_message, setup};

    #[test]
    fn setup_derives_log_name_from_configuration_stem() {
        assert_eq!(default_log_name("configs/foo.toml"), "foo.log");
        assert_eq!(default_log_name("benchmark"), "benchmark.log");
    }

    #[test]
    fn setup_output_explains_how_to_run_written_configuration() {
        assert_eq!(
            run_command_message("configs/foo.toml"),
            "Run it with: asb run configs/foo.toml"
        );
    }

    #[test]
    fn setup_writes_default_concurrency() {
        let mut output = Vec::new();
        setup(options("pi", "cli2key", "gpt-6.1-sol"), &mut output).expect("setup succeeds");
        let text = String::from_utf8(output).expect("setup is UTF-8");
        assert!(text.contains("concurrency = 1"));
    }

    #[test]
    fn setup_rejects_unbounded_concurrency_before_writing_configuration() {
        let mut selection = options("pi", "cli2key", "fixture");
        selection.concurrency = Some(usize::MAX);
        let mut output = Vec::new();
        assert!(
            setup(selection, &mut output)
                .unwrap_err()
                .to_string()
                .contains("must not exceed 64")
        );
        assert!(output.is_empty());
    }

    fn options(agent: &str, provider: &str, model: &str) -> SetupOptions {
        SetupOptions {
            agent: Some(agent.into()),
            workload: Some("bug-fix".into()),
            trial: None,
            trial_set: None,
            provider: Some(provider.into()),
            model: Some(model.into()),
            concurrency: None,
            workspace: None,
            output: None,
        }
    }

    #[test]
    fn setup_suggests_close_agent_and_provider_names() {
        let agent_error = setup(options("opencod", "local", "test"), &mut Vec::new())
            .expect_err("agent typo rejected");
        assert!(agent_error.to_string().contains("did you mean `opencode`?"));
        let provider_error = setup(options("opencode", "openruter", "test"), &mut Vec::new())
            .expect_err("provider typo rejected");
        assert!(
            provider_error
                .to_string()
                .contains("did you mean `openrouter`?")
        );
        setup(
            options("opencode", "cli2key", "new-live-model"),
            &mut Vec::new(),
        )
        .expect("setup must not reject models absent from an old snapshot");
    }
}
