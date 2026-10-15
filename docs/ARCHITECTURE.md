# Architecture

ASB is deliberately split into a small CLI and a file-backed workload directory:

- `benchmark`: the configuration record and workload prompt catalogue;
- `config`: the `setup` command and TOML writer;
- `providers`: a concise provider-name and environment-variable registry;
- `runner`: configuration loading and live agent process execution.
- `workloads/`: one UTF-8 `.txt` prompt per workload name.

The CLI is the stable user boundary. A benchmark configuration names an agent,
workload, provider, model, and workspace. Provider credentials are never stored
in that file. The runner passes provider/model selection through environment
variables and gives the selected agent one bounded workload prompt.
Provider credentials are never inspected by ASB: the child agent inherits the
parent environment. Known providers document conventional credential and
endpoint variable names, while any custom provider name is accepted unchanged.
Workloads are loaded from `workloads/` beside the configuration file. Set
`ASB_WORKLOAD_DIR` to use another directory; add a new `.txt` file to extend
the catalogue.

Future features should be separate modules behind these boundaries. Recording,
replay, measurements, tool commands, and a graphical or terminal frontend are
not part of the current product.
