# Development

Keep the project small and offline-buildable after dependencies are installed.
New behavior belongs in one of the three CLI modules and should preserve the
existing `setup` and `run` command forms.

Keep adapter-specific unit tests in the owning adapter module (or beside its
implementation helper). Shared modules keep registry, dispatch, and
cross-adapter tests; provider preparation cases belong to their providers.

Use the pinned Rust toolchain and run:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked
cargo build --locked
make rust-doc
make uml
```

`make rust-doc` runs `cargo doc --locked --no-deps --document-private-items`,
because the CLI implementation modules are intentionally private. `make uml`
installs the pinned AutoUML release under `target/tools/`, regenerates
`docs/asb-type-diagram.mmd`, normalizes AutoUML's source-path identifiers into
stable logical names, and renders `docs/asb-type-diagram.svg`. The normalized
diagram explicitly shows trait implementation edges and never publishes local
paths or branch names. It supplements the maintained architecture overview;
review both when changing public or private type relationships.

`make build` writes its generated diagrams and interactive views under `target/`,
leaving the tracked documentation unchanged. Run `make uml` and
`make interactive` explicitly when updating the published diagrams in `docs/`.
`make update` saves changes to legacy generated diagram paths in a Git stash
before pulling; it reports the stash and does not touch unrelated changes.
The backup may contain manual edits: inspect it with `git stash show -p` and
restore it deliberately if needed. A conflicting unrelated edit can still
prevent the fast-forward, and no stash is automatically discarded.

These checks are offline: replay tests start the embedded tape proxy and test
clients on loopback, but CI never starts a real agent, bridge, or live provider.
Run `asb run` with credentials only as an explicit local
smoke/benchmark operation.

Token accounting has synthetic, provider-free regression fixtures for all
registered agent families, plus replay CLI checks for JSON, logs, and
measurement files. Run the focused checks with `cargo test --offline --locked
token_usage_mock_tests` and `cargo test --offline --locked --test offline_cli
mock_replay_`. An offline Cargo flag prevents dependency downloads, and no
test requires a provider key.

The optional `local-host-checks` CI job repeats the ordinary checks on a local
Linux x86-64 GitHub Actions runner labeled `asb-local-ci`. It runs only for
pushes to `main` or a manual dispatch of `main`, and is disabled unless the repository variable
`ASB_LOCAL_CI_ENABLED` is set to `true`. The normal GitHub-hosted job remains
enabled for pull requests and pushes. The local runner needs the pinned Rust
toolchain, Node.js 22 (or a functional Actions tool cache), and access to the
dependencies needed for `cargo package`.

ASB is public. The job's event condition does not prevent an untrusted pull
request from proposing a different workflow that targets a registered self-hosted
runner. Do not register or enable a local runner until its isolation and runner
access controls have been explicitly chosen and verified; do not expose host
credentials, project trees, or host services to untrusted jobs. A dedicated
account and service hardening reduce risk but are not a disposable VM boundary.
In particular, a native runner shares the host network and can reach loopback
services, and a job runs with the runner account's own credentials. Review the
full workflow and code before approving external pull-request workflow runs.

The opt-in real-agent fixture test launches an installed client against a
synthetic loopback provider with dummy credentials. On Linux, compile it and
run each client in a separate rootless network and PID namespace with only
loopback enabled (replace the executable with the project-local installed binary).
The PID namespace also stops client-launched helper daemons when the test exits:

```sh
cargo test --locked --test real_codex_smoke --no-run
unshare -cfpn --keep-caps sh -c 'ip link set lo up && exec env ASB_REAL_AGENT_ID=codex ASB_REAL_AGENT_BIN=codex ASB_REAL_AGENT_EXECUTABLE=/absolute/path/to/codex cargo test --locked --test real_codex_smoke real_agent_uses_selected_openrouter_endpoint -- --ignored --nocapture'
```

The fixture refuses to run if it detects non-loopback network routes. Set
`ASB_REAL_PROVIDER=anthropic` to exercise the Anthropic Messages interface, or
`ASB_REAL_PROVIDER=cli2key ASB_REAL_MODEL=gpt-6-astra` for the local bridge's
Responses interface. The test uses fixture keys and never needs a live
provider credential. Some clients perform other initialization before sending
the benchmark prompt, so an endpoint probe alone is not a successful run.

Workspaces, caches, and build outputs stay under `/srv/data/projects`. Do not
put credentials, machine-specific paths, prompts, or provider responses in the
repository. Live runs receive credentials from the agent/provider environment.

Measurement smoke tests are local-only. Use `asb run CONFIG --measurements
DIR` when a host profile is needed; keep `ASB_MEASURE_*` collectors disabled in
CI. Measurement JSON must remain bounded and must not contain credentials,
provider responses, or full agent transcripts.

Lifecycle events are sent through the collector interface, and an ordered,
bounded `lifecycle.json` journal is written for measured runs, including early
failures. Optional live Perfetto tests require a project-local installation and
working tracefs permissions: set `ASB_TEST_PERFETTO_TOOLS` to the project's
tools directory and run the ignored `live_perfetto` tests serially. They use an
offline replay fixture, not a live provider; ordinary CI does not require
privileged tracing.
The separate ignored `real_trace_processor_verifies_synthetic_marker_capture`
test uses the same tool path but only reads a synthetic trace; it does not
start tracing or require tracing privileges.

Each completed run records `process_success` separately from
`workload_success`. Legacy external workload aliases install a pinned reference
plus a small deterministic adapter trial and local structural checker, not upstream
dataset instances. Even a passing adapter smoke check is unscored for benchmark
task resolution (`success_rate = null`, `workload_success = null`); its separate
`adapter_smoke_success` records only that exercise. Actual local task attempts
are scored by their evaluator, never by process exit alone. See
[the full benchmark integration ledger](FULL_BENCHMARK_INTEGRATION.md) for real SWE-bench and versioned
Terminal-Bench sessions; they use their official upstream evaluators instead
of the legacy structural checker. Optional runtime qualification scripts
`benchmark_live_swebench.py`, `benchmark_live_terminal_1.py`, and
`benchmark_live_harbor.py` need explicit local dataset/SDK/Docker opt-in, use
known fixture submissions and never contact a live LLM provider. The separate
`benchmark_live_swebench_source.py` qualification also requires
`ASB_TEST_SWE_SOURCE_ARCHIVE` pointing to the pinned source archive installed
by `asb workload install`. It forces only its own prebuilt lookup/pull to fail,
builds an original Dockerfile under the production CPU/memory/time limits,
grades known submissions, and checks cleanup without deleting shared images.
It requires current Docker Buildx/BuildKit with per-build `--resource` support.
`benchmark_live_parallel.py` uses the normal `ASB_TEST_BENCHMARK_ASB` and
`ASB_TEST_SWECATALOG_ROOT` opt-ins plus installed Pi, SWE-bench Lite and original
Terminal-Bench 1.0. A loopback fixture barrier requires both real clients to
reach it before either can receive its known repair. The test checks independent
official grades, lifecycle journals, scoped collectors, comparison graphs and
container cleanup. It never contacts a live LLM provider. Do not rebuild or
package over the selected executable while a runtime qualification is using it.
The same fixture removes completion receipts only after successful grading and
cleanup, verifies that ordinary resume refuses the missing evidence, injects a
contradictory retained grade and checks that explicit reconciliation publishes
no receipts, then restores the original report and reconciles both outcomes
without further provider requests. `benchmark_retained.py` supplies the
provider-free retained-artifact protocol tests for all five adapters.
Repeat the mixed original-task qualification with
`ASB_TEST_BENCHMARK_MEASUREMENTS=disabled` to omit collection for SWE-bench and
explicitly disable it for Terminal-Bench. It must still report official grades,
coverage, reconciliation provenance and success-rate graphs, without creating
measurement directories. The ordinary provider-free `unmeasured_sweep` CLI
test separately checks default/disabled collection, nonzero agent exits with
valid grades, unresolved tasks, comparison regeneration and cached resume.
Ordinary CI
uses synthetic provider-free protocol tests.
`benchmark_live_resources.py` additionally checks actual abrupt controller
SIGKILL with an installed SWE-bench task image: the durable ownership journal
quiesces only its recorded container, retains a partial output file and leaves
a simultaneously running unrelated fixture container untouched. Run it with
the evaluator virtual environment and explicit `ASB_TEST_SWECATALOG_ROOT`.
Artifacts are retained under that project; fixture containers are removed only
after the preservation assertions. This check does not authorize a new
benchmark attempt, assign a task grade or prove absence of in-flight daemon
creation operations. Ordinary CI covers the same ownership/locking contracts
with standard-library fixtures and requires no Docker access.
`benchmark_live_recovery.py` exercises the public CLI rather than constructing
only a resource journal. With `ASB_TEST_BENCHMARK_ASB` and
`ASB_TEST_SWECATALOG_ROOT`, it starts a cached original SWE-bench Lite attempt
through ASB with replay-only provider transport, locates its durable reference,
pins the exact controller child with a pidfd, kills it, and calls
`asb workload recover REFERENCE --json`. It verifies stopped/preserved partial
files and the absence of a fabricated submission, grade or replacement attempt.
The ordinary mixed-sweep qualification additionally checks that each reference
matches its replica's original start intent and completion seal. No test calls
a live LLM provider.

See
[evaluation semantics](EVALUATION.md). Perfetto
post-processing scopes scheduler counts and series to the benchmark process
tree and lifecycle interval; it never compares host-wide scheduler noise.
On Linux, measured commands run through a process gate that is also a child
subreaper. This retains helpers that daemonize or outlive an intermediate
wrapper under the benchmark root, so perf inheritance, bpftrace fork tracking,
pidstat sampling, and process-accounted timing include the complete relevant
tree without collecting unrelated host processes.
The guest driver likewise owns a private child supervisor; both supervisors
stop and reap leftover helpers before accepting an execution result. Offline
guest fixtures exercise double forks, ignored TERM, inherited output pipes,
nonzero exits and output-consumer failure, and verify that unrelated fixture
processes survive. The optional `benchmark_live_quiescence.py` qualification
uses the installed SWE-bench fixture and explicit
`ASB_TEST_BENCHMARK_ASB`/`ASB_TEST_SWECATALOG_ROOT` paths to check the actual
Docker kernel API and filesystem-preserving reset path. It starts only a
synthetic client and never contacts an LLM provider.

Two opt-in host checks exercise the accounting and BPF paths without a provider:

```sh
systemd-run --user --scope -p Delegate=yes -- env CARGO_NET_OFFLINE=true cargo test --offline --locked --test offline_cli live_delegated_cgroup_replay_keeps_exited_child_cpu -- --ignored --nocapture
ASB_TEST_BPFTRACE=/usr/bin/bpftrace cargo test --offline --locked --test offline_cli live_bpftrace_socket_profile_captures_replay_and_generates_graphs -- --ignored --nocapture
```

The cgroup test checks two concurrent replicas, exited CPU children, published
component counters and leaf cleanup. The BPF test needs an installed bpftrace
and existing tracepoint permissions or passwordless sudo. It checks actual
socket syscall counts and the generated SVG/HTML comparison from loopback
replay. The additional `io-wait` profile needs enabled scheduler statistics;
preflight rejects a disabled or unavailable host setting with a copyable
command. Tests never change this host-wide setting.
