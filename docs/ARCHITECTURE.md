# Architecture

ASB is deliberately split into three small CLI modules:

- `benchmark`: the configuration record and workload prompt catalogue;
- `config`: the `setup` command and TOML writer;
- `runner`: configuration loading and live agent process execution.

The CLI is the stable user boundary. A benchmark configuration names an agent,
workload, provider, model, and workspace. Provider credentials are never stored
in that file. The runner passes provider/model selection through environment
variables and gives the selected agent one bounded workload prompt.

Future features should be separate modules behind these boundaries. Recording,
replay, measurements, tool commands, and a graphical or terminal frontend are
not part of the current product.
