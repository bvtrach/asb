# Architecture

ASB is deliberately split into a small CLI and a file-backed workload directory:

- `benchmark`: the configuration record and workload prompt catalogue;
- `config`: the `setup` command and TOML writer;
- `install`: local copying of agent executables and workload prompt files;
- `providers`: a concise provider-name and environment-variable registry;
- `models`: bounded live model-catalog discovery using provider-specific
  endpoints and response shapes;
- `prepare`: provider-specific, pre-launch environment preparation hooks;
- `project`: `.asb/project.toml` and shared project paths. `init` creates the
  file, copies the bundled workload prompts and fixture trees, and creates its
  configured `tools_dir`, `workloads_dir`, and `workspace_dir`;
  every command resolves relative paths from this project root;
- `runner`: configuration loading and live agent process execution;
- `workloads/`: one UTF-8 `.txt` prompt per workload name. Built-in supporting
  files are stored under `.asb/fixtures/<name>/` and materialized into the run
  workspace without overwriting existing edits.

The CLI is the stable user boundary. A benchmark configuration names an agent,
workload, provider, model, and workspace. Provider credentials are never stored
in that file. The runner passes provider/model selection through environment
variables and gives the selected agent one bounded workload prompt.
Provider credentials are not stored in benchmark configuration. Known providers
document conventional credential and endpoint variable names; model discovery
and child launch use the environment first and then a bounded
`$HOME/.api_key_<provider>` file. Any custom provider name is accepted unchanged.
The `provider models` command is the one exception that reads the selected credential
only long enough to send a catalog request; it sends it through curl's stdin
configuration, never includes it in arguments or output, and never stores it.
Workloads are loaded from `workloads/` beside the configuration file. Set
`ASB_WORKLOAD_DIR` to use another directory; add a new `.txt` file to extend
the catalogue.

`asb tool install` installs a pinned package recipe into the configured tools
directory for the supported agents, or downloads/copies an executable supplied
with `--source`. npm recipes are project-local; Python recipes use a
project-local virtual environment. `asb workload install` downloads a prompt
URL, resolves a pinned external workload alias, or copies a local prompt into
the configured workloads directory. The `swe-bench` and `terminal-bench`
aliases download pinned upstream source archives into local bundle directories;
the runner copies those sources into `external-workload/` in the workspace.
Other aliases install pinned prompt metadata. These commands do not handle
credentials.
Installing `cli2key` also asks its bridge to refresh the local API key; model
discovery and runs then start the bridge and use that key ephemerally.
`asb tool list` and `asb workload list` expose these two catalogs without
installing anything. Command-specific help is available with `--help`.
The ASB source checkout is not a benchmark project: `init` rejects its
distinctive workspace layout before creating any files.

Future features should be separate modules behind these boundaries. Recording,
replay, measurements, tool commands, and a graphical or terminal frontend are
not part of the current product.

Finite human-entered names use bounded edit-distance suggestions on failure.
The original value is never replaced automatically; provider model IDs remain
provider-specific unless a finite local catalog is available.

The preparation stage is deliberately separate from command construction. Its
cli2key hook starts or connects to the OpenAI-compatible bridge, supplies the
bridge endpoint and API key to the selected agent, and tears down a bridge
process owned by ASB after the run. Additional preparation operations can be
added without changing `run` or the benchmark configuration format.
