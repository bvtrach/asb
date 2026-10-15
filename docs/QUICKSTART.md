# Quickstart

Build ASB, initialize a project, install an agent and workload, create a
configuration, then run it:

```sh
cargo build --locked
./target/debug/asb init
./target/debug/asb tool install codex
./target/debug/asb workload install \
  https://raw.githubusercontent.com/martin-beck/asb/main/workloads/bug-fix.txt \
  --name bug-fix
./target/debug/asb setup \
  --agent codex \
  --workload customer-task \
  --provider openrouter \
  --model openai/gpt-oss-20b:free \
  --output benchmark.toml
./target/debug/asb run benchmark.toml
```

Credentials come from the environment used to launch ASB. For example,
OpenRouter agents can use `OPENROUTER_API_KEY`; cli2key integrations can use
`CLI2KEY_API_KEY` and `CLI2KEY_BASE_URL`. ASB does not store or inspect these
values.

Workloads are the `.txt` files in `workloads/`. Add a file such as
`workloads/my-task.txt`, then select it with `--workload my-task`. To keep
workloads elsewhere, set `ASB_WORKLOAD_DIR` before `asb run`.

Install a local agent executable or external prompt with:

```sh
asb tool install my-agent --source /path/to/my-agent
asb workload install https://example.org/workloads/customer-task.txt
```

The built-in recipes install into `.asb-tools/<agent>/` and run from there.
Source-installed agents use `.asb-tools/<agent>/bin/`; workloads are stored in
`workloads/`. These paths come from `.asb/project.toml`, so changing that file
changes every command consistently. Add `--dir DIR` only when you need a
one-off destination.

The built-in recipes are:

| Agent | Installer | Local executable |
| --- | --- | --- |
| `codex` | pinned npm package | `codex` |
| `opencode` | pinned npm package | `opencode` |
| `gemini` | pinned npm package | `gemini` |
| `qwen-code` | pinned npm package | `qwen` |
| `aider` | pinned Python package | `aider` |
| `mini-swe` | pinned Python package | `mini` |
| `openhands` | pinned Python package | `openhands` |

The npm recipes require `npm`; the Python recipes require `python3` with
virtual-environment support. `asb tool install AGENT --dry-run` prints the
exact local installation plan without changing the project.
