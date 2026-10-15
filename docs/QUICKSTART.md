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
Credentials come from the environment used to launch ASB. For example,
OpenRouter agents can use `OPENROUTER_API_KEY`; cli2key integrations can use
`CLI2KEY_API_KEY` and `CLI2KEY_BASE_URL`. ASB does not store or inspect these
values.

Workloads are the `.txt` files in `workloads/`. Add a file such as
`workloads/my-task.txt`, then select it with `--workload my-task`. To keep
workloads elsewhere, set `ASB_WORKLOAD_DIR` before `asb run`.
