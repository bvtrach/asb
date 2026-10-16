//! Pre-run environment preparation hooks.

use crate::Error;
use crate::providers;
use std::path::Path;
use std::process::Command;

/// Apply preparation with an optional local tape endpoint.
///
/// A tape endpoint must be passed explicitly because Pi resolves providers from
/// its project-local `models.json`; generic OpenAI environment variables do not
/// override Pi's provider base URL. Replay also deliberately uses a sentinel
/// credential so it never needs a live provider key.
pub fn apply_with_tape(
    command: &mut Command,
    agent: &str,
    provider: &str,
    model: &str,
    codex_executable: Option<&str>,
    cli2key: Option<(&str, &str)>,
    pi_config_dir: Option<&Path>,
    workbuddy_config_dir: Option<&Path>,
    continue_config_dir: Option<&Path>,
    tape_base_url: Option<&str>,
    replay: bool,
) -> Result<(), Error> {
    if provider.eq_ignore_ascii_case("cli2key") {
        prepare_cli2key(
            command,
            agent,
            model,
            codex_executable,
            cli2key,
            pi_config_dir,
            workbuddy_config_dir,
            continue_config_dir,
        )?;
    } else if provider.eq_ignore_ascii_case("openrouter") {
        prepare_openrouter(
            command,
            agent,
            model,
            pi_config_dir,
            continue_config_dir,
            tape_base_url,
            replay,
        )?;
    }
    Ok(())
}

fn prepare_openrouter(
    command: &mut Command,
    agent: &str,
    model: &str,
    pi_config_dir: Option<&Path>,
    continue_config_dir: Option<&Path>,
    tape_base_url: Option<&str>,
    replay: bool,
) -> Result<(), Error> {
    let key = if replay {
        "asb-replay".to_owned()
    } else {
        providers::credential("openrouter").ok_or_else(|| {
            Error::Run(
                "openrouter provider requires OPENROUTER_API_KEY or ~/.api_key_openrouter; set one before a live or record run".into(),
            )
        })?
    };
    let base_url = tape_base_url
        .map(str::to_owned)
        .or_else(|| std::env::var("OPENROUTER_BASE_URL").ok())
        .unwrap_or_else(|| "https://openrouter.ai/api/v1".into());
    configure_openai_compatible(command, agent, model, &key, &base_url, continue_config_dir)?;
    command.env("OPENROUTER_API_KEY", &key);
    if agent.eq_ignore_ascii_case("pi") {
        let directory = pi_config_dir.ok_or_else(|| {
            Error::Run(
                "cannot configure Pi OpenRouter provider without a project-local Pi config directory".into(),
            )
        })?;
        configure_pi_provider(
            command,
            directory,
            "openrouter",
            model,
            &base_url,
            "openai-completions",
            "OPENROUTER_API_KEY",
        )?;
    }
    Ok(())
}

fn configure_openai_compatible(
    command: &mut Command,
    agent: &str,
    model: &str,
    key: &str,
    base_url: &str,
    continue_config_dir: Option<&Path>,
) -> Result<(), Error> {
    let root = base_url.trim_end_matches('/');
    let api_root = root.strip_suffix("/v1").unwrap_or(root);
    command
        .env("OPENAI_API_KEY", key)
        .env("OPENAI_BASE_URL", root)
        .env("OPENAI_API_BASE", root)
        .env("OPENAI_MODEL", model)
        .env("ANTHROPIC_API_KEY", key)
        .env("ANTHROPIC_BASE_URL", api_root)
        .env("GOOSE_PROVIDER", "openai")
        .env("GOOSE_MODEL", model)
        .env("OPENAI_HOST", api_root)
        .env("OPENAI_BASE_PATH", "/v1/chat/completions")
        .env("CLINE_API_KEY", key)
        .env("CONTINUE_API_KEY", key)
        .env("CONTINUE_API_BASE", root)
        .env("CONTINUE_MODEL", model);
    if agent.eq_ignore_ascii_case("continue") {
        let directory = continue_config_dir.ok_or_else(|| {
            Error::Run(
                "cannot configure Continue without a project-local configuration directory".into(),
            )
        })?;
        configure_continue(directory, model, root)?;
        command
            .env("CONTINUE_CONFIG", directory.join("config.yaml"))
            .env("CONTINUE_GLOBAL_DIR", directory);
    }
    Ok(())
}

fn prepare_cli2key(
    command: &mut Command,
    agent: &str,
    model: &str,
    codex_executable: Option<&str>,
    cli2key: Option<(&str, &str)>,
    pi_config_dir: Option<&Path>,
    workbuddy_config_dir: Option<&Path>,
    continue_config_dir: Option<&Path>,
) -> Result<(), Error> {
    let (key, base_url) = cli2key.ok_or_else(|| {
        Error::Run(
            "cli2key provider has no API endpoint; run `asb tool install cli2key` or set CLI2KEY_BASE_URL and CLI2KEY_API_KEY".into(),
        )
    })?;
    command
        .env("CLI2KEY_API_KEY", key)
        .env("CLI2KEY_BASE_URL", base_url)
        .env("CLI2KEY_MODEL", model)
        .env("OPENAI_API_KEY", key)
        .env("OPENAI_BASE_URL", base_url)
        .env("OPENAI_API_BASE", base_url)
        .env("OPENAI_MODEL", model)
        .env("ANTHROPIC_MODEL", model)
        .env("QWEN_DEFAULT_AUTH_TYPE", "anthropic")
        .env("ANTHROPIC_API_KEY", key)
        .env("ANTHROPIC_BASE_URL", base_url.trim_end_matches("/v1"));
    if agent.eq_ignore_ascii_case("opencode") {
        command.env("OPENCODE_CONFIG_CONTENT", opencode_config(model, base_url)?);
    }
    if agent.eq_ignore_ascii_case("pi") {
        let directory = pi_config_dir.ok_or_else(|| {
            Error::Run(
                "cannot configure Pi cli2key provider without a project-local Pi config directory"
                    .into(),
            )
        })?;
        configure_pi_provider(
            command,
            directory,
            "cli2key",
            model,
            base_url,
            "openai-responses",
            "CLI2KEY_API_KEY",
        )?;
    }
    if agent.eq_ignore_ascii_case("qwen-code") {
        command
            .env_remove("OPENAI_API_KEY")
            .env_remove("OPENAI_BASE_URL")
            .env_remove("OPENAI_API_BASE")
            .env_remove("OPENAI_MODEL");
    }
    if agent.eq_ignore_ascii_case("mini-swe") {
        command
            .env("MSWEA_CONFIGURED", "true")
            .env("MSWEA_MODEL_NAME", format!("anthropic/{model}"))
            .env("MSWEA_SILENT_STARTUP", "1")
            .env("MSWEA_COST_TRACKING", "ignore_errors");
    }
    if agent.eq_ignore_ascii_case("openhands") {
        command
            .env("LLM_API_KEY", key)
            .env("LLM_BASE_URL", base_url.trim_end_matches("/v1"))
            .env("LLM_MODEL", format!("anthropic/{model}"))
            .env("OPENHANDS_SUPPRESS_BANNER", "1");
    }
    if agent.eq_ignore_ascii_case("workbuddy") {
        let directory = workbuddy_config_dir.ok_or_else(|| {
            Error::Run(
                "cannot configure WorkBuddy cli2key provider without a project-local configuration directory".into(),
            )
        })?;
        configure_workbuddy(directory, model, base_url)?;
        command
            .env("CODEBUDDY_API_KEY", key)
            .env("CODEBUDDY_AUTH_TOKEN", key)
            .env("CODEBUDDY_BASE_URL", base_url)
            .env("CODEBUDDY_MODEL", model)
            .env("CODEBUDDY_CONFIG_DIR", directory)
            .env("CODEBUDDY_API_KEY_HELPER_DISABLED", "1");
    }
    if agent.eq_ignore_ascii_case("openjiuwen") {
        command.env("OPENJIUWEN_API_KEY", key);
    }
    if agent.eq_ignore_ascii_case("goose") {
        command
            .env("GOOSE_PROVIDER", "openai")
            .env("GOOSE_MODEL", model)
            .env("OPENAI_HOST", base_url.trim_end_matches("/v1"))
            .env("OPENAI_BASE_PATH", "/v1/chat/completions")
            .env("OPENAI_API_KEY", key);
    }
    if agent.eq_ignore_ascii_case("cline") {
        command
            .env("OPENAI_API_KEY", key)
            .env("OPENAI_BASE_URL", base_url)
            .env("OPENAI_API_BASE", base_url)
            .env("OPENAI_MODEL", model)
            .env("CLINE_API_KEY", key);
    }
    if agent.eq_ignore_ascii_case("continue") {
        command
            .env("CONTINUE_API_KEY", key)
            .env("OPENAI_API_KEY", key)
            .env("OPENAI_BASE_URL", base_url)
            .env("OPENAI_API_BASE", base_url)
            .env("CONTINUE_MODEL", model);
        let directory = continue_config_dir.ok_or_else(|| {
            Error::Run(
                "cannot configure Continue without a project-local configuration directory".into(),
            )
        })?;
        configure_continue(directory, model, base_url)?;
        command
            .env("CONTINUE_CONFIG", directory.join("config.yaml"))
            .env("CONTINUE_GLOBAL_DIR", directory);
    }
    if let Some(executable) = codex_executable {
        command.env("CLI2KEY_CODEX_BIN", executable);
    }
    Ok(())
}

fn configure_pi_provider(
    command: &mut Command,
    directory: &Path,
    provider: &str,
    model: &str,
    base_url: &str,
    api: &str,
    api_key: &str,
) -> Result<(), Error> {
    std::fs::create_dir_all(directory).map_err(|error| {
        Error::Run(format!(
            "cannot create Pi provider configuration directory: {error}"
        ))
    })?;
    let configuration = serde_json::json!({
        "providers": {
            provider: {
                "baseUrl": base_url,
                "api": api,
                "apiKey": api_key,
                "models": [{
                    "id": model,
                    "name": model,
                    "reasoning": true,
                    "input": ["text"],
                    "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0},
                    "contextWindow": 128000,
                    "maxTokens": 16384
                }]
            }
        }
    });
    let path = directory.join("models.json");
    let content = serde_json::to_vec_pretty(&configuration)
        .map_err(|error| Error::Run(format!("cannot render Pi provider configuration: {error}")))?;
    std::fs::write(&path, content)
        .map_err(|error| Error::Run(format!("cannot write {}: {error}", path.display())))?;
    command.env("PI_CODING_AGENT_DIR", directory);
    Ok(())
}

fn configure_workbuddy(directory: &Path, model: &str, base_url: &str) -> Result<(), Error> {
    std::fs::create_dir_all(directory).map_err(|error| {
        Error::Run(format!(
            "cannot create WorkBuddy provider configuration directory: {error}"
        ))
    })?;
    let configuration = serde_json::json!([{
        "id": model,
        "name": model,
        "url": base_url,
        "api": "openai-responses"
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
    })?;
    Ok(())
}

fn configure_continue(directory: &Path, model: &str, base_url: &str) -> Result<(), Error> {
    std::fs::create_dir_all(directory).map_err(|error| {
        Error::Run(format!(
            "cannot create Continue provider configuration directory: {error}"
        ))
    })?;
    let configuration = format!(
        "name: ASB provider\nversion: 1.0.0\nmodels:\n  - name: {model}\n    provider: openai\n    model: {model}\n    apiBase: {base_url}\n"
    );
    std::fs::write(directory.join("config.yaml"), configuration).map_err(|error| {
        Error::Run(format!(
            "cannot write Continue provider configuration: {error}"
        ))
    })
}

fn opencode_config(model: &str, base_url: &str) -> Result<String, Error> {
    let mut models = serde_json::Map::new();
    models.insert(model.to_owned(), serde_json::json!({"name": model}));
    serde_json::to_string(&serde_json::json!({
        "$schema": "https://opencode.ai/config.json",
        "provider": {
            "cli2key": {
                "npm": "@ai-sdk/anthropic",
                "name": "cli2key (Codex Bridge)",
                "options": {
                    "baseURL": base_url,
                    "apiKey": "{env:CLI2KEY_API_KEY}"
                },
                "models": models
            }
        }
    }))
    .map_err(|error| {
        Error::Run(format!(
            "cannot configure OpenCode cli2key provider: {error}"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli2key_preparation_configures_the_selected_agent_api() {
        let mut command = Command::new("cli2key");
        apply_with_tape(
            &mut command,
            "opencode",
            "cli2key",
            "gpt-test",
            Some("/project/codex"),
            Some(("cb_test", "http://127.0.0.1:3456/v1")),
            None,
            None,
            None,
            None,
            false,
        )
        .expect("preparation succeeds");
        let values = command
            .get_envs()
            .filter_map(|(name, value)| {
                value.map(|value| (name.to_string_lossy(), value.to_string_lossy()))
            })
            .collect::<std::collections::HashMap<_, _>>();
        assert!(values.contains_key("CLI2KEY_API_KEY"));
        assert_eq!(
            values.get("CLI2KEY_MODEL").map(|value| value.as_ref()),
            Some("gpt-test")
        );
        assert_eq!(
            values.get("CLI2KEY_CODEX_BIN").map(|value| value.as_ref()),
            Some("/project/codex")
        );
        assert_eq!(
            values.get("OPENAI_API_KEY").map(|value| value.as_ref()),
            Some("cb_test")
        );
        assert_eq!(
            values.get("OPENAI_BASE_URL").map(|value| value.as_ref()),
            Some("http://127.0.0.1:3456/v1")
        );
        assert_eq!(
            values.get("OPENAI_API_BASE").map(|value| value.as_ref()),
            Some("http://127.0.0.1:3456/v1")
        );
        assert_eq!(
            values.get("ANTHROPIC_BASE_URL").map(|value| value.as_ref()),
            Some("http://127.0.0.1:3456")
        );
        let config: serde_json::Value = serde_json::from_str(
            values
                .get("OPENCODE_CONFIG_CONTENT")
                .expect("OpenCode config is set"),
        )
        .expect("OpenCode config is valid JSON");
        assert_eq!(
            config["provider"]["cli2key"]["models"]["gpt-test"]["name"],
            "gpt-test"
        );
        assert_eq!(
            config["provider"]["cli2key"]["options"]["apiKey"],
            "{env:CLI2KEY_API_KEY}"
        );
    }

    #[test]
    fn pi_cli2key_preparation_writes_project_local_provider_config() {
        let directory = std::env::temp_dir().join(format!("asb-pi-config-{}", std::process::id()));
        let mut command = Command::new("pi");
        apply_with_tape(
            &mut command,
            "pi",
            "cli2key",
            "gpt-6.1-sol",
            None,
            Some(("cb_test", "http://127.0.0.1:3456/v1")),
            Some(&directory),
            None,
            None,
            None,
            false,
        )
        .expect("Pi preparation succeeds");
        let config: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(directory.join("models.json")).expect("Pi config exists"),
        )
        .expect("Pi config is JSON");
        assert_eq!(config["providers"]["cli2key"]["api"], "openai-responses");
        assert_eq!(
            config["providers"]["cli2key"]["models"][0]["id"],
            "gpt-6.1-sol"
        );
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn workbuddy_cli2key_preparation_uses_project_bridge_credentials() {
        let directory =
            std::env::temp_dir().join(format!("asb-workbuddy-config-{}", std::process::id()));
        let mut command = Command::new("codebuddy");
        apply_with_tape(
            &mut command,
            "workbuddy",
            "cli2key",
            "gpt-6.1-sol",
            None,
            Some(("cb_test", "http://127.0.0.1:3456/v1")),
            None,
            Some(&directory),
            None,
            None,
            false,
        )
        .expect("WorkBuddy preparation succeeds");
        let values = command
            .get_envs()
            .filter_map(|(name, value)| {
                value.map(|value| (name.to_string_lossy(), value.to_string_lossy()))
            })
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(
            values.get("CODEBUDDY_API_KEY").map(|value| value.as_ref()),
            Some("cb_test")
        );
        assert_eq!(
            values
                .get("CODEBUDDY_AUTH_TOKEN")
                .map(|value| value.as_ref()),
            Some("cb_test")
        );
        assert_eq!(
            values.get("CODEBUDDY_BASE_URL").map(|value| value.as_ref()),
            Some("http://127.0.0.1:3456/v1")
        );
        assert_eq!(
            values.get("CODEBUDDY_MODEL").map(|value| value.as_ref()),
            Some("gpt-6.1-sol")
        );
        assert_eq!(
            values
                .get("CODEBUDDY_API_KEY_HELPER_DISABLED")
                .map(|value| value.as_ref()),
            Some("1")
        );
        let config: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(directory.join("models.json")).expect("models config exists"),
        )
        .expect("models config is JSON");
        assert_eq!(config[0]["id"], "gpt-6.1-sol");
        assert_eq!(config[0]["api"], "openai-responses");
        assert_eq!(config[0]["url"], "http://127.0.0.1:3456/v1");
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn continue_cli2key_preparation_writes_project_local_yaml() {
        let directory =
            std::env::temp_dir().join(format!("asb-continue-config-{}", std::process::id()));
        let mut command = Command::new("cn");
        apply_with_tape(
            &mut command,
            "continue",
            "cli2key",
            "gpt-test",
            None,
            Some(("cb_test", "http://127.0.0.1:3456/v1")),
            None,
            None,
            Some(&directory),
            None,
            false,
        )
        .expect("Continue preparation succeeds");
        let config = std::fs::read_to_string(directory.join("config.yaml")).expect("config");
        assert!(config.contains("provider: openai"));
        assert!(config.contains("model: gpt-test"));
        assert!(config.contains("apiBase: http://127.0.0.1:3456/v1"));
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn openrouter_preparation_maps_new_agents_to_compatible_endpoints() {
        let directory =
            std::env::temp_dir().join(format!("asb-openrouter-config-{}", std::process::id()));
        let mut command = Command::new("cn");
        configure_openai_compatible(
            &mut command,
            "continue",
            "deepseek/deepseek-chat-v3-0324:free",
            "or-test",
            "https://openrouter.ai/api/v1",
            Some(&directory),
        )
        .expect("OpenRouter preparation succeeds");
        let values = command
            .get_envs()
            .filter_map(|(name, value)| {
                value.map(|value| (name.to_string_lossy(), value.to_string_lossy()))
            })
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(
            values.get("OPENAI_API_KEY").map(|v| v.as_ref()),
            Some("or-test")
        );
        assert_eq!(
            values.get("OPENAI_BASE_URL").map(|v| v.as_ref()),
            Some("https://openrouter.ai/api/v1")
        );
        assert_eq!(
            values.get("ANTHROPIC_BASE_URL").map(|v| v.as_ref()),
            Some("https://openrouter.ai/api")
        );
        assert!(
            std::fs::read_to_string(directory.join("config.yaml"))
                .expect("Continue config")
                .contains("provider: openai")
        );
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn pi_openrouter_record_uses_tape_provider_configuration() {
        let directory =
            std::env::temp_dir().join(format!("asb-pi-openrouter-config-{}", std::process::id()));
        let mut command = Command::new("pi");
        configure_pi_provider(
            &mut command,
            &directory,
            "openrouter",
            "deepseek/deepseek-chat-v3-0324:free",
            "http://127.0.0.1:4567/v1",
            "openai-completions",
            "OPENROUTER_API_KEY",
        )
        .expect("Pi OpenRouter preparation succeeds");
        let config: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(directory.join("models.json")).expect("Pi config exists"),
        )
        .expect("Pi config is JSON");
        assert_eq!(
            config["providers"]["openrouter"]["baseUrl"],
            "http://127.0.0.1:4567/v1"
        );
        assert_eq!(
            config["providers"]["openrouter"]["api"],
            "openai-completions"
        );
        assert_eq!(
            config["providers"]["openrouter"]["apiKey"],
            "OPENROUTER_API_KEY"
        );
        let values = command
            .get_envs()
            .filter_map(|(name, value)| {
                value.map(|value| (name.to_string_lossy(), value.to_string_lossy()))
            })
            .collect::<std::collections::HashMap<_, _>>();
        assert!(!values.contains_key("OPENROUTER_API_KEY"));
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn pi_openrouter_replay_uses_no_live_credential() {
        let directory =
            std::env::temp_dir().join(format!("asb-pi-openrouter-replay-{}", std::process::id()));
        let mut command = Command::new("pi");
        apply_with_tape(
            &mut command,
            "pi",
            "openrouter",
            "tool-model",
            None,
            None,
            Some(&directory),
            None,
            None,
            Some("http://127.0.0.1:4567/v1"),
            true,
        )
        .expect("replay preparation succeeds without a provider key");
        let values = command
            .get_envs()
            .filter_map(|(name, value)| {
                value.map(|value| (name.to_string_lossy(), value.to_string_lossy()))
            })
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(
            values.get("OPENROUTER_API_KEY").map(|value| value.as_ref()),
            Some("asb-replay")
        );
        let config: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(directory.join("models.json")).expect("Pi config exists"),
        )
        .expect("Pi config is JSON");
        assert_eq!(
            config["providers"]["openrouter"]["api"],
            "openai-completions"
        );
        let _ = std::fs::remove_dir_all(directory);
    }
}
