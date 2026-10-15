# Quickstart

Build ASB, create a configuration, then run it:

```sh
cargo build --locked
./target/debug/asb setup \
  --agent codex \
  --workload original.repository-navigation \
  --provider openrouter \
  --model openai/gpt-oss-20b:free \
  --output benchmark.toml
./target/debug/asb run benchmark.toml
```

The agent executable must already be installed and available on `PATH`.
