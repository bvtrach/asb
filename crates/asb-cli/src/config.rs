//! The setup command and its small file format.

use super::{Error, io_error};
use crate::benchmark::BenchmarkConfig;
use crate::project;
use std::io::Write;

/// Parse setup flags and write a TOML benchmark configuration.
pub fn setup(args: &[String], output: &mut dyn Write) -> Result<(), Error> {
    let mut agent = None;
    let mut workload = None;
    let mut provider = None;
    let mut model = None;
    let mut workspace = None;
    let mut destination = None;
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        if let Some(name) = flag.strip_prefix('-').filter(|name| {
            !name.starts_with('-')
                && matches!(
                    *name,
                    "agent" | "workload" | "provider" | "model" | "workspace" | "output"
                )
        }) {
            return Err(Error::Input(format!(
                "setup option `{flag}` is invalid; use `--{name}` with two dashes"
            )));
        }
        let value = args
            .get(index + 1)
            .ok_or(Error::Usage("setup options require values"))?;
        match flag {
            "--agent" => agent = Some(value.clone()),
            "--workload" => workload = Some(value.clone()),
            "--provider" => provider = Some(value.clone()),
            "--model" => model = Some(value.clone()),
            "--workspace" => workspace = Some(value.clone()),
            "--output" => destination = Some(value.clone()),
            _ => {
                return Err(Error::Usage(
                    "setup accepts --agent, --workload, --provider, --model, --workspace, and --output",
                ));
            }
        }
        index += 2;
    }
    let (_, project_config) = project::load(std::path::Path::new("."))?;
    let config = BenchmarkConfig {
        agent: required(agent)?,
        workload: required(workload)?,
        provider: required(provider)?,
        model: required(model)?,
        workspace: workspace.unwrap_or(project_config.workspace_dir),
    };
    let text = toml::to_string_pretty(&config)
        .map_err(|error| Error::Input(format!("cannot render configuration: {error}")))?;
    if let Some(path) = destination {
        std::fs::write(&path, text.as_bytes())
            .map_err(|error| Error::Io(format!("cannot write {path}: {error}")))?;
        writeln!(output, "Wrote {path}").map_err(io_error)?;
    } else {
        output.write_all(text.as_bytes()).map_err(io_error)?;
    }
    Ok(())
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
    use super::setup;

    #[test]
    fn explains_single_dash_long_option_typo() {
        let error = setup(
            &[
                "--agent".into(),
                "opencode".into(),
                "-workload".into(),
                "bug-fix".into(),
            ],
            &mut Vec::new(),
        )
        .expect_err("single-dash option must be rejected");
        assert_eq!(
            error.to_string(),
            "setup option `-workload` is invalid; use `--workload` with two dashes"
        );
    }
}
