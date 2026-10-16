//! The setup command and its small file format.

use super::{Error, io_error};
use crate::benchmark::{BenchmarkConfig, MeasurementConfig};
use crate::measurements;
use crate::project;
use crate::providers;
use crate::suggestions;
use std::io::Write;

pub struct SetupOptions {
    pub agent: Option<String>,
    pub workload: Option<String>,
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
        provider,
        model,
        concurrency,
        workspace,
        output: destination,
    } = options;
    let (_, project_config) = project::load(std::path::Path::new("."))?;
    let agent = required(agent)?;
    if let Some(candidate) = suggestions::closest(
        &agent,
        &[
            "codex",
            "pi",
            "opencode",
            "workbuddy",
            "qwen-code",
            "mini-swe",
            "openhands",
            "openjiuwen",
            "claude-code",
            "goose",
            "cline",
            "continue",
        ],
    ) {
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
    if let Some(candidates) = providers::known_models(&provider) {
        if !candidates.iter().any(|candidate| *candidate == model) {
            if let Some(candidate) = suggestions::closest(&model, candidates) {
                return Err(Error::Input(format!(
                    "model `{model}` for provider `{provider}` is not tool-capable; did you mean `{candidate}`?"
                )));
            }
            return Err(Error::Input(format!(
                "model `{model}` for provider `{provider}` is not in the tool-capable catalog; run `asb provider models {provider}`"
            )));
        }
    }
    let concurrency = concurrency.unwrap_or(1);
    if concurrency == 0 {
        return Err(Error::Input("concurrency must be at least 1".into()));
    }
    let config = BenchmarkConfig {
        agent,
        workload: required(workload)?,
        provider,
        model,
        concurrency,
        trial_set: None,
        trial: None,
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

    fn options(agent: &str, provider: &str, model: &str) -> SetupOptions {
        SetupOptions {
            agent: Some(agent.into()),
            workload: Some("bug-fix".into()),
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
        let model_error = setup(
            options("opencode", "cli2key", "gpt-6.1-sl"),
            &mut Vec::new(),
        )
        .expect_err("model typo rejected");
        assert!(
            model_error
                .to_string()
                .contains("did you mean `gpt-6.1-sol`?")
        );
    }
}
