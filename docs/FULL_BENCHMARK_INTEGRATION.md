# Full benchmark integration: delivery checklist

Target release: next ASB minor release (`0.2.0` from the current `0.1.x`).
This work is complete only when all five real upstream benchmarks are usable
through ASB setup/run/sweep, verified, merged through a PR, main CI is green,
and the minor release is published. Reference downloads, toy smoke trials,
generic compatibility, mock-only results, and just installing an upstream CLI
are not substitutes for those requirements.

## Exact benchmark identities

- SWE-bench: `SWE-bench/SWE-bench`, pinned test split.
- SWE-bench Lite: `SWE-bench/SWE-bench_Lite`, pinned test split.
- SWE-bench Verified: `SWE-bench/SWE-bench_Verified`, pinned test split.
- Terminal-Bench 1.0: the original Terminal-Bench 1 dataset and its compatible
  original harness, not Terminal-Bench 2 or the current floating repository.
  The original registry identifies its leaderboard task set as
  `terminal-bench-core@0.1.1`, with 80 selected tasks at repository commit
  `91e10457b5410f16c44364da1a34cb6de8c488a5`. Do not confuse the public
  benchmark title 1.0 with the distinct, older 71-task dataset version 0.1.0.
- Terminal-Bench 2.1: `harbor-framework/terminal-bench-2-1` and its compatible
  Harbor harness, not Terminal-Bench 2.0 or current Terminal-Bench 4.0.

All dataset/harness revisions and image identities must be recorded. Shared
engines are appropriate inside each benchmark family; the five public
workload registrations remain explicit.

## Implementation and evidence gates

- [ ] Extend WorkloadAdapter with installed task catalogs and per-attempt
  sessions. Put preparation, submission capture, evaluation, environment and
  cleanup in the owning adapters; keep agent/provider dispatch generic.
- [ ] Add structured task/dataset/attempt identity, baseline evidence,
  submission identity and evaluator outcomes to run and comparison records.
- [ ] Install functioning, pinned project-local evaluator tools and dataset
  material, not only upstream reference text. Reuse the project Python/proxy
  configuration and shared caches. Diagnose missing Docker/resource access.
- [ ] Implement real SWE-bench issue selection, clean base checkout and task
  environment, candidate patch capture and official containerized evaluation
  for all three dataset variants. Keep gold patches/evaluator tests out of
  agent inputs. Grade usable submissions independently of agent exit status.
- [ ] Implement real original Terminal-Bench 1.0 task/environment execution
  and official grading, using the configured ASB agent/provider/model.
- [ ] Implement real Terminal-Bench 2.1 task/environment execution and Harbor
  grading, using the configured ASB agent/provider/model.
- [ ] Make existing installed agents usable inside required task environments;
  do not silently replace the selected agent with an upstream default agent.
- [ ] Integrate generic execution targets, collector targeting and provider/
  tape connectivity for containerized attempts. Distinguish agent execution
  from environment preparation and grading in timing/accounting.
- [ ] Support complete catalogs and selected subsets through setup and the
  workload selector. Keep dataset task parallelism distinct from identical
  replicas/repetitions; bind record/replay material to the actual instance.
- [ ] Implement bounded resource usage, fresh attempts, safe resume/cache
  identities, precise failure classes and cancellation/crash cleanup that
  touches only ASB-owned resources. Never prune unrelated host resources.
- [ ] Aggregate complete selected populations with explicit denominators,
  coverage and infrastructure failures. Preserve official outcome semantics;
  never report partial/mock/smoke output as a full upstream score.
- [ ] Add provider-free unit/integration tests for contracts, all five
  catalogs, adapters, install commands, selection, run/sweep, record/replay,
  evaluation failures, stale evidence, accounting and cleanup.
- [ ] Validate actual upstream evaluator/container flows with known-good and
  incorrect submissions without paid/live provider calls. Record what each
  runtime test actually proves; do not confuse an oracle evaluation with
  evidence that a real agent solved the task.
- [ ] Run development gates, review the full diff, repair failures, merge via
  PR and verify exact-merge main CI on hosted/local runners.
- [ ] Publish and verify the new minor release from the verified main commit.

## Primary references

- https://www.swebench.com/SWE-bench/guides/evaluation/
- https://github.com/SWE-bench/SWE-bench
- https://github.com/harbor-framework/terminal-bench-1
- https://github.com/harbor-framework/terminal-bench-2-1
- https://github.com/harbor-framework/harbor

Ordinary tests must not contact live LLM providers or require credentials.
Any container/network fixture must be bounded and must not disrupt other host
processes. Local installations, caches and build artifacts remain under the
project tree. Large upstream datasets/images are cached inputs, not committed
source files.

## Implementation ledger

### Session and installation foundation

Implemented (not a claim of complete upstream benchmark support):

- A workload adapter now creates a mutable per-attempt `WorkloadSession`.
  The runner gets its prompt, prepared workspace, frozen baseline evidence
  and evaluator through that session instead of orchestrating file graders.
- Runs, measurements and comparisons preserve structured workload/task/attempt
  identity, with an optional pinned dataset identity. Attempts are distinct
  even when a task is repeated; dataset revisions partition comparison groups.
- Official `swebench`, `terminal-bench` and `harbor` tools have individual
  project-local Python recipes. The three installers were exercised against
  their real upstream distributions, not fake executables.
- Unit tests cover identity round trips, per-attempt state and installed
  recipe metadata. Existing run/sweep/replay regressions remain required.

Still to implement beyond the foundation: complete submission/evaluation
contracts, all five upstream workload sessions, container execution and
collector targeting, complete-suite scheduling/reporting, runtime evaluator
qualification, PR/merge/main CI and release. Keep the checklist above open
until those full requirements have direct evidence.

### SWE-bench catalog materializer

The family helper now resolves a public Hugging Face dataset to a concrete
revision, validates each task with the pinned official SDK, and stores a
stable public catalog separately from private evaluator-only rows. Payload
hashes, task/base-commit identity and bounded file reads detect accidental
changes or stale selection. Agent instructions use an explicit public-field
allowlist, never gold patches or hidden test patches. Publication is atomic;
failed or incompatible input cannot replace a previous complete catalog.

Actual official test splits materialized successfully during qualification:

- Full: 2,294 tasks, revision `c6fe717fd7a4c3ac1daa4055a4fd082c6a1d28a2`.
- Lite: 300 tasks, revision `b0dde1093fe417d83b7184254edf8199c1f0dff5`.
- Verified: 500 tasks, revision `78f471bf655a3137b2e8a75af1501690ec009ec3`.

These are catalog/SDK-schema checks, not agent-solving or container-evaluation
results. Adapter-owned Rust workload-install and TUI task-discovery hooks are
now connected. Actual `asb workload install` recognized all three complete
populations, and a fresh Lite installation through that command produced the
300 real instance selections. Legacy incompatible selections are backed up;
valid user-selected subsets remain unchanged. SWE-bench no longer falls back
to synthetic smoke trials; its real execution session is still unfinished.
Provider-free tests cover empty/duplicate/malformed tasks, immutable metadata,
private/public separation, source checksums, path/symlink escape, prompt bounds
and preservation of the previous catalog on failure.

### Official evaluation protocol and scoped Docker transport

The SWE-bench helper captures a frozen prediction and dataset row, uses a
fresh evaluation directory/run identity, and calls the installed upstream SDK.
Its result protocol verifies the selected task in both the official summary
and instance report, retaining patch hash and evaluator/dataset identities.
Provider-free fixtures cover resolved/unresolved results, wrong-task/missing
reports, empty submissions, timeouts and rejection of cached attempt reuse.
These fixtures do not by themselves qualify actual grading.

The SDK transport uses a private socketpair to the existing Docker CLI's
`system dial-stdio`, with scoped `sudo -n` only when necessary. Controllers
and dataset code remain unprivileged; no listener, host permission change,
group change, global Docker configuration or image prune is used. A live SDK
version-and-ping check succeeded against Docker 29.7.2/API 1.55 on this host.
Offline transport tests verify binary duplex I/O, failed startup, closure and
that unrelated processes survive cleanup. Abrupt-controller-crash recovery
remains an open requirement.

Actual Docker evaluator qualification also passed: the shared real instance
`astropy__astropy-12907` was evaluated against each of Full, Lite and Verified,
using its reference patch and a deliberately incorrect patch. All three
reference patches resolved and all three incorrect patches were unresolved.
The six checks took 265 seconds in total. The SDK cleanup path needed to use
the scoped SDK transport instead of its unprivileged CLI subprocess; that was
repaired, and all six attempt containers were confirmed absent afterward.
No live LLM provider was used. This qualifies one actual evaluator instance
per population, not a complete dataset run or ASB-selected-agent execution.

The six cases were repeated after adding actual image/SDK provenance capture;
all passed again in 208 seconds, with per-attempt cleanup checks. Reports now
retain the actual image ID/repository digest and installed SDK version/source
receipt, rather than treating a mutable `latest` image tag or installer recipe
as sufficient evidence of which code ran.

The real CLI installer also passed a repair qualification: install all 300
Lite tasks, deliberately damage evaluator metadata, reinstall while preserving
the changed dataset in a separate backup, verify the restored metadata, then
reinstall with a user-selected subset and verify its bytes remain unchanged.
This check took 8 seconds using the project cache. Cached datasets are verified
locally before reuse, and helper scripts are published atomically so parallel
workload installers never see a partially written controller.

Optional qualification commands (public dataset/image downloads, no LLM calls):

```sh
BENCHMARK_PROJECT=/absolute/path/to/asb-project
ASB_TEST_SWECATALOG_ROOT="$BENCHMARK_PROJECT" \
  "$BENCHMARK_PROJECT/.asb-tools/swebench/venv/bin/python" \
  crates/asb-cli/tests/benchmark_live_swebench.py
ASB_TEST_SWECATALOG_ROOT="$BENCHMARK_PROJECT" \
  ASB_TEST_BENCHMARK_ASB="$PWD/target/debug/asb" \
  python3 crates/asb-cli/tests/benchmark_live_install.py
```

Install the three workload catalogs first. The runtime check needs Docker
access and reserved free disk space; the installer check uses a new isolated
workload directory. Both retain their diagnostic artifacts under the supplied
project. Ordinary CI uses only synthetic catalog/evaluator/transport fixtures.

### Owned task execution and private controller bridge

The new generic Docker execution helper creates an exclusively owned,
resource-limited task container from a content-addressed image. It uses no
host mounts, host networking or privileged mode. Its stopped shell gate
reports namespace and Docker-daemon-host PIDs before a collector-ready
callback may resume it. A daemon-host PID is not automatically claimed to be
local to ASB. Argument/environment transport is structured; credentials are
not put into controller command arguments or echoed by protocol diagnostics.
Output capture is bounded, while live output can be streamed separately.

The SWE-bench task helper prepares the exact dataset base commit in that
container, provides only the public issue instruction, and captures a binary
Git diff against that base including committed and untracked changes. Typed
baseline evidence says `upstream-task-definition`, not a fabricated measured
test failure. Timeouts stop the whole owned execution tree and restart only
the container's idle process so partial filesystem changes can be captured.

The private JSON-lines task bridge exposes ready/paused/output/completed,
submission/evaluation and cleanup events. It executes the supplied command;
it does not choose an agent or provider. EOF/cancellation/SIGTERM cleans the
owned attempt, including an active evaluator. Evaluation uses the actual
prepared image ID, rather than resolving a mutable image tag a second time.

Live, provider-free qualifications passed:

- All three SWE-bench populations: clean base, stopped gate with PID-namespace
  verification, committed/untracked changes after exit 7, preserved partial
  changes after a deadline, and container removal (about 23 seconds total).
- Private bridge: unsuccessful synthetic changes graded unresolved and an
  oracle/reference fix graded resolved, both after exit 7; EOF while executing
  and SIGTERM while idle also remove the owned container. The reference fix
  is a qualification fixture, never evidence that an AI agent solved the task.
- A cancellation test exposed upstream thread-pool shutdown waiting before
  container cleanup. Cancellation now removes the exact owned grader first,
  unblocking its worker; both parent-controller EOF during grading and direct
  evaluator SIGTERM were qualified, with no remaining containers. One orphan
  created while reproducing that bug was explicitly removed; its evidence was
  retained, and no unrelated container/image was removed.
- The final combined bridge qualification passed all five controller cases
  plus the independent direct-harness SIGTERM case in 97 seconds. This includes
  a reference fix graded resolved after exit 7 and a non-fix graded unresolved
  after exit 7. Both runtime and grader containers were checked absent.
- A Docker SDK stream-wrapper leak found by the real environment test was
  repaired by closing both the stream and HTTP response; the repeated test
  produces no resource warnings. The SDK dependency is pinned to Docker 7.1.0.

Run the additional optional checks with the same project/SDK interpreter:

```sh
ASB_TEST_SWECATALOG_ROOT="$BENCHMARK_PROJECT" \
  "$BENCHMARK_PROJECT/.asb-tools/swebench/venv/bin/python" \
  crates/asb-cli/tests/benchmark_live_task.py
ASB_TEST_SWECATALOG_ROOT="$BENCHMARK_PROJECT" \
  "$BENCHMARK_PROJECT/.asb-tools/swebench/venv/bin/python" \
  crates/asb-cli/tests/benchmark_live_task_server.py
```

This is an actual execution/submission/evaluation helper, **not yet a usable
ASB run path**. Rust session/runner wiring, selected-tool bootstrap and private
runtime artifacts, container-aware collector accounting, local provider/tape
connectivity, crash recovery, full-suite planning/reporting and both original
Terminal-Bench runtime integrations remain required before merge/release.

### Terminal-Bench dataset compatibility checks

The pinned Terminal-Bench 1 dataset checkout's exact registry subset was
validated with the installed original SDK: all 80 `terminal-bench-core@0.1.1`
task definitions load. For Terminal-Bench 2.1, the pinned public checkout
`7131e4375048a0e408a8fb404b5f499d726b695b` has 89 actual task directories;
all 89 load with Harbor 0.24.0, including its verifier/input-path validation.
An additional template `task.toml` outside the task directory is not a 90th
benchmark instance. These are schema checks, not runtime execution evidence.

### Selected-agent guest qualification

The workload-neutral `__asb-agent` driver reuses the normal Rust agent adapter
registry, endpoint contract, preparation, output parsing and artifact accounting.
Its private request is passed through the controller environment, not argv or
an agent-visible request file. It pauses after adapter preparation, before the
client starts, so the execution controller can attach measurements. Standard
output and error remain distinct, and private result metadata retains usage,
final response and process status independently of task grading.

Project-local Node.js (including npm) and static uv recipes now support task
environment bootstrapping through the existing tool registry. npm download
caches stay within the project. Runtime PATH handling is shared rather than
implemented as agent-specific installer branches.

Qualification used the actual installed Pi package, official Node distribution
and a static ASB binary inside the official Astropy task image. ASB installed
the tools from public project caches inside the container; no host venv or
host directory was mounted. External container networking was disconnected
before execution, and a synthetic provider ran on container loopback. Actual
tool use created a file, provider-reported usage matched exact fixture counts,
patch capture included the file, and owned-container cleanup passed (11 seconds).
This proves selected-client execution, not an AI solution to the Astropy issue.
The complete native test suite and strict Clippy also passed after this change.

This remains a qualification path: normal run/sweep controller integration,
automatic portable-helper provisioning, local endpoint forwarding, container
collector targeting and both Terminal-Bench runtime engines are still open.
