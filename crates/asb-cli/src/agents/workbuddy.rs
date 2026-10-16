use super::{CommandContext, CommandSpec, PreparationContext, StaticAdapter};
use crate::{Error, install::Recipe};
pub(super) static ADAPTER: StaticAdapter = StaticAdapter::new(
    "workbuddy",
    "codebuddy",
    Recipe::Npm {
        package: "@tencent-ai/codebuddy-code@2.167.0",
        executable: "codebuddy",
    },
    command,
    prepare_adapter,
);
fn command(context: &CommandContext<'_>) -> CommandSpec {
    CommandSpec {
        program: "codebuddy".into(),
        args: vec![
            "--print".into(),
            "--output-format".into(),
            "json".into(),
            "--model".into(),
            context.config.model.clone(),
            "-y".into(),
        ],
        append_prompt: true,
    }
}
fn prepare_adapter(context: &mut PreparationContext<'_>) -> Result<(), Error> {
    if !context.provider.eq_ignore_ascii_case("cli2key") {
        return Ok(());
    }
    let (key, base_url) = context
        .cli2key
        .ok_or_else(|| Error::Run("cli2key provider has no API endpoint".into()))?;
    let directory = context.tools_dir.join("workbuddy/config");
    configure(&directory, context.model, base_url)?;
    context
        .command
        .env("CODEBUDDY_API_KEY", key)
        .env("CODEBUDDY_AUTH_TOKEN", key)
        .env("CODEBUDDY_BASE_URL", base_url)
        .env("CODEBUDDY_MODEL", context.model)
        .env("CODEBUDDY_CONFIG_DIR", &directory)
        .env("CODEBUDDY_API_KEY_HELPER_DISABLED", "1");
    Ok(())
}

fn configure(directory: &std::path::Path, model: &str, base_url: &str) -> Result<(), Error> {
    std::fs::create_dir_all(directory).map_err(|error| {
        Error::Run(format!(
            "cannot create WorkBuddy provider configuration directory: {error}"
        ))
    })?;
    let configuration = serde_json::json!([{
        "id": model, "name": model, "url": base_url, "api": "openai-responses"
    }]);
    let content = serde_json::to_vec_pretty(&configuration).map_err(|error| {
        Error::Run(format!(
            "cannot render WorkBuddy provider configuration: {error}"
        ))
    })?;
    std::fs::write(directory.join("models.json"), content).map_err(|error| {
        Error::Run(format!(
            "cannot write WorkBuddy provider configuration: {error}"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_configuration_is_owned_by_the_workbuddy_adapter() {
        let directory =
            std::env::temp_dir().join(format!("asb-workbuddy-config-{}", std::process::id()));
        configure(&directory, "test-model", "http://127.0.0.1:3456/v1")
            .expect("configuration succeeds");
        let text =
            std::fs::read_to_string(directory.join("models.json")).expect("configuration exists");
        assert!(text.contains("test-model"));
        let _ = std::fs::remove_dir_all(directory);
    }
}
