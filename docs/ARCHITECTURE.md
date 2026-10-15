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
  file and its configured `tools_dir`, `workloads_dir`, and `workspace_dir`;
  every command resolves relative paths from this project root;
- `runner`: configuration loading and live agent process execution;
- `workloads/`: one UTF-8 `.txt` prompt per workload name.

The CLI is the stable user boundary. A benchmark configuration names an agent,
workload, provider, model, and workspace. Provider credentials are never stored
in that file. The runner passes provider/model selection through environment
variables and gives the selected agent one bounded workload prompt.
Provider credentials are never inspected by ASB: the child agent inherits the
parent environment. Known providers document conventional credential and
endpoint variable names, while any custom provider name is accepted unchanged.
The `models` command is the one exception that reads the selected credential
only long enough to send a catalog request; it sends it through curl's stdin
configuration, never includes it in arguments or output, and never stores it.
Workloads are loaded from `workloads/` beside the configuration file. Set
`ASB_WORKLOAD_DIR` to use another directory; add a new `.txt` file to extend
the catalogue.

`asb tool install` installs a pinned package recipe into the configured tools
directory for the supported agents, or downloads/copies an executable supplied
with `--source`. npm recipes are project-local; Python recipes use a
project-local virtual environment. `asb workload install` downloads a prompt
URL or copies a local prompt into the configured workloads directory. These
commands do not handle credentials.

Future features should be separate modules behind these boundaries. Recording,
replay, measurements, tool commands, and a graphical or terminal frontend are
not part of the current product.

The preparation stage is deliberately separate from command construction. Its
development-only cli2key hook validates the loopback endpoint and supplies an
ephemeral process-local key to the Codex-compatible environment without
persisting credentials. Additional preparation operations can be added without
changing `run` or the benchmark configuration format.
