# Agent Systems Benchmark

ASB is a small command-line benchmark runner. Initialize a project, install an
agent/workload, create a configuration, then run it:

```text
asb init
asb tool install AGENT
asb workload install SOURCE.txt|URL
asb models [PROVIDER]
asb setup --agent AGENT --workload WORKLOAD --provider PROVIDER --model MODEL [--workspace DIR] [--output FILE]
asb run CONFIG
```

`init` writes `.asb/project.toml` with project-local tool, workload, and
workspace directories. `setup` writes a human-readable TOML configuration.
`run` loads the project and benchmark configuration, selects the workload
prompt, and starts the configured agent with the provider and model in the
environment. Credentials stay outside the configuration.

`models` queries live provider model catalogs. Pass a provider to query one,
or omit it to query every supported provider and report unavailable providers
without stopping the rest. Use `--json` for machine-readable output. Model
discovery uses the provider credential and endpoint environment variables but
never writes or prints credential values.

Before a run, ASB executes a modular preparation stage. For `cli2key`, it
reuses `CLI2KEY_API_KEY` when supplied or creates an ephemeral process-local
key for the cli2key executable. ASB passes that executable the key, selected
model, and local Codex binary through `CLI2KEY_*` environment variables; the
key and endpoint are never passed to Codex.

Built-in tool recipes install pinned releases into the project (and do not
modify global tools): `codex`, `opencode`, `gemini`, `qwen-code`, and `cli2key`
use npm; `aider`, `mini-swe`, and `openhands` use an isolated Python virtual
environment. Any other agent can be installed from a local executable or URL
with `--source`.

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
