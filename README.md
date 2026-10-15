# Agent Systems Benchmark

ASB is a small command-line benchmark runner. The current product has two
commands:

```text
asb setup --agent AGENT --workload WORKLOAD --provider PROVIDER --model MODEL [--workspace DIR] [--output FILE]
asb run CONFIG
```

`setup` writes a human-readable TOML configuration. `run` loads that file,
creates its workspace, selects the workload prompt, and starts the configured
agent with the provider and model in the environment. Credentials stay outside
the configuration and are supplied by the selected agent/provider installation.

The CLI is intentionally modular. Configuration, workload prompts, and process
execution are separate modules so recording/replay, measurements, tool
commands, and an external frontend can be added later without changing the
setup file or command shape.

## Build

```sh
cargo build --locked
cargo test --locked
```

The only workspace member is `crates/asb-cli`.

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
