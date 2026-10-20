# Agent Systems Benchmark

ASB is a small command-line benchmark runner. Initialize a project, install an
agent/workload, create a configuration, then run it:

```text
asb init [PROJECT_DIR] [--reset] [--yes|-y]
asb tool list
asb tool install AGENT
asb workload list
asb workload install SOURCE.txt|URL
asb workload tui
asb provider models [PROVIDER] [--free]
asb setup tui
asb setup --agent AGENT --workload WORKLOAD --provider PROVIDER --model MODEL [--workspace DIR] [--output FILE]
asb run CONFIG [--record|--replay] [--cassette FILE] [--json] [--measurements DIR]
asb sweep SWEEP.toml [--record|--replay] [--cassette-root DIR] [--respect-record-timings] [--json]
asb compare SWEEP.toml
```

Before starting each sweep configuration, ASB prints its filename, agent,
provider, model, workload, concurrency, and position within the sweep. The
percentage indicates how far through the configuration list execution has
reached, not how much of the current benchmark has completed. `--json` omits
these human-readable progress lines.

`setup tui` opens a Ratatui wizard with agent, provider, model, workload, and
measurement steps. Select multiple entries with Space; Tab or Enter advances
only after a selection. It writes one uniquely named test TOML per compatible
combination and one sweep TOML referencing those files. The model step queries
the selected provider catalogs and lists all returned tool-capable models (for
example DeepSeek, GLM, GPT, and Fable on OpenRouter), falling back to a pinned
local default only when discovery is unavailable. When generation succeeds, the
wizard prints the exact `asb sweep <generated>.toml` command to run the sweep.
`asb workload tui` opens
the same Ratatui interaction components in a
two-level workload/trial tree. Select workloads and enabled trials, then it
writes `<workload>.workload.toml` files; `setup tui` presents those trial sets
as selectable workload entries and generates one benchmark configuration per
enabled trial. Both TUIs prefix every selectable item with a colored `ⓘ`:
green means runnable, red means missing, and yellow means not locally
verifiable (for example a remote provider catalog). Selecting a missing agent,
external workload, or tool-backed collector with Space immediately runs its
project-local installer, and the item is checked only after installation
succeeds. Missing collectors are not preselected. An installation error leaves
the item unchecked and shows the reason; selecting it again retries. Providers
and models are discovered or configured rather than installed. `Ctrl-I` can
also explicitly reinstall an item on terminals that distinguish it from Tab;
Tab always navigates. While an installer runs, the footer is refreshed live as
`Installing (XX%)`; completion is shown in green and failure/unavailability in
red. Installer subprocess output is appended to `.asb/install.log` instead of
overwriting the TUI. `init` writes `.asb/project.toml` with project-local tool, workload, and
workspace directories and copies the bundled workload prompts into
`workloads/` and seeds `.asb/fixtures/` with the source, tests, and supporting
files for every built-in workload. `run` copies the selected fixture into the
workspace without overwriting existing edits. `setup` writes a human-readable
TOML configuration; when `--output` is supplied it also prints the exact
`asb run <file>` command for that configuration.

`init` detects the standard `http_proxy`, `https_proxy`, `all_proxy` and
`no_proxy` environment variables (also their uppercase counterparts; SOCKS
may also be supplied as `socks_proxy`) and saves credential-free values under
`[proxy]` in `.asb/project.toml`. You can edit these settings, for example:

```toml
[proxy]
http = "http://proxy.example:8080"
https = "http://proxy.example:8080"
socks = "socks5h://proxy.example:1080"
no_proxy = "internal.example"
```

`socks` is the all-protocol fallback, so HTTP/HTTPS settings take precedence
in clients such as curl; use `socks5h` for proxy-side DNS. At execution time,
explicit environment values override the corresponding project setting; an
empty environment value disables that setting. ASB always adds `localhost`,
`127.0.0.1`, and `::1` to the bypass list for its child processes. Values
containing credentials or query data are never saved by `init`; keep them in
the environment instead. Detection reads environment variables, not desktop or
system proxy configuration. ASB uses the settings for model and endpoint
discovery, downloads, and the built-in record/compatibility proxies. Benchmark
agents, provider bridges and installer subprocesses receive the same standard
environment variables; third-party programs must support those variables
themselves. The Node.js sidecars use `curl` when a proxy is configured, so
HTTP, HTTPS and SOCKS proxy behavior does not depend on the Node version.
When proxied, these sidecars follow same-origin redirects but reject
cross-origin redirects rather than risk sending provider credentials elsewhere.
For external Node programs, ASB also enables Node's HTTP(S) proxy opt-in when
applicable, unless `NODE_USE_ENV_PROXY` was explicitly set. This depends on
the external program's Node version and does not add SOCKS support to it.

Run `asb init` in a separate benchmark project directory; it intentionally
refuses to modify the ASB source checkout.
Initialization prepares a project-local Python environment using an available
Python 3.12 or 3.13. If neither is available, it uses `uv` to download Python
3.12 into the project. If this is not possible, it warns and Python tool
installation retries setup later; non-Python functionality remains available.
If the directory is already initialized, ASB warns and makes no changes;
rerun with `--reset` to remove managed state and create a fresh project.
On Linux, init lists every privileged command it may run (including sysctl
changes and delegated-cgroup setup) and asks for explicit `y`/`n` approval for
each setup group; Enter is never treated as approval. Use `asb init --yes` (or
`-y`) only when all listed commands are already reviewed and should be
approved automatically. If passwordless sudo is unavailable, approved sysctl
commands use the normal sudo password prompt; delegated-cgroup commands remain
passwordless-only.
For safety, reset accepts managed directories only as direct project children
and refuses symbolic links. If your project uses nested or external managed
directories, move their contents manually and update `.asb/project.toml` before
retrying. Do not modify the project tree concurrently with a reset.
`run` loads the project and benchmark configuration, selects the workload
prompt, and starts the configured agent with the provider and model in the
environment. `setup` writes a `log_file` entry (for example `foo.log` for
`foo.toml`); full bounded agent output is appended there. The terminal prints
only success rate, elapsed time, and token usage; pass `--json` for those same
metrics as JSON. Credentials stay outside the configuration. Pass `--measurements DIR` to write machine-readable
`system.json`, `run.json`, and bounded `summary.json` records. Token usage is
normalized into input, output, cached, and total tokens. ASB reads generic
`ASB_*_TOKENS` variables, provider/agent-prefixed equivalents such as
`OPENROUTER_PROMPT_TOKENS` and `CLI2KEY_COMPLETION_TOKENS`, structured JSON
usage emitted by agents, and recorded response usage in llmtape cassettes.
All supported agent families' usage events are normalized, including JSON
embedded in human-readable output, so OpenRouter and cli2key runs report the
same input, output, cached, and total fields whenever the provider reports
them. A missing usage field remains `null`; ASB never guesses token counts from
text length.

Qwen Code usage comes from its final run summary rather than intermediate
message snapshots. OpenJiuwen uses its per-model-call streaming usage events;
ASB applies a process-local compatibility fix for the pinned client's loss of
input counts between streaming events. Those events cover calls reported by the
agent, not necessarily separate startup probes. Recorded cassettes can also
provide per-request usage, including streamed provider responses. If a source
does not provide enough usage information, ASB reports it as unavailable
instead of treating a partial count as a complete run total.

Failed runs report whether the agent failed, a workload check rejected its
output, the grader itself could not evaluate it, or another run operation
failed. The terminal includes an actionable reason; the run log, JSON summary,
and measurement `run.json` include `failure_kind` and `failure_reason`.
`workload_success` is `null` when grading did not run or could not complete,
and `false` when the grader ran and rejected the output. Comparison reports
include available failure reasons. Bundled fixture graders use fixed accepted
solutions, so a rejection does not necessarily prove an alternative solution is
incorrect; external suite adapter-smoke checks are not full upstream grades.

### Agent memory and worker processes

Select `agent-memory` and `agent-workers` in the setup wizard's measurement
pane, or add them to `measurements.collectors`. Linux needs no extra tool or
privilege for these collectors. For an existing configuration, enable them with
`ASB_MEASURE_AGENT_MEMORY=1 ASB_MEASURE_AGENT_WORKERS=1 asb run CONFIG --measurements DIR`.
Then `asb compare SWEEP.toml` writes one all-agent bar-chart SVG per metric and
embeds those charts in `report.html`.

Memory is the sampled peak aggregate resident memory (RSS, MiB) of processes
running the selected agent executable or script entry point. It is **not**
system memory or the entire subprocess tree: tool calls, shells, compilers,
tests, and the ASB process gate are excluded. Shared pages count towards each
matching process's RSS, so this is not unique physical memory.
The integer worker count is the number of additional matching OS processes
observed over that benchmark execution, excluding the primary agent. It is
not a count of tool invocations, threads, or logical subagents within one
process. PID/start-time pairs distinguish successive processes.

These are sampled observations (default 1000 ms; set
`ASB_MEASURE_INTERVAL_MS=100` for finer sampling), not exact peaks or complete
spawn-event accounting. Short-lived workers may be missed. Different launcher
entry points are not inferred. No matching process or unsupported platforms
produce an explicit `unavailable` artifact with no metric, never a fabricated
zero. Each concurrent replica has independent accounting.

Measurement collectors are opt-in environment switches so normal and CI runs
remain deterministic: `ASB_MEASURE_MPSTAT=1`, `ASB_MEASURE_IOSTAT=1`, and
`ASB_MEASURE_PERF=1` start the corresponding local tools. BPFTrace programs
are selected with `ASB_MEASURE_BPFTRACE=1` and a semicolon-separated
`ASB_BPFTRACE_PROGRAMS` list such as
`program.bt?PID=123&INTERVAL=1`. ASB also includes PID-scoped BPFTrace profiles:
`sched-latency`, `cpu-migrations`, `futex-wait`, `syscall-rate`, and
`lock-contention`. Select them in a benchmark configuration as
`bpftrace:PROFILE` entries in `measurements.collectors`, or set
`ASB_BPFTRACE_PROFILES=sched-latency,futex-wait` (use `all` for every profile).
Two additional profiles are explicit opt-ins: `bpftrace:socket-operations`
records elapsed connect/receive syscall time, including CPU work, and
`bpftrace:io-wait` records completed kernel-accounted I/O waits. Their times
overlap scheduler states; they are diagnostic measures rather than exclusive
parts of wall time. The I/O profile requires `CONFIG_SCHEDSTATS` and
`kernel.sched_schedstats=1`. Its preflight gives the command
`sudo sysctl -w kernel.sched_schedstats=1` when needed; ASB does not change
that host setting automatically. During `asb init`, if either setting is needed,
ASB prints the exact `sudo sysctl -w ...` commands and requires an explicit
`y` or `n` answer (Enter is not a default). After `y`, passwordless sudo is
used when available; otherwise sudo may prompt for the user's password. Plain
`bpftrace` keeps the original default profiles;
`all` includes these optional profiles. Built-in probes acknowledge readiness
before the paused agent begins execution.
Each profile writes its own JSON result and SVG; `asb compare` embeds those
artifacts and produces one cross-run graph per numeric profile metric. The
profiles deliberately do not include a generic NUMA or language-runtime data
structure probe: neither has a portable kernel ABI, so such measurements stay
available through the custom-program interface. Flamegraphs use the local tools named by
`ASB_FLAMEGRAPH_STACKCOLLAPSE` and `ASB_FLAMEGRAPH_RENDER`; the SVG is paired
with machine-readable `flamegraph.json` metadata. The mpstat and iostat result
files use process-scoped `pidstat` sampling for the agent process and its child
tasks; they retain bounded raw samples, averaged machine-readable CPU and I/O
metrics, and per-sample `series` arrays. `perf stat` is run in interval mode
and emits the same bounded series. Set `ASB_MEASURE_INTERVAL_MS` to choose the
sampling period (100--60000 ms, default 1000 ms); pidstat rounds sub-second
values up to its whole-second interface, while perf supports the requested
millisecond period. `asb compare` turns every series with at least two points
into a line SVG with the same hover, zoom, pan, reset, and enlarge controls as
bar charts. BPFTrace custom programs can provide additional series when they
print interval-scoped `@asb_*` values; the built-in profiles now emit
cumulative one-second `@asb_*` snapshots as well as their final aggregates.
Perfetto remains the authoritative timestamped native trace and now also
extracts timestamped scheduler-event and scheduler-duration series from the
trace processor; those timestamps are used as the line-chart x coordinates.
If perf permissions,
samples, or FlameGraph tools are unavailable, `flamegraph.json` records the
reason and status as `unavailable`; an ordinary `asb run` is unaffected.
Selecting `flamegraph` also selects `perf`. ASB installs the required `perf`
tool in the project automatically when installing FlameGraph, using setup,
or starting an existing flamegraph-only configuration. `asb tool install perf`
first attempts to download and extract a matching perf package from the host's
configured APT repositories (including Ubuntu kernel-specific packages). If no
working package is available, it downloads a pinned, SHA-256-verified Linux
6.12 perf source release from kernel.org and builds it locally. The fallback
needs a C build toolchain, make, Python, flex, bison, libelf and the other
development dependencies reported in `.asb/install.log`. A source-built perf
may not support every event of a newer kernel; if libtraceevent development
files are missing, tracepoint decoding is disabled in the fallback build.
ASB does not install system
packages or change tracing permissions. Network access is needed for installation;
the resulting executable is stored in the project tools directory.
Before a run starts, perf and configured BPFTrace collectors are probed without
prompting. If direct access fails, ASB tries `sudo -n` and uses it for that
collector only; if passwordless sudo is not already authorized, the run stops
before the agent starts and prints a scoped `/etc/sudoers.d/asb-<collector>`
repair command.

Install Perfetto locally with `asb tool install perfetto`. It installs both the
project-local Tracebox collector and the project-local Perfetto trace processor.

The `system` collector records bounded, non-secret host metadata in
`system.json`: CPU model/vendor and Arm implementer/part identifiers,
package/core/thread topology, governors and frequency limits, NUMA node count,
VM/container class, compiler/runtime and selected collector versions, and
relevant kernel settings. Unavailable fields are omitted rather than guessed;
raw proc/sysfs, cgroup, command output, credentials, and private paths are never
retained.
Measured runs additionally write `components.json` with separate lifecycle
component wall times and process-accounted user/system CPU, block-I/O delay,
and scheduler wait for agent execution. Network wait remains `null` unless a
collector directly observes it; residual time is never guessed. Process-scoped
collectors include the complete agent process tree: pidstat uses child
aggregation, perf enables inherited counters for the root and discovered
descendants, and built-in bpftrace profiles track descendants as they fork.
Unrelated host processes are excluded.
For user/system CPU totals that retain exited children, run ASB with writable
cgroup-v2 delegation. On systemd Linux this needs no system-wide changes:

```sh
systemd-run --user --scope -p Delegate=yes -- asb run CONFIG.toml
systemd-run --user --scope -p Delegate=yes -- asb sweep SWEEP.toml
```

ASB creates an exclusive accounting leaf for each replica and removes it when
empty. `cpu-accounting.json` records whether cgroup totals or sampled process
accounting were used and any cleanup limitation. `ASB_CGROUP_PARENT` can name
an existing writable delegated cgroup-v2 directory for other launchers.
On Linux, `asb init` also attempts (using only `sudo -n`) to create a
user-owned `asb.slice` under the current systemd user slice, enable the CPU
controller, and save that parent in `.asb/project.toml`; measured runs use it
automatically. If the host is not using a compatible systemd cgroup layout, or
the setup fails, init reports the reason and ASB continues with sampled
process accounting. It never changes resource limits or edits sudoers.
When the `perfetto` collector is selected (or `ASB_MEASURE_PERFETTO=1` is set),
ASB validates the exact ftrace configuration before the agent starts, then
starts Tracebox after the agent PID is known, filters scheduler events to that
PID, and writes
`perfetto-scheduling.perfetto-trace`, the generated trace configuration, and
`perfetto-tracepoints.json` to the measurement directory. It also writes one
portable `perfetto-metric-*.svg` per extracted metric; `asb compare` embeds
those per-metric graphs, links the native trace, and charts the extracted
scheduler/process metrics. The trace contains
scheduler switch/wakeup, process/thread lifecycle, and fixed ASB lifecycle
markers for measurement-ready, agent spawn, agent execution, and agent exit.
It contains no prompt, provider response, configuration value, or credential.

The `perf` collector writes both `perf.data` for call-stack analysis and
`perf-stat.json` with task-clock, context switches, CPU migrations, page
faults, cycles, and instructions where host policy permits those counters.
Unavailable collector reasons are retained as JSON rather than silently being
reported as success. Ftrace is a Linux kernel tracefs interface, not a host
binary that ASB can install locally. If the installed collector needs elevated
tracefs access, ASB tries `sudo -n` during run preparation; without a scoped
passwordless sudo rule it stops before the agent starts and prints a repair
command instead of prompting or producing a partial benchmark.

Configurations generated by `asb setup` include an enabled `measurements`
section listing every collector module and defaulting its output directory to
`measurements`, plus a `log_file` defaulting to the configuration's stem with a
`.log` extension. They also include `concurrency = 1`; increase it to run
isolated copies of the same agent/model/workload in parallel. The setup TUI has
a final concurrency step for selecting this value. A hand-written configuration
can omit that section to retain the no-collection behavior, or pass `--measurements DIR` to enable collection
for one run.

`asb sweep` automatically runs the same comparison after all benchmark entries
finish, including partial sweeps so failed or missing entries remain visible.
`asb compare SWEEP.toml` can be run at any time to regenerate
`results/<sweep>-comparison/report.html` in
addition to `comparison.json` and SVG charts. Open the report locally for a
professional, offline overview: every benchmark row shows its agent, provider,
model, workload, all numeric measurements, and links to raw run/collector/log
artifacts. Checkbox filters can enable or disable individual benchmarks,
agents, providers, models, and workloads without contacting a provider or
rerunning the sweep.

`asb run CONFIG --record` runs the same configuration through ASB's built-in
local tape proxy and writes a JSONL cassette (by default below `cassettes/`
next to the configuration). ASB materializes this small proxy in a private
temporary session directory beside the cassette; no external recorder package is required.
It does not search the host `PATH` implicitly. To deliberately use another
compatible executable, set `ASB_LLM_TAPE`. Use `--cassette FILE` to choose an
explicit path. `asb run CONFIG --replay` starts the tape server and routes
all supported provider endpoints to it; it refuses a missing cassette and does
not start cli2key or connect to any live provider. Add
`--respect-record-timings` to `run --replay` or `sweep --replay` to reproduce
the bounded response delays recorded in each cassette entry. Cassettes are local test
artifacts and should not be committed when they contain provider responses.

The tape layer is a provider-neutral proxy chain. `--record` and `--replay`
enable the default `llmtape` adapter; future adapters can be chained by setting
`ASB_PROXY_CHAIN` to comma-separated names ordered from agent to provider. The
agent always receives only the chain's loopback ingress endpoint, so enabling,
disabling, or extending the chain does not change agent configuration.

For a sweep, keep the sweep entries and their order unchanged, and use the same
`--cassette-root DIR` for recording and replay (or omit it for both). To record
once and replay eight concurrent replicas, record with `concurrency = 1` in
each benchmark TOML, then set `concurrency = 8` in those TOMLs before replay.
Each replica receives an independent copy of its entry's original cassette and
an isolated workspace and agent configuration. If no single-replica cassette
exists, ASB instead looks for the `-concurrency-N` cassettes produced by a
concurrent recording. A missing cassette stops that replica; replay does not
fall back to live mode. Sweep entries run sequentially; only their replicas run
in parallel. ASB's tape routing is not a network sandbox for arbitrary agents.

Replay checks each ordered request's method, path and normalized full body,
including prompt and tool definitions. Workspace paths, UUIDs and long
timestamps are normalized to allow different replica directories and sessions.
Known credential-free health probes may be omitted by a client, and a client
retry may omit already-consumed tool-result turns; those cases are accepted
only when the endpoint, model, and initial user prompt still match. Other
differences are rejected. During replay ASB also replaces inherited HTTP(S)
and all-protocol proxies with a refused local proxy while exempting loopback,
so provider, updater, and telemetry traffic cannot leave the host. Older
recordings that normalized paths differently may need to be recorded again.
Replaying responses does not freeze
external tools or filesystem state, and without `--respect-record-timings`
provider delays are not reproduced.

`provider models` queries live provider model catalogs. Pass a provider to query one,
or omit it to query every supported provider and report unavailable providers
without stopping the rest. Use `--json` for machine-readable output. Add `--free`
to list only free OpenRouter models, OpenCode Zen models ending in `-free`,
and local Ollama models; with `--free`, omitting the provider queries only
those providers. OpenCode Zen model discovery is public and does not require
an API key. Model
discovery uses the provider credential and endpoint environment variables but
never writes or prints credential values.
Failures distinguish provider HTTP errors, proxy tunnel errors, connection and
TLS failures, invalid or truncated responses, and the 16 MiB per-response
limit. Response bodies and proxy credentials are never included in diagnostics.
If a setup TUI lookup fails, its limited built-in choices are marked
`[fallback]` and the TUI shows a warning; run `asb provider models PROVIDER`
for the full diagnostic, then go back and retry discovery.
Models explicitly reported as lacking tool support are excluded; models whose
catalog does not report tool support remain selectable, with a warning before
a run. `run` and `run --record` revalidate the selected model before launch;
`run --replay` instead validates that the cassette contains a tool-calling
request for the configured model. A listed model is not guaranteed to complete
an agent workload.
When an environment credential is absent, ASB also checks the bounded local
file `$HOME/.api_key_<provider>` (for example `$HOME/.api_key_openrouter`).
For `cli2key`, `asb tool install cli2key` refreshes the local bridge key, and
`asb provider models cli2key` starts the project-local bridge and discovers its
available Codex models without requiring `CLI2KEY_API_KEY`.
`asb provider models opencode-zen --free` queries OpenCode Zen's public model
catalog without an API key and lists its models marked with the `-free` suffix;
an `OPENCODE_API_KEY` is required only when running a benchmark through Zen.

Every command and subcommand has concise help; use `asb COMMAND --help`.
`asb tool list` shows all built-in agent and support-tool installers and `asb workload list`
shows the `.txt` prompts available in the current project plus installable
external workload aliases. Install one by its short name, for example
`asb workload install swe-bench`; ASB fetches the pinned upstream source into
the current project's workload directory. `swe-bench` and `terminal-bench`
install complete local source bundles and materialize them into the run
workspace; the other aliases install their pinned upstream prompt metadata.

**These external aliases currently run ASB adapter smoke exercises, not real
tasks from the named upstream suites.** In particular, `swe-bench` asks the
agent to fix a tiny string-normalization fixture. Downloading the upstream
harness source does not install a dataset, check out an issue's repository,
apply its test patch, or execute its official evaluator. These exercises have
`adapter_smoke_success`, but `workload_success` and `success_rate` are `null`;
they must not be used to claim a SWE-bench score. See
[evaluation semantics and limitations](docs/EVALUATION.md).

Before a run, ASB executes a modular preparation stage. For `cli2key`, it
starts the project-local bridge when `CLI2KEY_BASE_URL` is not supplied, reads
the bridge API key, and configures the selected agent with the bridge endpoint,
API key, and selected model. OpenCode uses the bridge's Anthropic Messages API
for compatibility. The bridge process is stopped when the run finishes. If an
external bridge endpoint is supplied, `CLI2KEY_API_KEY` must contain its
matching key.

The bundled cli2key bridge executes workloads through `codex`, `opencode`,
`qwen-code`, `mini-swe`, `openhands`, `openjiuwen`, `workbuddy`, `claude-code`,
`goose`, `hermes`, `cline`, and `continue` when the selected CLI supports the
configured compatible endpoint.
WorkBuddy uses project-local CodeBuddy model settings configured for the
bridge's Responses API.
Built-in tool recipes install pinned releases into the project (and do not
modify global tools): `codex`, `pi`, `opencode`, `workbuddy`,
`qwen-code`, `cli2key`, `claude-code`, `cline`, and `continue` use pinned npm
packages; `mini-swe`, `openhands`, `openjiuwen`, and `hermes` use an isolated Python
virtual environment. `goose` uses a SHA-256-pinned upstream binary. Any
other agent can be installed from a local executable or URL with `--source`.

ASB negotiates a compatible API interface and authentication style between
the selected agent and provider, then writes the agent-specific configuration.
Not every agent, provider, and model combination is compatible; ASB reports
when there is no matching interface.

If npm, Python, curl, or tar is unavailable, the failed command describes the
missing prerequisite. The npm repair uses user-local `fnm`; the Python setup
can use `uv` and records details in `.asb/python/setup.log`.

Pi is run in non-interactive print mode. With `provider = "cli2key"`, ASB
maps the request to Pi's OpenAI-compatible provider and supplies the bridge
endpoint and generated API key through the child environment.

The CLI is intentionally modular. Configuration, workload prompts, process
execution, and optional measurements are separate modules so recording/replay,
additional collectors, tool commands, and an external frontend can be added
without changing the setup file.

Finite command, tool, workload, provider, and locally enumerated model names
fail closed on typos and print the closest match as a question; ASB never
silently substitutes a value.

## Build

```sh
cargo build --locked
cargo test --locked
```

Install the already-built binary into the current user's `~/.local/bin` with:

```sh
make install
```

`make build` regenerates `target/asb.bash` and `target/_asb` from ASB's command
and option catalog, generates Rust API documentation under `target/doc/`, and
generates the AutoUML Mermaid diagram plus SVG under `docs/`. It installs the
pinned AutoUML generator project-locally under `target/tools/`. `make install`
installs Bash completion at
`~/.local/share/bash-completion/completions/asb` and Zsh completion at
`~/.local/share/zsh/site-functions/_asb`, alongside the binary.

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
