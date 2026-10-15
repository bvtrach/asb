# Agent Systems Benchmark

ASB is a small command-line benchmark runner. Initialize a project, install an
agent/workload, create a configuration, then run it:

```text
asb init [PROJECT_DIR] [--reset]
asb tool list
asb tool install AGENT
asb workload list
asb workload install SOURCE.txt|URL
asb provider models [PROVIDER]
asb setup --agent AGENT --workload WORKLOAD --provider PROVIDER --model MODEL [--workspace DIR] [--output FILE]
asb run CONFIG [--json]
```

`init` writes `.asb/project.toml` with project-local tool, workload, and
workspace directories and copies the bundled workload prompts into
`workloads/`. `setup` writes a human-readable TOML configuration.
Run `asb init` in a separate benchmark project directory; it intentionally
refuses to modify the ASB source checkout.
If the directory is already initialized, ASB warns and makes no changes;
rerun with `--reset` to remove managed state and create a fresh project.
`run` loads the project and benchmark configuration, selects the workload
prompt, and starts the configured agent with the provider and model in the
environment. It prints a concise human-readable result by default; pass
`--json` for machine-readable output. Credentials stay outside the
configuration.

`provider models` queries live provider model catalogs. Pass a provider to query one,
or omit it to query every supported provider and report unavailable providers
without stopping the rest. Use `--json` for machine-readable output. Model
discovery uses the provider credential and endpoint environment variables but
never writes or prints credential values.
When an environment credential is absent, ASB also checks the bounded local
file `$HOME/.api_key_<provider>` (for example `$HOME/.api_key_openrouter`).
For `cli2key`, `asb tool install cli2key` refreshes the local bridge key, and
`asb provider models cli2key` starts the project-local bridge and discovers its
available Codex models without requiring `CLI2KEY_API_KEY`.

Every command and subcommand has concise help; use `asb COMMAND --help`.
`asb tool list` shows all built-in agent installers and `asb workload list`
shows the `.txt` prompts available in the current project plus installable
external workload aliases. Install one by its short name, for example
`asb workload install swe-bench`; ASB fetches the pinned upstream description
into the current project's workload directory. These aliases install a prompt
adapter/description, not an upstream dataset or evaluator.

Before a run, ASB executes a modular preparation stage. For `cli2key`, it
starts the project-local bridge when `CLI2KEY_BASE_URL` is not supplied, reads
the bridge API key, and configures the selected agent with the bridge endpoint,
API key, and selected model. OpenCode uses the bridge's Anthropic Messages API
for compatibility. The bridge process is stopped when the run finishes. If an
external bridge endpoint is supplied, `CLI2KEY_API_KEY` must contain its
matching key.

The bundled cli2key bridge executes workloads through `codex`, `opencode`,
`qwen-code`, `mini-swe`, `openhands`, and `openjiuwen`. `aider`, `gemini`, and
`workbuddy` require wire protocols the bridge does not expose, so ASB rejects
those combinations before launch. `codeagent` is a web-server-only binary and
has no non-interactive workload command.

Built-in tool recipes install pinned releases into the project (and do not
modify global tools): `codex`, `opencode`, `codeagent`, `workbuddy`, `gemini`,
`qwen-code`, and `cli2key` use npm; `aider`, `mini-swe`, `openhands`, and
`openjiuwen` use an isolated Python virtual environment. Any other agent can
be installed from a local executable or URL with `--source`.

The CLI is intentionally modular. Configuration, workload prompts, and process
execution are separate modules so recording/replay, measurements, tool
commands, and an external frontend can be added later without changing the
setup file or command shape.

## Build

```sh
cargo build --locked
cargo test --locked
```

Install the already-built binary into the current user's `~/.local/bin` with:

```sh
make install
```

The only workspace member is `crates/asb-cli`.

Update a checkout to the current `main` branch with:

```sh
make update
```

## Example

```sh
asb setup \
  --agent codex \
  --workload original.repository-navigation \
  --provider openrouter \
  --model openai/gpt-oss-20b:free \
  --output benchmark.toml

asb run benchmark.toml
```
