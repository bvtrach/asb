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

- [x] Extend WorkloadAdapter with installed task catalogs and per-attempt
  sessions. Put preparation, submission capture, evaluation, environment and
  cleanup in the owning adapters; keep agent/provider dispatch generic.
- [x] Add structured task/dataset/attempt identity, baseline evidence,
  submission identity and evaluator outcomes to run and comparison records.
- [x] Install functioning, pinned project-local evaluator tools and dataset
  material, not only upstream reference text. Reuse the project Python/proxy
  configuration and shared caches. Diagnose missing Docker/resource access.
- [x] Implement real SWE-bench issue selection, clean base checkout and task
  environment, candidate patch capture and official containerized evaluation
  for all three dataset variants. Keep gold patches/evaluator tests out of
  agent inputs. Grade usable submissions independently of agent exit status.
- [x] Implement real original Terminal-Bench 1.0 task/environment execution
  and official grading, using the configured ASB agent/provider/model.
- [x] Implement real Terminal-Bench 2.1 task/environment execution and Harbor
  grading, using the configured ASB agent/provider/model.
- [x] Make existing installed agents usable inside required task environments;
  do not silently replace the selected agent with an upstream default agent.
- [x] Integrate generic execution targets, collector targeting and provider/
  tape connectivity for containerized attempts. Distinguish agent execution
  from environment preparation and grading in timing/accounting.
- [x] Support complete catalogs and selected subsets through setup and the
  workload selector. Keep dataset task parallelism distinct from identical
  replicas/repetitions; bind record/replay material to the actual instance.
- [x] Implement bounded resource usage, fresh attempts, safe resume/cache
  identities, precise failure classes and cancellation/crash cleanup that
  touches only ASB-owned resources. Never prune unrelated host resources.
- [x] Aggregate complete selected populations with explicit denominators,
  coverage and infrastructure failures. Preserve official outcome semantics;
  never report partial/mock/smoke output as a full upstream score.
- [x] Add provider-free unit/integration tests for contracts, all five
  catalogs, adapters, install commands, selection, run/sweep, record/replay,
  evaluation failures, stale evidence, accounting and cleanup.
- [x] Validate actual upstream evaluator/container flows with known-good and
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

Entries below are chronological implementation/qualification snapshots.
Later entries supersede earlier development limitations; the delivery checklist
above remains the full scope required before merge and release.

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

At this stage execution/submission/evaluation were helper-only. The normal
ASB connection is described in the later ledger entry. Container-aware
collector accounting, local provider/tape connectivity, crash recovery,
full-suite planning/reporting and both original Terminal-Bench runtime
integrations remain required before merge/release.

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

### Evaluation provenance and process-independent task credit

Run, measurement and comparison now share typed baseline evidence. Measured
fixture results remain distinct from official clean task-definition evidence;
the latter is accepted only for an upstream harness, with a concrete commit.
Malformed explicit evidence cannot fall back to an old boolean. Legacy
records without the new field retain their measured-baseline semantics.

The normal runner evaluates task state even after client/proxy failure. A
resolved submission can earn task credit while the command still reports its
operational failure. If both fail, separate workload diagnostics survive in
the result. Provider-free regressions cover these combinations, measurement
aggregation and comparison denominators, including missing official grades.
This removes fixture-only scoring assumptions; it does not yet wire an
official workload environment into normal run/sweep execution.

Offline runtime-cache tests additionally found that the existing checksum
helper removed a damaged archive before the new installer could preserve it.
A non-destructive verification path now retains that evidence and tests cover
replacement, offline cache reuse and previous-install backups.

The full development build exposed a Graphviz 2.43 clustered-layout crash.
Function diagrams now disable edge concentration and mark real entry/terminal
functions instead of introducing synthetic cross-cluster entry/exit calls.
Regression checks render both actual project graphs within a bounded timeout
and verify that every function and real call edge survives into SVG.

### Normal SWE-bench run integration

The generic workload-session execution hook now connects all three original
SWE-bench adapters to their owned Python/Docker sessions. Each receives the
negotiated endpoint and selected configuration from Rust, installs the chosen
client using ASB's actual tool recipes inside the task environment, executes
the same Rust agent adapter, and returns its independently parsed metadata.
The host never launches that client against an empty host workspace. Public
package caches are reused without mounting host homes/venvs; credential-bearing
registry caches are not staged. Private stdout/stderr, task/submission/evaluator
evidence and cleanup status remain under the project. Observed execution time
comes from the guest execution span, excluding tool installation and grading.

The controller transport uses bounded JSON-lines messages/queues, response
deadlines, byte-preserving output, private stdin requests and EOF cancellation.
Its tests exercise framing, padding, request round trips and child reaping.
Bootstrap tests use no Docker daemon, agent installation or live provider.
An actual Pi API failure with exit zero uncovered during qualification now
has an adapter-owned terminal-failure hook for both native and guest runs;
tests verify failures without echoing provider bodies and allow recovered turns.

Actual **normal `asb run`** qualification passed six cases in 281.5 seconds:
the real instance `astropy__astropy-12907` in Full, Lite and Verified, each with
an unrelated change and a deterministic known-good repair. ASB installed and
ran real Pi, negotiated the Responses API against a synthetic provider in an
internal Docker network, captured its real tool changes and exact reported
usage, invoked the official evaluator, and returned unresolved/resolved task
outcomes respectively. Each owned task/provider container and fixture network
was cleaned up. This is explicitly an oracle/provider fixture, not evidence
that an AI solved these issues or that a full population was benchmarked.
The final Lite positive case was repeated after the execution-time journal
was added (45.4 seconds); its reported agent elapsed time matches the guest
observation rather than tool installation or evaluator time. Explicit task-set
selection is validated independently of prompt files before the environment
starts. Final provider-free tests and strict Clippy pass.

Development limitations remain explicit: provide a static `ASB_GUEST_BINARY`;
host-loopback endpoints (including tape/local bridges) and measured container
runs currently report that their integration is unfinished. Automatic helper
provisioning, collector targeting, complete-suite scheduling/reporting/resume,
background-agent quiescence before submission freezing, both Terminal-Bench
engines, official image-build fallback when a prebuilt image is unavailable,
final PR/merge/main CI and the minor release remain required.

Optional normal-run qualification (cached public tools/image; no live LLM):

```sh
ASB_TEST_SWECATALOG_ROOT="$BENCHMARK_PROJECT" \
  ASB_TEST_BENCHMARK_ASB="$PWD/target/debug/asb" \
  ASB_TEST_GUEST_BINARY="$PWD/target/x86_64-unknown-linux-musl/debug/asb" \
  ASB_TEST_BENCHMARK_SUITE=swe-bench-lite \
  ASB_TEST_BENCHMARK_RESOLVED=1 \
  "$BENCHMARK_PROJECT/.asb-tools/swebench/venv/bin/python" \
  crates/asb-cli/tests/benchmark_live_run.py
```

### Original Terminal-Bench catalogs and first runtime

Both original Terminal-Bench catalogs now install through their owning
adapters: `terminal-bench-1.0` exposes the original 80-task
`terminal-bench-core@0.1.1` population, and `terminal-bench-2.1` exposes all 89
tasks at its pinned revision. Their separate official SDKs validate definitions.
Normalized source-tree hashes pin the original material; public catalogs omit
reference solutions and private verifiers. Damaged archives/datasets and
incompatible selections are preserved before replacement; valid user-selected
subsets remain unchanged. Complete catalogs do not imply complete execution.

Original Terminal-Bench 1.0 now uses its SDK environment, test injection,
tmux test execution and result parser while ASB's existing selected agent
adapter owns client execution. Its session shares the generic controller with
SWE-bench, not a new benchmark-name branch in the runner. Compose resources
are attempt-scoped and labelled; teardown never invokes global image/build
pruning. Task build inputs omit reference solutions and benchmark verifier
files, while retaining application-owned tests. Only SDK task interpolation
variables enter the private Compose env-file; provider credentials do not.
Baseline evidence identifies the original environment definition, not a
fictitious repository or measured initial test result. The original declared
agent timeout reaches both the guest process and controller.

Actual normal CLI qualification of `hello-world` passed with a deterministic
correct change (27.5 seconds) and an unrelated change (26.2 seconds), using
real Pi against an internal synthetic provider and the original official
grader. Neither case used a live LLM or establishes AI-solving ability.
The tests check reported token usage, actual guest execution timing, official
per-test results, and removal of owned containers. Qualification found and
repaired two minimal-image assumptions: Node now uses official gzip archives
instead of requiring xz, and the stopped execution gate resumes using the
shell's kill builtin rather than assuming `/bin/kill` exists. The verifier
restores exactly the task's original networks after an agent-network override;
leaving the internal fixture network attached prevented a verifier dependency
download and was correctly rejected rather than scored as an agent failure.

Offline tests cover both catalogs, private/public separation, safe extraction,
exact ownership/resource evidence, build-input filtering, private Compose
variables, original verifier network restoration and truthful cleanup errors.
Terminal-Bench 2.1's Harbor environment/evaluation session remains unfinished;
its adapter explicitly refuses execution rather than substituting a smoke
trial. The common connectivity, collector, full-population scheduling/reporting,
helper provisioning, image fallback and final merge/release gates remain open.

The shared-controller SWE-bench Lite positive regression also passed after
these changes (44.3 seconds). Both real Terminal-Bench installers passed
source-corruption repair and subset-preservation checks (10.2 seconds total).
The ordinary offline suite passed: 319 unit tests, 80 CLI tests, seven Python
protocol suites, three guest-driver tests and the remaining layout/graph/
replay checks. Strict Clippy and `make build` passed. None of these results
closes the remaining delivery checklist or qualifies the full task populations.

Current main was merged into the development branch after its native Hermes
adapter and collector repairs landed. The only textual conflict was resolved
by preserving both sources of step metadata: guest-returned metadata for
isolated execution and adapter-artifact/fallback parsing for native execution.
A new provider-free Hermes guest fixture additionally proves artifact-reported
steps and exact usage survive the guest driver (four calls, 42 fixture tokens).
The post-integration full offline suite and strict Clippy passed; hosted PR
checks restarted once the merge conflict was removed.

### Terminal-Bench 2.1 Harbor runtime

The 2.1 adapter now creates an owned Harbor `DockerEnvironment` and invokes
the original `Verifier`; it never constructs a Harbor agent or model. The
configured ASB client still executes through the common Rust guest adapter.
Original resource settings, public task instructions and agent/verifier
timeouts remain task-owned. Build contexts contain only the original public
environment directory, not solutions or tests. The original verifier receives
its tests only during grading, and stale reward files cannot substitute for a
new grade. Missing, non-binary or malformed rewards remain evaluation errors.

Compose labels, network/volume names, bind roots and published ports are
attempt-scoped. Host namespace/device/privileged access and external-resource
references are rejected instead of changing host policy. SDK overlays are
created below the private project attempt directory rather than `/tmp`.
Cancellation waits for the bounded in-flight Compose operation before teardown;
cleanup avoids Harbor's shared-image deletion path, verifies no attempt
containers remain, and records failures truthfully. General abrupt-crash
recovery remains a separate incomplete requirement.

An unavailable prebuilt image can fall back to the original task Dockerfile
through Harbor's own build path, within the same preparation budget. This does
not alter the dataset or silently replace its environment. The actual image
identity and preparation source are retained with task evidence. The Harbor
tool recipe now explicitly includes the Docker Python SDK needed by the shared
ASB transport; reinstalling it was qualified in the real local project.

Actual original `cancel-async-tasks` SDK qualification passed reference and
incorrect changes (48.6 seconds total). Normal `asb run` also passed with real
Pi against the isolated synthetic provider: deterministic correct change
(39.9 seconds) and unrelated change (26.3 seconds). These tests check original
Harbor rewards, exact client usage, guest-only elapsed time, task identity and
scoped cleanup. Fault-injected missing prebuilt images then exercised the
actual original source build and both original grades (52.1 seconds total).
None of these fixtures uses a live LLM or establishes AI-solving ability or
full-population execution.

Provider-free tests additionally cover scoped Compose resources, prohibited
host exposure, exact original-verifier invocation, private/stale reward
handling, malformed rewards, preparation-budget sharing and cleanup outcomes.
The previous pushed milestone's exact-head hosted CI passed. Both versioned
Terminal-Bench adapters now require a runtime helper rather than carrying an
optional unfinished-runtime registration. Common portable helper provisioning,
host-loopback provider/tape connectivity, measured container targeting,
complete-suite scheduling/resume/reporting and final delivery gates remain
open for all five benchmark targets.

The source-fallback qualification was repeated after moving SDK temporary
overlays under the project: both original grades passed again (47.2 seconds).
A separate offline regression proves synchronous signal cancellation drains
the SDK operation before teardown. The runtime now
probes exact SDK/transport dependencies before starting Docker. Workload
installation repairs incomplete environments using its owning tool recipe;
execution instead gives an explicit `asb tool install <tool>` repair command.
Both Terminal-Bench tool recipes pin Docker SDK 7.1.0, matching the shared
transport. Provider-free tests cover wrong SDK/dependency versions, cancellation
draining, reward validation and failed cleanup in addition to scoped resources.

### Automatic same-version ASB runtime staging

The normal session now uses the running ASB binary without requiring a separate
static build or `ASB_GUEST_BINARY`. Static ELF64 executables are staged directly;
dynamic executables bring their own matching ELF loader and resolved libraries.
The loader is invoked with its private library path, not `LD_LIBRARY_PATH`, so
selected agents and task programs continue to use their original environment.
No guest system library is replaced, host directory mounted, compiler run or
toolchain downloaded. The original executable bytes and library identities are
recorded in a bounded private `runtime.json` receipt. Unsupported/mismatching
architectures, malformed ELF headers, missing/conflicting libraries and size
limit violations are rejected before client execution. The process gate uses
the runtime's launcher for recursive ASB execution rather than confusing the
dynamic loader's `/proc/self/exe` with the ASB program.

Without a guest override, normal `asb run` passed deterministic positive cases
against the synthetic provider with actual Pi and official evaluators:
SWE-bench Lite (46.4 seconds), original Terminal-Bench 1.0 (27.9 seconds), and
Terminal-Bench 2.1 (37.4 seconds). Each records `bundled-loader` evidence and
checks exact usage, observed guest timing and scoped cleanup. These are
representative runtime qualification fixtures, not full population scores or
AI-solving claims. Ordinary offline tests execute a bundle of the just-built
ASB program and verify CLI/guest entry behavior without Docker, credentials or
network. A real static musl build also passed the same standalone tests.

The original Terminal-Bench qualification initially found an older installed
Docker SDK (7.2.0 rather than the pinned 7.1.0). The pre-start probe rejected it
with a concrete repair command, and `asb workload install terminal-bench-1.0`
automatically repaired the SDK dependency and retained all 80 tasks. This also
qualifies the real installer repair path, not just fake SDK metadata checks.

Remaining common gaps still include reliable language/package bootstrap in
minimal task images when host public caches are absent, local provider/tape
forwarding, collector targeting, full-population scheduling/resume/reporting,
abrupt-crash recovery, SWE-bench image-build fallback and final delivery gates.

### Recipe-owned bootstrap for minimal task images

The installer now builds a typed guest bootstrap plan from the selected agent's
existing recipe. npm clients require the registered Node distribution, Python
clients require the registered uv distribution, and the standalone recipe
supplies its pinned archive. Tool adapters own archive URLs, hashes and cache
layouts; the workload controller receives only declared runtimes and assets.
There are no new agent/provider/benchmark-name branches in the runner.

Prefetch downloads and verifies archives without replacing a host runtime or
requiring a task-image curl. Per-project/per-file locks serialize publication;
downloads are staged, checksum-verified and atomically published. Damaged and
failed downloads are retained. Runtime transfers have size/time limits. The
standalone installer now retains/reuses its verified archive and extracts into
a fresh private stage rather than overwriting a previous extraction tree.

The guest validates asset paths, regular files/project-cache boundaries, duplicate
targets and bounded reads before copying. Each attempt retains a private
`bootstrap.json` manifest without host paths. Large temporary transfer files
stay under the project attempt directory. Only npm recipes copy the separately
validated public npm cache; Python/standalone installation no longer transfers
an irrelevant npm cache. Credentials and host virtual environments are never
staged. Original recipes still perform actual installation inside the task.

Actual cold-cache normal `asb run` qualification passed original Terminal-Bench
1.0 with real Pi and official grading (50.2 seconds). The project Node/npm
caches were preserved before the run, so it needed actual primary-source
downloads rather than prepopulated archives or npm packages. The cache-only
prefetch variant passed again (55.3 seconds), explicitly checking that it did
not create/replace a host Node runtime. Prior npm cache data was restored;
prior Node directories remain preserved in qualification-project backups.

Actual uv + mini-SWE installation in the original SWE-bench environment passed
(46.7 seconds), and the standalone archive install/version check passed
(10.9 seconds). Both were then qualified with an installation-only PATH that
excludes curl and all host Python interpreters (48.8 seconds total). These are
installation/CLI usability checks, not live-provider execution or AI task
resolution. The ordinary suite uses only fake files/programs and local protocol
fixtures; registered-agent recipe coverage, asset escapes/symlinks/duplicates,
transfer bounds and stale-cache preservation are independently checked.

Preparation now carries the selected project root/configuration explicitly.
SWE-bench and both Terminal-Bench adapters resolve SDK tools and attempt
storage from that context, not by guessing a project from the catalog folder.
A regression covers catalogs outside the project with a custom tools directory.
The previous pushed runtime-staging milestone's exact-head hosted CI passed.
Local endpoint/tape forwarding, measured container targets, full-population
scheduling/resume/reporting, abrupt-crash recovery, SWE-bench source-image
fallback and final PR/merge/main-CI/minor-release gates remain open.

The no-curl Python/standalone checks were repeated with the final preparation
split (46.3 seconds), including an explicit check that uv provisioned Python
3.12 rather than relying on an interpreter from the original task image. This
qualifies installation only; no provider was contacted. The complete ordinary
suite, strict Clippy and full development build passed for the bootstrap work.

### Current-main integration and guest relay transport

Merged main `2e531c87f5958b30d0b3ca5c70837b4a01d1d673` into the development
branch. Adapter-specific tests remain in their owning modules. The complete
function diagrams exposed a Graphviz 2.43 `init_rank` failure with compound
global ranking; ordinary cluster-local ranking fixes it without omitting
functions, call edges, module boxes or hover labels. Regression tests render
both complete diagrams and count every real node and edge. Strict Clippy and
the full development build passed after this integration repair.

The same staged ASB executable now exposes a private guest TCP relay entry.
It binds only ephemeral IPv4 loopback listeners inside its execution namespace
and multiplexes raw bytes over its owned exec standard I/O. Targets remain
opaque numeric IDs: the guest never chooses or receives host destinations.
There are at most 64 listeners, 128 active channels and 64 KiB per frame.
Malformed/oversized frames are rejected, half-closes preserve the reverse
stream, closed channels cannot be reopened by late data, and transport EOF
shuts down owned sockets/listeners and joins their readers. Provider-free
real-executable tests cover binary traffic, concurrent independent targets,
1 MiB streaming, half-close responses, all 128 channels and overflow,
malformed frames, target-count bounds and disconnect cleanup.

This is the guest transport component, not completed local endpoint support.
Host-side TCP forwarding, URL/environment/project-proxy rewrites and ownership
across installation/execution must still be wired and qualified before removing
the existing host-loopback execution rejection. No host listener, host-network
mode, firewall change or global service is introduced by this component.

The post-main-integration ordinary suite passed: 336 unit tests, 82 offline CLI
tests, ten benchmark protocol suites and the guest/runtime, diagram and replay
integration checks. The expanded relay capacity/streaming checks also passed
separately after that run. Strict Clippy, `make build` and package verification
passed. These are local development gates, not hosted CI or release evidence.

### Host-loopback provider, tape and outbound-proxy forwarding

The host side now forwards only the selected explicit loopback destinations
through an owned `docker exec -i` stream. No host listener, network-host mode,
firewall change or extra host authorization is required. The guest binds the
same loopback address family (including IPv6 and non-default 127/8 addresses).
Opaque raw bytes preserve HTTP/SSE, TLS and SOCKS protocol traffic. URL rewrites
change only the port, retaining the TLS hostname, userinfo and escaped path.
Host endpoint credentials and URLs are not printed or placed in arguments.

Local API endpoints, lowercase/uppercase HTTP/HTTPS/all/SOCKS proxy environment
variables and typed project proxy configuration are all rewritten before
runtime/client installation. The relay remains owned through client execution;
success, preparation failure, execution failure and cancellation close its
sockets, streams and CLI process. Transport failure interrupts the installer
or client instead of leaving its retries running until the full task timeout.
The old blanket rejection of host-loopback connections is removed.

Provider-free regression tests exercise both sides of the real ASB relay:
binary streaming, repeated independent targets, IPv6-only localhost servers,
non-default loopback addresses, half-closes, cancellation, channel/frame limits,
malformed startup cleanup, URL/proxy configuration rewrites and immutable host
configuration. Guest-bootstrap tests prove rewriting precedes installation and
the owner closes the relay on both installation success and failure. These
tests do not require Docker, a real client or any provider credential.

Actual normal CLI qualification with real Pi and a host-loopback synthetic
provider passed one original task from each population with official grading:

- SWE-bench Full: 48.0 seconds.
- SWE-bench Lite: 48.7 seconds.
- SWE-bench Verified: 47.5 seconds.
- Terminal-Bench 1.0: 30.2 seconds.
- Terminal-Bench 2.1: 36.0 seconds.

The fixture directs a known deterministic repair; these are runtime/evaluator
qualifications, not AI-solving claims or complete population scores. Original
task environments, usage, selected identity and owned-container cleanup are
checked by each test. Terminal-Bench 1.0 also passed normal recording (28.0
seconds) and replay (28.9 seconds) through the embedded host tape proxy. A final
replay after the cancellation repair passed in 26.2 seconds and explicitly
verified zero provider inference OR catalog requests. No live LLM was contacted.

The first live check caught a mismatch between a cancellation Event and the
relay's callback contract. The relay now accepts both and the Event case has
an ordinary regression. Full offline tests, strict Clippy and the development
build passed; expanded protocol checks passed again after the last changes.
Hosted CI for preceding signed milestone `c8e278e` passed. Current changes still
require their own final package/hosted evidence before any delivery claim.

Final local gates passed again after bounding reader threads across all
listeners: the complete ordinary suite, strict Clippy, full `make build`,
package verification and archive integrity. The packaged crate's full suite
passed before that final bound adjustment; its ten protocol suites passed again
against the final packaged source. Owned qualification provider/relay processes
were confirmed absent after the tests.

Remaining full-goal gates include measured container targets, complete selected
population scheduling/resume/reporting, abrupt-crash recovery, SWE-bench source
image fallback, full-diff review, PR merge, exact-main CI and release `0.2.0`.

### Verified local measurement targets and checked normal cleanup

The isolated execution contract now carries an optional mutable measurement
session. Before resuming the stopped guest gate, ASB compares its private
kernel boot ID, namespace PID and start time with this host's bounded `/proc`
reads. The host process must still be stopped, map to the reported namespace
PID, retain its start time and belong to the exact owned container cgroup.
Daemon PID numbers alone, a remote daemon, unrelated cgroups, stale processes
and malformed identity fields are rejected before collector attachment.
The same bundled runtime produces the identity snapshot without requiring
task-image Python/proc utilities. A read-only control exec avoids recursively
acquiring the task execution lock while its gate is stopped.

Collectors receive a generic isolated-process attachment hook, not benchmark
or provider dispatch. PID-based collectors use the verified host root. Native
CPU cgroup attachment is explicitly bypassed: Docker processes are never moved
into a new ASB leaf, and no host cgroup is created, removed or modified for
these attempts. The owning CPU adapter instead reads existing container CPU
counters, freezes their delta at `AgentExecuteEnd` (before grading), and exposes
separately named `container_cpu_user_ms`/`container_cpu_system_ms`. They include
runtime relay/background services and must not be labeled agent-only CPU.
Sampled agent CPU stays separate; when no CPU activity was observed at sampling
resolution, it is unavailable rather than an invented zero total.

Normal measured CLI qualification passed with real Pi and synthetic host-local
providers for SWE-bench Lite (61.4 seconds), Terminal-Bench 1.0 (37.9 seconds)
and Terminal-Bench 2.1 (49.1 seconds). Tests check verified target provenance,
ordered agent lifecycle events and preserved cgroup membership. This qualifies
the core targeting path, not every optional collector or full population.
Agent-entrypoint-only sampler identity mapping and wider optional-collector
qualification remain necessary; do not claim those from PID dispatch alone.

Measured replay exposed a normal cleanup race: the destructor's three-second
grace could terminate an SDK controller before Compose cleanup confirmation.
The workload-session contract now has explicit checked `finish()`. The runner
executes a separate `workload.cleanup` lifecycle phase after grading, keeps the
controller input open until resource removal is acknowledged, and verifies
successful controller exit. Missing/failed acknowledgements become operational
run failures without erasing an independently successful task grade. Drop/EOF
remains cancellation fallback, not evidence of successful normal cleanup.

Measured Terminal-Bench 1.0 replay passed after that repair in 38.4 seconds:
official grading, read-only container CPU deltas, zero provider/catalog calls,
the final cleanup receipt and owned process exit were all checked. The original
failed cleanup receipt is retained; read-only checks confirmed no containers
or networks remained for that specific attempt. No global pruning was used.

Ordinary tests cover target identity mismatches, stopped-state/namespace checks,
preserved cgroup membership, separate/frozen CPU deltas, decreasing counters,
unknown sampled CPU versus exact zero counters, control-probe dispatch,
cleanup slower than the old destructor grace, and failed/missing acknowledgements
or unsuccessful controller exit. The full lifecycle regression includes the
new cleanup phase. Preceding forwarding milestone `870cf57` passed hosted CI.
Final gates for the current measurement/cleanup changes are still in progress.

The final pre-main-refresh local gates passed: 343 unit tests, 82 offline CLI
tests, all ten protocol suites, diagram/guest/replay checks, strict Clippy,
full development build, package verification/integrity and the packaged crate's
complete ordinary suite. Two replay-error checks failed once during the first
concurrent run, then passed both focused reproduction and the complete rerun;
the lifecycle expectation was separately updated for the intentional cleanup
phase. Do not treat these local passes as exact-head hosted or main CI evidence.

Remaining complete-goal work includes agent-entrypoint-specific isolated
collector support/qualification, full-population planning/scheduling/reporting
and resume, background-agent quiescence before submission capture,
failure-path cleanup/recovery and abrupt-controller-crash recovery,
SWE-bench source-image fallback,
final review, PR merge, exact-main CI and minor release `0.2.0`.

Refreshed the branch against main `706db088a4e2acf97fa50ca197ae2f34a2b43e22`.
The Pi adapter merge keeps both its terminal API-failure detection and the
upstream native step counter. Post-merge gates passed: 348 unit tests, 82
offline CLI tests, all protocol/diagram/guest/replay checks, strict Clippy,
full build, clean-checkout package verification/integrity and the packaged
crate's complete suite. Normal measured Terminal-Bench 1.0 tape replay also
passed again (41.3 seconds) with zero provider contacts and confirmed cleanup.
These are local exact-branch results; hosted CI must still validate this head.

Collector follow-up must also guard process identity for the full sampling
lifetime (not only attachment), so a root PID reused after agent exit cannot
be sampled while grading continues. This remains part of full isolated
collector support, alongside entry-point matching and optional-tool qualification.

### Namespace collector qualification and bounded error cleanup

Process sampling now guards root and descendant PID/start-time generations
and ancestry for the full interval, rejects zombies/reparented/reused tasks,
and stops promptly at execution completion. Periodic pidstat arguments belong
to their adapters rather than a collector-name branch in orchestration.
BPF tree maps and pending per-thread samples are deleted on task exit.
Perfetto SQL resolves exactly one root lifetime alive at the start marker;
ambiguous generations produce an explicit scope warning and unavailable
metrics, not mixed lifetime data or false zero-filled charts. Synthetic SQLite
fixtures cover process generations, descendants, missing markers and ambiguous
bounds; installed real trace-processor and live replay capture checks passed.

The collector trait now provides optional execution-namespace observation
hooks. Memory/worker collectors sample beside the selected guest client using
the same prepared entry-point identity as native execution, respecting its
PATH and cwd. Only versioned bounded scalar aggregates are imported; no argv,
prompts, keys or transcripts cross this observation interface. Missing or
invalid observations remain unavailable. The host never guesses guest paths
through elevated `/proc` access. Real namespace sampling checks passed for
SWE-bench Lite and both original Terminal-Bench versions, using Pi with an
explicit synthetic provider and known repair fixture, not paid inference.

Preparation/collector attachment has a separate deadline from agent execution.
The execution budget starts on resume; the explicit `execution_ended` event
stops accounting before metadata transfer, relay teardown and official grading.
Actual Terminal-Bench 1.0 with namespace collectors and Perfetto passed in
58.0 seconds after this timing repair. Lifecycle artifacts showed preparation
ending before execution began and execution ending before grading. A normal
single-configuration sweep replay then passed in 40.2 seconds with official
grading, no provider/catalog requests, and separate memory, worker and
container-CPU SVGs included in its HTML comparison report.

Failure paths now explicitly cancel and drain in-flight output/error/result
events until the owner acknowledges cleanup and exits, with a 240-second bound.
Cancellation may exit nonzero, but cannot pass without removed/not-created
resource evidence. Startup/execution preserve their original errors and append
cleanup failures. A prepared session dropped during provider/proxy setup also
attempts bounded owner cleanup. Tests cover teardown beyond three seconds,
in-flight events, cancellation deadlines, failed/missing acknowledgements,
startup with no created resource and disconnect while paused for attachment.
Abrupt-controller-crash recovery is not claimed by these cooperative checks.

Inspection also found that replica handles were consumed from a lazy spawn
iterator, joining each replica before the next was started. All handles are
now collected before joining. A provider-free two-replica barrier requires
actual overlap, rather than inferring concurrency from isolated output paths.
This fixes replica execution but does not replace distinct-task scheduling.

Still required: full selected-population planning, task parallelism/resume and
complete-denominator reporting; agent background-process quiescence before
submission capture; SWE-bench source-image fallback; abrupt-crash recovery;
final review/PR merge/exact-main CI and the minor release. Qualification above
covers representative tasks, not complete-population benchmark scores.

Local development gates for this follow-up passed: 359 unit tests, 83 offline
CLI tests (including the overlap barrier), all ten Python protocol suites,
diagram/guest/replay tests, strict all-target/all-feature Clippy and `make build`.
One earlier run overlapped a Cargo binary replacement with executable-relaunch
tests and failed with ENOENT; the focused check and complete serialized gates
passed. Subsequent checks must not rebuild the same ASB executable while a
qualification using it is active. Packaging and exact-head hosted CI still
need their own evidence after the commit.

The follow-up was merged with main `d8de114` (lifecycle-total timing repair).
That exact branch passed 360 unit tests, 83 offline CLI tests, the protocol and
other integration suites, Clippy, full build, package verification and the
packaged crate's full test suite. Real Docker cancellation qualification also
passed disconnect, SIGTERM and disconnect-during-official-grading cases in
16.5 seconds, verifying absence of the owned task/grader containers.

An overlapping package build then reproduced a genuine executable-identity
edge case: `current_exe()` returned the deleted pathname of the invoking ASB,
so guest staging failed after preparation. The failed attempt's cleanup receipt
confirmed resource removal. Linux guest staging now uses the numeric host
process's `/proc/<pid>/exe`, and recursive native gates use `/proc/self/exe`,
retaining the actual running inode across pathname replacement. An offline
regression removes its own ASB executable copy after startup and requires
measured replay to complete. Explicit guest/runtime launcher overrides retain
their existing behavior. Serialized real namespace/Perfetto execution on the
main-refreshed branch passed again in 51.2 seconds; the additional kernel-path
repair still needs its own final local/hosted qualification.

### Quiescence before submission capture

The Linux guest driver and measured native gate now own exclusive subreaper
scopes. After the principal client exits, they verify direct child ownership
with `waitid(P_PIDFD)`, signal pinned pidfds rather than stale numeric PIDs,
and stop/reap adopted descendants, including double forks and helpers that
ignore TERM or hold output pipes. Cleanup has a 500 ms graceful period and a
five-second bound. A startup probe checks the required kernel APIs before
starting the client. The shared runner never becomes a child-owning supervisor;
the old unit check that changed the shared test process's subreaper state was
replaced by tests in actual short-lived supervisors.

The generic output waiter invokes cleanup as soon as the principal is reaped,
not after inherited output pipes close. Its failure/drop path also tears down
owned helpers. Only a quiesced guest writes completed metadata; nonzero client
exit codes are preserved. Missing/old/unconfirmed metadata instead triggers
an owned-container stop/wait/restart before partial submission evaluation.
This preserves the filesystem, does not turn process failure into success,
and records the cleanup source in `execution.json`. Failure to confirm a
stopped tree cannot proceed to grading. Task attachment and reset reject host
or shared container PID namespaces; no host process groups or unrelated
containers are signalled.

Provider-free tests cover process-list bounds, kernel refusal to signal a
non-child, double-forked helpers, ignored TERM, inherited/closed output streams,
zero/nonzero clients, consumer failure without completed metadata, unrelated
fixture survival, reset ordering and refusal, and failed reset acknowledgement.
Local gates passed 361 unit tests, 84 offline CLI tests, seven guest-driver
integration tests, all ten protocol suites, strict Clippy and the full build.
The first actual Docker supervisor qualification passed in 6.5 seconds and a
real Pi/synthetic-provider SWE-bench Lite run with official grading passed in
59.7 seconds. The final reset/metadata additions still need their own Docker,
packaging and hosted exact-head checks before this checkpoint is qualified.

This completes the normal/client-error pre-submission quiescence mechanism,
not abrupt-supervisor/host-crash recovery. Full-population planning, distinct
task scheduling/resume and coverage reporting, SWE source-image fallback,
final review/merge/main CI and release `0.2.0` remain open.

Final actual Docker qualification of the reset/acknowledgement additions
passed in 9.8 seconds: the normal supervisor preserved its namespace peer,
then the explicit unconfirmed-agent reset stopped that old peer exec and
preserved an identical submission hash. Real Pi with the synthetic provider
and official Terminal-Bench 2.1 grading passed in 49.5 seconds. Terminal-Bench
1.0 sweep replay passed in 38.1 seconds with no provider/catalog calls and the
expected SVG/HTML comparison artifacts. These are representative original
task qualifications, not complete-suite or AI-solving claims.

A final ordinary-suite run exposed a transient `ETXTBSY` when the executable
replacement fixture started its freshly copied binary while another test had
forked with a briefly inherited writer. That fixture now retries only that
specific exec failure within two seconds, without retrying benchmark results.
A separate Codex record fixture failed once without a cassette; diagnostic
coverage now retains its fixture run log. Its cause was not reproduced and is
not claimed repaired. The complete offline CLI suite then passed a focused
rerun and three additional concurrent stress runs (84 tests each).

The quiescence checkpoint `a4b73f53f8e53f7c281b853ae89d5d9143f2b3ea`
passed package verification and the packaged crate's full tests. GitHub
recognized its signature as valid, and exact-head hosted CI `39452622263`
completed successfully, including package/install smoke checks. The optional
local-host job was skipped. This is branch evidence, not main/release evidence.

### Full-population manifest foundation

Setup, sweep and compare now share a manifest type/reader. The old 256-entry
and 64 KiB limits could not represent even Lite's 300 tasks, let alone Full's
2,294. The shared bounds are now 65,536 configurations and 16 MiB; individual
configuration and prompt bounds remain unchanged. The reader bounds actual
reads, validates every reference path before execution, and rejects unknown
manifest fields and excessive entry counts. Setup resolves trial selections
once and checks its complete cross product before creating any output files.

Provider-free fixtures cover Full/Lite/Verified population sizes and all five
populations across eight agents. A complete setup-to-compare fixture writes
2,294 distinct task configurations, checks each selection and an over-64-KiB
manifest, then verifies that an empty-result comparison retains every selected
row. It also rejects an oversized setup without publishing partial config
files. That fixture completed in 0.45 seconds locally. Its instance IDs are
synthetic: this proves manifest/setup/report-census capacity, not official task
grading or complete original-suite execution. Distinct-task scheduling,
content-bound resume, planned dataset identities, coverage denominators and
scalable aggregate report presentation remain necessary.

The final manifest/source checks passed 365 unit tests, 84 offline CLI tests,
all ten protocol suites, seven guest-driver checks and the remaining graph/
replay integration tests, with strict all-target/all-feature Clippy. Missing
later references are now rejected before creating any sweep result directory,
not after earlier entries have already executed. Final package and exact-head
hosted CI qualification remain separate from these local checks.

The manifest checkpoint `35321c6625613b4999d008fd2edd5f3489131648` passed
full development build, clean package verification and the packaged crate's
complete suite. GitHub recognized its signature, and exact-head hosted CI
`39453938913` completed successfully (optional local-host job skipped).

### Pre-execution selections, fresh attempts and coverage

`WorkloadAdapter` now exposes an optional read-only planning catalog. The three
SWE registrations and both original Terminal-Bench registrations provide
their installed public task IDs, pinned dataset identity and expected evaluator
scope through that interface. The generic sweep planner reads each catalog
once per workload, validates selected instances/trial sets and snapshots the
normalized configuration and `TaskSelection` before any agent/provider starts.
This selection has no invented attempt ID or baseline. Aggregate configuration
text is bounded at 32 MiB and the published private snapshot at 64 MiB.

Every normal sweep attempt preserves prior results and comparison artifacts in
a run-scoped history directory instead of reusing dirty workspaces or stale
measurements. The archive preserves the original relative result/report layout,
without rewriting historical evidence. Default replay inputs are hard-linked
from preserved recordings; repeated default replays and explicitly selected
roots inside the old results remain usable without copying payload bytes.
The eight-replica offline integration covers repeated fresh replays and checks
stable recording inode identity, successful new task baselines and mode labels.

Compare loads the saved selection without requiring an installed/live SDK or
provider. It checks current configuration semantics and actual task/dataset,
grader and unique attempt identities. Missing preparation/evaluation stays in
the requested denominator, but creates neither an actual attempt nor a grade.
JSON/HTML expose evaluation coverage; per-population summaries distinguish
unique tasks, attempts, coverage and resolution lower bounds. A complete
population rate is emitted only for a full pinned population with all selected
evaluations and exactly one attempt per task, not for subsets or repetitions.
Live/record/replay mode and replay timing choice are also preserved; legacy
mode/selection evidence is unknown rather than guessed.

Provider-free tests cover cached catalog discovery, both original Terminal
registrations, source changes/malformed snapshots, failed preparation inventory,
partial/complete populations, duplicate attempts and incompatible graders,
replicas, history/symlink safety, archived report links and replay continuity.
These changes do not yet implement a content-bound resume cache, distinct-task
parallel scheduler, scalable population graph aggregation, SWE source-image
fallback or abrupt-crash recovery. Final PR merge/main CI/release remain open.

Sweep writers now hold a per-output OS lock from planning through comparison,
before moving prior results. The lock file stays outside archived output and
is never removed/recreated. Tests cover same-output exclusion, independent
outputs, non-truncation, invalid paths, abrupt writer exit, and actual CLI
rejection without changing prior results/reports. Kernel lock release is not
claimed as recovery of containers orphaned by abrupt supervisor termination.

Representative actual sweep qualifications passed using the real Pi client
and known deterministic repairs, without a live LLM provider: Terminal-Bench
1.0 replay completed in 39.8 seconds with zero provider/catalog calls, and
SWE-bench Lite standard execution with a synthetic provider in 58.8 seconds.
Both used official grading and namespace samplers, retained selected/population
coverage (1/80 and 1/300), and left the full-population score unavailable. The
HTML now also presents population/evaluation coverage directly in a table.

The final selection/lock qualification passed 378 unit tests, 85 offline CLI
tests, all ten protocol suites, seven guest-driver checks and the remaining
diagram/runtime/replay integration tests, with strict all-target/all-feature
Clippy and `make build` (Rustdoc, diagrams and completions). A final actual
Terminal-Bench 1.0 sweep replay passed in 40.1 seconds after adding the lock,
with the same official grading/coverage checks and zero live provider calls.
Package verification and exact-head hosted CI remain separate gates.

The selection checkpoint `788068d7b741b9dc10d91ad07fb0cfddc41802c5` passed
clean package verification and the packaged complete test suite. GitHub
recognized its signature, and exact-head hosted CI `39461383735` succeeded,
including packaged tests and installation/smoke checks (local-host job skipped).

### Original SWE-bench source-image fallback

The shared SWE family installer now downloads the original task-source archive
at `SWE-bench/swe-bench-tasks@3d07b464b7b311a0cbfb5ed5b2d8a3b96f84a33d`,
SHA256 `ff31aa1f8d2bb78f16559524264a99b868b7218ca82b390dc750bbc59538aa20`.
Bounded authenticated archive reads do not extract tar members into the host
filesystem. Installation compares every Full/Lite/Verified task against its
frozen dataset: repository/base/image identity, setup metadata, problem text,
reference/test patches, evaluation script and both official test lists. The
three actual workload-install commands passed for 2,294/300/500 instances and
installed the shared archive project-locally. Invalid archives are preserved
before replacement; symlink/directory targets are refused.

Task preparation prefers a local image, then its published registry image.
Registry/network pull failures can build the exact original Dockerfile using
scoped Docker Buildx, with 2-CPU/8-GiB per-build limits and a 600-second deadline.
At this source revision all original Dockerfiles are self-contained. The
private build context therefore contains only the Dockerfile, never gold/test
data, SDK metadata, agent configuration or other host files. No source recipe
is rewritten. Build receipts retain source/Dockerfile/image identities, limits
and failure classes. A unique ASB-labeled tag is removed with `noprune` after
grading or a failed build; published images/shared caches are never pruned.

Provider-free tests cover archive identity/path/type/size bounds, missing or
mismatched original task data, context privacy, prebuilt/local/pull preference,
fallback selection, timeout/cancellation/nonzero build outcomes, tag ownership,
source-pin consistency and backup behavior. A full-suite run also exposed an
OS-lock release race while other test threads forked. Explicit guard unlock
now releases ownership even while an inherited/duplicated descriptor remains
open; the regression retains that descriptor instead of hiding the race with
a retry. Two additional unit-suite stress runs passed before the final gates.

Actual original-source qualification forced only this fixture's prebuilt
lookup/pull to fail; it did not remove any shared image. It built the unmodified
original `sympy__sympy-20590` Dockerfile, accepted a known reference repair under
all three official population graders, rejected an incorrect submission, and
verified removal of the owned source tag and preservation of the published
image. First qualification took 408.6 seconds; a final-code cached rerun from
the installed source archive passed in 33.9 seconds. Neither run used an agent
or an LLM provider, and neither is a complete-population/AI-solving claim.

The larger Rust graph exposed Graphviz 2.43 rank failures/timeouts in the build
gate. Function-flow edges now use visible external labels rather than adding
synthetic label nodes to rank assignment. Every function, relation, call rank
constraint and visible label is preserved and counted by the regression;
there is no graph pruning, skipped gate or increased timeout. The complete
data/control graph probes then rendered in approximately 1.2 seconds each.

Distinct-task scheduling/resource admission, content-bound resume, scalable
aggregate population graphs and abrupt-crash recovery remain open, as do final
review, merge, exact-main CI and minor release. Final source-checkpoint package
and hosted CI qualification are recorded separately from these local results.

Final local source gates passed 381 unit tests, 85 offline CLI tests, all eleven
Python protocol suites (including seven source-contract cases), seven guest
driver checks and all remaining diagram/runtime/replay tests, strict Clippy,
and `make build` with Rustdoc/diagrams/completions. The full diagram regression
with node/edge/visible-label counts passed in 2.8 seconds. A final normal
SWE-bench Lite sweep using real Pi, a synthetic provider, official grading and
namespace collectors passed in 62.4 seconds, confirming that the source-helper
addition preserved the ordinary prebuilt-image client path. This uses a known
repair and does not claim an AI solution or a full-population run.

The source checkpoint `8c764a6d54f29bcc930f1b00cf654f76736d6832` passed
clean package verification and the packaged complete suite. GitHub recognized
its signature, and exact-head hosted CI `39464881901` succeeded, including
packaged tests and install/smoke checks (optional local-host job skipped).

### Bounded distinct-configuration scheduling

`asb sweep --jobs N` now selects concurrent configurations, independently from
each configuration's identical `concurrency` replicas. Jobs are bounded to
1..32 and default to 1. `--max-active-attempts N` bounds the sum of admitted
replicas (1..64), defaulting to max(jobs, largest selected concurrency). Each
configuration must fit. FIFO permits cover preparation/execution/grading/
cleanup, not just process launch, and worker count/event-channel capacity are
bounded. Invalid admission is rejected before moving prior results. Standalone
run/setup/configuration parsing also rejects replica counts above 64 before
allocating worker vectors or writing an unusable configuration. The count
budget is not a RAM reservation; environment resource limits remain owned by
the adapters and jobs must be chosen for host capacity.

Parallel output is kept in bounded private per-configuration temporary files,
then drained as complete configuration blocks instead of interleaving bytes.
Output failure stops admission and joins admitted workers; temporary duplicates
are removed without deleting original agent logs. Worker failures/panics release
slots and retain independent successful tasks and comparison denominators.
Sequential defaults retain immediate warnings and `--show-output`; a fixture
cannot finish until the parent observes its streamed line and acknowledges it.
Raw `--show-output` conflicts with `--json` at parsing rather than contaminating
JSONL. Compact JSON records now include agent/provider/model/workload names and
task/attempt identity, so completion-order output remains identifiable. Shared
collector dependency publication is serialized without serializing execution.

Scheduling is persisted in selection snapshots, checked on comparison load,
and disclosed in JSON/HTML because concurrent tasks share host resources.
Legacy snapshots omit scheduling rather than inventing it. Shell completions
and command help expose the new options. Provider-free tests cover overlapping
execution and replica-weighted peaks, queue completion, failure/panic slot
release, output failure/draining, private bounded non-overwriting spools,
invalid snapshots/CLI options, preservation of prior outputs on invalid
admission, and sequential streaming. A loopback-replay integration runs two
distinct local tasks with three simultaneous clients (two replicas plus one
independent task), validates JSONL and reports, then fails the first two
clients while retaining the independent task's successful grade.

An opt-in actual mixed-original-task test runs real Pi in SWE-bench Lite's
`astropy__astropy-12907` and original Terminal-Bench 1.0's `hello-world`.
A synthetic provider barrier requires both distinct clients to reach it before
either can receive the deterministic known repair. It validates official
grades, distinct scoped collector targets/cgroups, lifecycle ordering,
collector graphs, coverage and removal of the owned task containers. The
first run reached successful official grades but exposed a fixture assertion
that expected workload names in the old compact summary. The corrected full
qualification passed in 55.3 seconds. No live LLM provider is contacted, and
this is not an AI-solving or complete-population result.

Content-bound resume/cache identities, scalable population presentation and
abrupt-crash recovery remain unfinished, along with final full-diff review,
PR merge, exact-main CI and the requested minor release. Scheduling does not
silently satisfy those separate requirements.

The final scheduling source gates passed 390 unit tests, 86 offline CLI tests,
eleven protocol suites, seven guest-driver checks, the parallel integration
(including failure isolation and sequential streaming), existing replay/graph
tests, strict all-target/all-feature Clippy and `make build`. After adding
compact JSON identity fields, the actual mixed-original-task qualification
passed again in 65.9 seconds with independent grades, scoped collectors,
graphs and cleanup. Package/main-sync/hosted CI checks follow separately.

Scheduling implementation `e180246` was merged with current main
`ff5d58d515622b08d567d468fd7f0bb72a7cac0c` (PR #240's partial-replay token
repair), producing `eb88d6654b8e0132beee192fa3e9d541245bda7b`. The combined
tree passed strict Clippy, 390 unit tests, 87 offline CLI tests, all protocol/
runtime/parallel/replay/graph tests and `make build`. The extra main regression
ensures an unconsumed replay cannot report the full cassette's token totals.

### Private input stability and cleanup-confirmed outcomes

Sweep now records optional before/after input stability separately from task
outcomes. The private key is generated once per output tree with mode 0600;
fingerprints bind file contents, modes, missing inputs, symlink targets,
project configuration, installed workload/tool trees, project Python, saved
selection plan, actual running executable and inherited environment values.
Only keyed opaque digests cross the helper boundary. Reading the executable
through its kernel file handle works after its pathname is replaced/unlinked;
process IDs and handle paths are not software identity. Resource limits and
mutation checks reject changing, cyclic, special-file or excessive inputs.
The helper opens ordinary files nonblocking without following a replacement
symlink, so substitution with a FIFO cannot stall a sweep. Length-framed
context prevents ambiguous concatenations from sharing a digest.

After workload cleanup and measurement finalization, private completion
records preserve every available evaluator outcome, including unresolved
attempts. They retain actual task identity, cleanup confirmation and
adapter-declared evaluator artifact roots, without duplicating stdout/stderr.
Per-replica records use independent atomic filenames. A failed cleanup remains
explicitly unconfirmed; optional publication failure never changes the grade.
Provider-free unit and loopback-replay tests cover unresolved outcomes,
distinct replicas, private modes, symlink refusal, input changes, unavailable
seals, secret-free responses and executable-handle stability.

These are explicitly labelled foundations, not resume receipts. No completed
task is skipped or retried by this change. Content-bound evidence sealing,
host runtime/proxy/cassette closure, actual continuation dispatch and recovery
remain required before `--resume` can be implemented honestly. Scalable
population reporting, abrupt-crash recovery, final review/merge and release
also remain open. Qualification results for this checkpoint follow below.

The final source passed strict all-target/all-feature Clippy, 395 unit tests
(six opt-in tests ignored), 87 offline CLI tests, all twelve protocol suites
(including ten fingerprint cases), seven guest-driver checks, replay/parallel/
diagram regressions and `make build`. The actual mixed SWE-bench Lite/original
Terminal-Bench 1.0 qualification passed in 77.3 seconds. It checked both real
Pi executions against the synthetic provider, official grades, collectors,
container removal and each task's private outcome record/evaluator evidence
root after cleanup. Retained artifacts are under
`target/benchmark-runtime/parallel-original-tasks-ht5bcxt0`; this remains a
representative known-repair test, not a complete-population or AI-solving claim.
The actual input seal covered approximately 3.47 GB and 232,293 nodes. This
conservative whole-tool-tree approach needs more precise adapter-declared input
closure before scalable resume is complete. Package/hosted CI checks follow.

A follow-up review repaired explicit file override identity: retain the
declared symlink rather than canonicalizing away retargeting. The Python
reader likewise preserves parent components after symlinks instead of using
lexical `abspath`, which can read a different file than the kernel would.
Two additional regressions verify both cases without providers or credentials.
The parallel loopback replay fixture additionally retargets an explicit
file-valued environment override during execution, verifies a `changed` seal
while its original target remains untouched, and still preserves the independent
successful task and both unresolved replicas. All source/build gates passed
again after the repair. Checkpoint commit `b4860e3032cafaa0c702ae4b7eb4298a6e5e3c71`
passed clean packaging, the full packaged suite and hosted CI `39474879476`
(optional local-host job skipped); GitHub verified its signature.

### Content-bound evaluator outcomes

Checkpoint continuation now has per-configuration outcome/evidence seals under
`.checkpoint/completed/`. Before publication, every expected replica must have
a unique actual task/attempt identity matching its selected dataset, agent,
provider, model and grader; a boolean resolved/unresolved grade; supported
baseline evidence; and cleanup confirmation. Dataset attempts must retain
evaluator evidence. Enabled measurement records must match each unique outcome,
including grade, provenance, process status, timing and reported usage. Wrong,
missing, duplicated or mismatched records cannot become sealed completions.

The keyed digest covers the entire configuration result tree and each
adapter-owned evaluator root, bound to the planned entry and original input
digest. Temporary terminal spools moved outside task result trees; there are
no filename exclusions from the evidence hash. Receipt publication uses an
atomic non-replacing link and syncs the containing directory on Unix. Existing
receipts can be verified but never overwritten: changing evidence, results,
selection, input identity or the receipt itself is rejected. Optional seal
failure does not change a grade or block execution. No historical failed grade
is treated as a request to retry it.

Six provider-free unit checks cover true/false outcomes, metadata/privacy,
result/evaluator mutation, non-replacement, selection/input binding, wrong or
incomplete populations, record bounds/symlinks and measurement consistency.
The parallel loopback replay integration now requires seals for both failed
replicas as well as the independent successful task and checks that all
temporary output spools are removed. The first full suite exposed a raw-file
lock fixture race when another test forked before descriptor close; the product
guard already explicitly unlocks, and the fixture now does the same rather
than relying on close of every inherited descriptor. The focused check and
complete suite passed after repair.

The full source gates passed 401 unit tests (six opt-in ignored), 87 offline
CLI tests, all twelve protocol suites, seven guest-driver checks, existing
parallel/replay/diagram regressions, strict Clippy and `make build`. Live
container, package and hosted qualification are recorded below when complete.
These seals are deliberately outcome-only: complete input closure, interrupted
replica handling and actual resume dispatch remain open, as do scalable
population presentation, abrupt-crash recovery and final delivery/release.

The mixed actual SWE-bench Lite/original Terminal-Bench 1.0 qualification
passed in 78.1 seconds, now requiring both keyed result/evaluator seals,
matching outcome/measurement identities, official grades, scoped collectors
and container/spool cleanup. The input seal remained stable. Its two outcome
seals bound approximately 817 KB and 136 KB respectively, including the
workload-owned official evaluator roots; no whole SDK/client installation is
duplicated into those output hashes. Artifacts are retained under
`target/benchmark-runtime/parallel-original-tasks-zhm1uyr7`. As before, these
are known-repair/synthetic-provider qualifications, not full-population or
autonomous AI-solving claims. Previous checkpoint `4ef04e6` also passed its
exact-head hosted CI `39475840348` (optional local-host job skipped).

### Proxy and cassette input/output identity

Record/replay input seals now ask the selected proxy adapters for their owned
execution-file declarations. The embedded tape adapter declares the same host
Node command or explicit override that startup selects, without starting a
proxy or creating scripts/scratch files. Shared executable lookup preserves
PATH order, access modes and symlink identities rather than canonicalizing
away retargeting. Source and override selection remain inside the adapter;
sweep/checkpoint code has no proxy/tool-name branches. These declarations do
not yet claim transitive interpreter/shared-library closure.

Replay input bindings now cover the physical source cassette directories for
each selected configuration, independently of its fresh workspace/output tree.
The source mapping shares the execution path naming and correctly points into
preserved history when defaults or an explicit old-result root are replayed.
The loopback integration mutates a source cassette during a sweep, independently
of environment-file mutation, and requires a changed input seal. Repeated
default/old-root replay checks require stable seals and history-backed inputs.
Record outputs are not accidentally hashed as inputs.

Proxy completion now exposes persistent output paths and resource-cleanup
confirmation independently from request/completion validation. A rejected
replay may still have a cleanly stopped process and a valid failed task grade.
The runner records both facts without choosing an adapter. Outcome sealing
requires both workload and proxy teardown confirmation and includes external
recording outputs, even when a failed agent created no file; later appearance
or modification invalidates that observed outcome. Old records lacking a proxy
cleanup acknowledgement do not become eligible by default. Tests cover those
states and ensure raw recording bytes never appear in a public digest.

The pure fingerprint helper permits up to 100,000 root descriptions within
its existing request, node, byte and time budgets, instead of rejecting every
manifest with more than 10,000 cassette-source declarations. A provider-free
65,536-root regression completes without reading any payload bytes and still
rejects oversized root populations. This is format/cardinality qualification,
not a claim to have evaluated 65,536 tasks or removed resource limits.

Complete host runtime/library input closure, interrupted-replica handling and
actual resume dispatch remain required. Scalable population presentation,
abrupt-crash recovery, full review/merge, exact-main CI and the minor release
also remain open. Previous outcome checkpoint `489982e` passed exact-head CI
`39477763826` (optional local-host job skipped). New gate/runtime results follow.

Proxy/cassette implementation `e895d83d` was merged with main `4d765890`
(PR #241's child-owned built-in proxy readiness), producing `0cb5927b`.
The constructor conflict retained the upstream ephemeral-port/readiness logic
and this branch's separate cleanup acknowledgement. The combined tree passed
strict Clippy, 408 unit tests (six opt-in ignored), 87 offline CLI tests, all
twelve protocol suites (including thirteen fingerprint cases), seven guest
driver checks, replay/parallel/diagram regressions and `make build`.
Exact-head hosted CI `39480823032` passed, including packaged tests and
install/smoke qualification; the optional local-host job was skipped.

Actual normal SWE-bench Lite sweep record and replay qualification passed on
the combined code in 67.9 and 66.6 seconds. It ran real Pi against the synthetic
provider, applied the known repair and used official grading, namespace
collectors and comparison graphs. Record asserted that its external cassette
was bound as an output and not a replay input. Replay asserted a source binding,
stable input identity and zero provider/catalog requests; both required sealed
outcomes and original evaluator evidence/cleanup. Retained artifacts are under
`target/benchmark-runtime/normal-cli-guest-5kq6s168` (record) and
`target/benchmark-runtime/normal-cli-guest-8si_lzkd` (replay). These remain
representative known-repair checks, not full-population or AI-solving claims.

### Adapter-owned runtime inputs and execution dependency preflight

Runtime declarations now live behind the agent, provider, workload, tool,
collector and proxy interfaces. Generic sweep code requests those declarations
without component-name dispatch. Native programs bind their matching loader
and shared libraries; Python declarations bind the selected stdlib/venv and
native extension dependency roots. Discovery parses PyInit exports rather than
loading modules or treating every bundled wheel DSO as an independent program.
It preserves logical PATH lookup, symlinks and missing configuration paths,
and checks that the invoking ASB's mapped native libraries still identify the
same files. A second discovery before the final snapshot detects changed
resolution. Bounded discovery runs in its own process group; cancellation
stops only that group. Private errors do not echo paths, environment values or
interpreter output. This remains declared-input stability, not whole-host
immutability or automatic coverage of arbitrary application dlopen/data files.
Failure to obtain this optional seal never blocks a fresh execution.

The original workload adapters also implement a separate execution preflight.
It publishes the compiled SDK helpers and prefetches selected guest runtime
archives through the existing owning installer recipes before any environment
starts. Sweeps deduplicate project/workload/agent combinations and perform
this before moving prior output and taking input snapshots. Runtime metadata
discovery itself remains read-only and never downloads. Missing evaluator
installations identify the exact `asb workload install <name>` repair command.
Identical helper publication retains private regular files without changing
their inode; changed files and destination symlinks are atomically replaced,
leaving symlink targets untouched.

New provider-free regressions cover actual native loader/RPATH resolution
without executing the application, shared-library content changes, earlier
PATH candidates, bounded native-symbol parsing, error privacy, owned-process
group cancellation, idempotent private helper publication, and preflight
dispatch/read-only metadata for all five original suite registrations. Local
strict Clippy and the full source suite passed with 413 unit tests, 87 offline
CLI tests, thirteen protocol suites and the existing guest/replay regressions.
The mixed real-Pi/SWE-bench Lite/Terminal-Bench 1.0 synthetic-provider
qualification passed in 75.2 seconds before the final preflight addition,
including unchanged runtime input identity, separate official grades,
collector targets and complete container cleanup. Qualification of the final
preflight tree and packaged/hosted gates follows.

The final preflight tree passed the strengthened mixed original-task
qualification in 65.8 seconds, retaining artifacts under
`target/benchmark-runtime/parallel-original-tasks-jf44lljv`. It explicitly
requires identical before/after input fingerprints and discovered native
library roots in addition to the official-grade, overlap, graph and cleanup
checks. This uses warm installed runtime caches; it is not a cold-download
qualification or an evaluation of the full populations.

Actual resume dispatch, interrupted replica handling, scalable population
presentation, abrupt-crash recovery, final review/merge, exact-main CI and the
minor release remain open; these changes do not close those delivery gates.

### Completed-group continuation dispatch

`asb sweep --resume` now consumes the original plan and validates the selected
dataset/task population, execution mode, replay timing and scheduling. It
recomputes declared input identity with the original private key, then checks
every reusable group's outcome/measurement/evaluator/proxy evidence before
admitting any work. Neither input-only nor outcome-only seals authorize reuse
on their own. A missing original key is never silently regenerated. Missing,
changed, malformed or unbounded input/evidence rejects continuation without
replacing the existing results or input seal. Fresh execution remains available
without these optional continuation requirements.

Validated completed groups return their original summary and attempt IDs
without starting agents, providers, proxies or workload environments. JSONL
adds `reused: true`; human output identifies reuse explicitly. A false task
grade or failed client remains a failed result/command, not an invitation to
retry until it passes. Original measurements/logs are not rewritten and the
comparison continues to count the original attempts once. Configurations with
no result directory can be admitted normally. A pending initial seal left by a
stopped owner requires the same current inputs and complete evidence; process
exit alone is never completion evidence.

An existing unsealed or partially completed replica group is currently rejected
intact before scheduling. Per-replica continuation and owned-resource recovery
must still be added; the current option is not an abrupt-crash recovery promise.
Runtime identity is the declared component/file/environment graph, not a claim
of whole-host immutability or arbitrary dynamic application input coverage.

Provider-free tests cover unchanged eight-replica reuse with no new agent
launches, preserved logs/attempts, changed executable rejection, missing
checkpoint keys, pending input seals and unsealed-group rejection. Unit cases
preserve false grades/client failures, output errors and safe one-line human
presentation. Replay tests also check preserved-history/default cassette paths.
The source suite passed 416 unit tests, 87 offline CLI tests, thirteen protocol
suites, guest/replay checks, strict Clippy and `make build`; final package and
hosted gates follow.

The actual mixed original SWE-bench Lite/Terminal-Bench 1.0 qualification passed
in 79.6 seconds with real Pi and a synthetic loopback provider. It then resumed
both configurations, required original task/attempt identities and official
grades, and observed no additional completion requests. Existing scoped
collector/graph/cleanup checks still passed. Artifacts are retained under
`target/benchmark-runtime/parallel-original-tasks-d03_v9om`. This is a warm-cache
representative known-repair qualification, not full-population AI solving.

Per-replica interrupted-group recovery, scalable population presentation,
abrupt-crash recovery, cold-cache qualification, final review/merge,
exact-main CI and the minor release remain open.

### Stable replica slots and independently published completion evidence

The runner now assigns a one-based `(index,total)` identity to every replica.
Its shared configuration helper owns workspace/log/measurement suffixes and
native runtime paths; checkpoint validation uses that same helper rather than
inferring a slot from completion order or an agent's output. Private outcome
schema 2 records this identity and the actual execution-output paths. Existing
schema-1 complete-group evidence remains readable, while independent replica
sealing requires the explicit new identity and paths.

A generic optional completion observer runs after grading, workload/proxy
cleanup and measurement completion, before waiting for sibling replicas. It
publishes immutable private receipts under
`.checkpoint/replicas/<configuration>/<replica>.json`, bound to the original
declared-input digest, selection, outcome, generated configuration, own
workspace/log/runtime/measurements and external grader/proxy evidence. A
false grade is completed evidence too. Sibling output mutation does not
invalidate that replica; edits to its own output or grader evidence do. Missing
cleanup, wrong slots/populations, omitted/shared output paths and measurement
slot mismatches cannot earn a receipt. Per-configuration publication is
serialized without serializing execution; metadata failure never changes a
fresh task's grade. Outcome publication also synchronizes its directory before
notifying the observer.

Resume can recover the window where every replica was sealed but the owner
stopped before publishing its final group receipt. It validates every selected
replica and the complete outcome population, preserves original attempts and
grades, and neither retries them nor republishes a group receipt during
validation. Missing/unsealed genuinely unfinished slots remain a recovery
requirement; no partial-slot execution or abrupt-crash resource cleanup is
claimed by this increment.

Provider-free tests cover bounded slot identity, canonical output naming,
duplicate-slot rejection, false-grade evidence before sibling completion,
independent sibling mutation, immutable receipt publication and own-evidence
mutation. A real loopback replay barrier prevents replica 2 from finishing
until replica 1's receipt exists, proving publication precedes group completion.
Eight-replica replay checks individual receipts, recovery without a final group
receipt, no new agent launches, and rejection when one replica seal is missing.
Strict Clippy, 419 unit tests, 87 offline CLI tests, thirteen protocol suites,
guest/replay regressions and `make build` passed.

The actual real-Pi/original SWE-bench Lite/Terminal-Bench 1.0 synthetic-provider
qualification passed in 79.0 seconds, including new private slot/receipt checks,
official grades, completed-group resume, original attempt identity, no extra
completion requests, scoped collectors/graphs and cleanup. Artifacts remain at
`target/benchmark-runtime/parallel-original-tasks-dlv1nuqt`. This is warm-cache
representative known-repair qualification, not full-population AI solving.
Package and hosted gates follow.

Actual continuation of unfinished replica slots and owned-resource crash
recovery, population-scale presentation, cold-cache qualification, final
review/merge, exact-main CI and the minor release remain open.

### Partial-group dispatch with positive never-started reservations

Continuation now distinguishes complete groups, individually graded slots and
positively reserved slots that have never started. The owner publishes all
slot reservations before beginning a configuration. A starting hook first
publishes execution intent, then consumes and synchronizes the matching
reservation before measurement/provider/workload startup. A missing start file
alone never proves a slot was unused: deleting a consumed start record leaves
no positive reservation and cannot admit another attempt. Reservation identity
includes the selected slot/population, original input digest and configuration.

Before any resumed work is scheduled, every existing graded slot must retain
its validated receipt and matching outcome. Every pending slot must retain its
original reservation, have no start intent or graded-but-unsealed outcome, and
have no workspace/log/runtime/measurement output. Already-started unsealed
slots remain rejected pending owned-resource recovery; this increment does not
claim to clean up crashed containers or native processes. A start-intent I/O
failure stops the attempt before execution. Completion-evidence failure still
never changes a computed grade, and missing optional input fingerprints never
block a fresh non-resume sweep.

The generic runner skips cached slots while preserving their original indices,
false grades and failed-client status, and executes only admitted remaining
slots. It does not silently renumber a resumed slot or repeat a graded failure.
Identical isolated configuration files are not rewritten. Existing default
replay recordings must remain the original preserved hard links. Interrupted
parallel presentation spools are privately archived rather than overwritten;
they are not grading evidence. Actual admitted slots are still conservatively
charged against the original selected replica budget.

New unit tests cover reservation consumption, missing/changed/deleted intent
state, existing output rejection, failure before provider/workload/measurement
startup and preserved bounded terminal spools. The new provider-free
`partial_sweep` fault-injection test uses real loopback tape transport and three
fake-agent replicas. One client exits zero without fixing its task. After
restoring a controlled pre-start state for slot 2, continuation preserves the
two original graded slots (including the false grade), launches only slot 2,
retains old evidence, preserves original replica numbering, and reports three
attempts rather than four. It also refuses the same slot while its previous
start intent remains. This is controlled state-machine fault injection after
fixture processes stopped, not live crashed-container recovery.

The source suite passed strict Clippy, 423 unit tests, 87 offline CLI tests,
thirteen protocol suites, guest/replay/partial-sweep regressions and `make
build`. A diagram timeout was traced to a test-only `State::is_empty` helper:
the heuristic call extractor incorrectly treated ordinary collection calls as
calls to that helper. Renaming the helper removed the false global hub; all
nodes/relations and existing rendering timeouts remain unchanged, and the
four diagram regressions passed in 3.2 seconds.

The actual mixed real-Pi/original SWE-bench Lite/Terminal-Bench 1.0
synthetic-provider qualification passed in 84.2 seconds. It additionally checks
matching private start intents and consumed reservations, alongside official
grades, original attempt identity, group resume without extra completion
requests, scoped collectors/graphs and cleanup. Artifacts remain at
`target/benchmark-runtime/parallel-original-tasks-erb0w9rm`. This is warm-cache
representative known-repair qualification, not full-population AI solving.
Package and hosted gates follow.

Remaining work includes owned-resource recovery for already-started unfinished
slots, explicit execution-cohort/admitted-slot disclosure in partial-sweep
comparison reports (fresh remaining slots do not recreate the old concurrent
load), population-scale presentation, cold-cache qualification, final review/
merge, exact-main CI and the minor release. The original selected concurrency
must not be presented as proof that restored and newly executed measurements
were collected simultaneously.

### Dispatch context for resumed measurements

Run summaries, logs, measurement files and comparison JSON now preserve an
explicit execution cohort and original replica index. One fresh dispatch
records its selected replica count and ordered admitted slots. Partial resume
retains cached contexts unchanged and assigns only newly executed slots a new
cohort; the selected concurrency is never rewritten to imply the old load was
recreated. HTML gives cohorts local group numbers and explicitly states that
dispatch membership is not proof of exact temporal overlap. Legacy measurement
context stays unknown. These metadata fields are excluded from numeric charts.

Pre-start intents carry the context, and individual outcome receipts bind that
intent as well as the matching measurement and summary. Validation rejects
out-of-range, unsorted, duplicate, malformed or contradictory declarations.
The provider-free partial-sweep regression proves original three-slot context,
new slot-2-only context, preserved false grade and identities, correct JSON/HTML
disclosure, and refusal after measurement or intent tampering without launching
another client.

Source verification passed strict Clippy, 425 unit tests, 87 offline CLI tests,
all thirteen benchmark protocol suites, guest/diagram/replay/partial-resume
regressions and `make build`. Actual upstream runtime, packaging and hosted
qualification results are recorded separately rather than inferred from those
offline checks.

Actual mixed original SWE-bench Lite/Terminal-Bench 1.0 qualification passed
in 84.8 seconds with real Pi and the synthetic provider. It verifies matching
pre-start and completion cohort metadata alongside official grading, scoped
collectors, report graphs, complete reuse without extra requests and container
cleanup. Artifacts are retained under
`target/benchmark-runtime/parallel-original-tasks-can2rb1_`. This remains a
warm-cache representative known-repair test, not full-population solving.

This closes dispatch-context disclosure, not the overall delivery checklist.
Owned-resource recovery for already-started unfinished attempts, scalable
population presentation, cold-cache qualification, final review/merge, exact
main CI and the minor release remain outstanding.

### Durable owned-resource recovery foundation

Task environments for all three SWE-bench populations, original Terminal-Bench
1.0 and original Terminal-Bench 2.1 now persist bounded resource declarations
before creation. A shared private journal binds exact container/network/volume
names to the unique attempt, a random ownership label and the selected Docker
daemon ID. Its OS lock rejects concurrent recovery. Compose expansion publishes
the labels and declarations before starting services. The installed support
helper can quiesce only those explicitly declared resources; all declarations
and observed ownership are validated before any stop operation.

Recovery stops containers by their resolved SDK identity and preserves their
files, all volumes and the journal. It performs no host-wide listing/pruning,
PID signalling, agent launch or grading. An absent resource is only a current
observation: an in-flight daemon operation could still finish later. The output
explicitly is not a retry receipt, and sweep continuation still refuses an
already-started unsealed slot. Source images, controller-to-slot recovery
binding, interrupted submission/evaluator handling, native/proxy process
ownership and final sweep reconciliation remain required work.

Official SWE-bench evaluator subprocesses also journal their container before
creation. The pinned SDK's pre-create removal is wrapped so a matching name
alone cannot remove another owner's container, and its conflict fallback cannot
silently choose an undeclared name. Parent cancellation/cleanup additionally
checks attempt labels rather than relying on the name alone.

Eleven provider-free journal checks cover active and SIGKILL-released locks,
private files, symlink/FIFO refusal, wrong daemons/tokens, duplicate/malformed/
oversized declarations, idempotent publication, preserved volumes and refusal
without partial mutation when any owner is wrong. A real Docker SIGKILL check
passed in 3.5 seconds: it stopped only the dead controller's container, read its
preserved partial file and verified an unrelated running sibling survived.
Artifacts: `target/benchmark-runtime/resource-recovery-bsq6_vrr`.

Actual task-environment/journal checks passed for Full/Lite/Verified in 19.0
seconds. Original Terminal-Bench 1.0 correct/incorrect official grading passed
in 36.7 seconds; original Terminal-Bench 2.1 passed in 38.3 seconds. All 89
installed 2.1 task definitions were additionally checked as single-step Linux
tasks, matching the existing session contract. The normal mixed real-Pi/
synthetic-provider SWE-bench Lite/Terminal-Bench 1.0 sweep, reuse, collectors,
graphs and cleanup passed in 76.8 seconds, retaining artifacts under
`target/benchmark-runtime/parallel-original-tasks-7yxo9hv6`.

These are warm-cache representative runtime qualifications, not a claim that
agents solved full populations or that resource quiescence completes crash
recovery. Final source/package/hosted gates and the separate six official
SWE-bench grader checks are recorded after completion.

All six official SWE-bench grader checks then passed in 211.9 seconds: known
gold resolved and deliberately incorrect patches remained unresolved for each
of Full/Lite/Verified, with scoped cleanup. Their artifacts are retained at
`target/benchmark-runtime/official-swebench-7jyj0bhg`. These use known submissions,
not LLM task solving. Final source gates passed 425 unit tests, 87 offline CLI
tests, all fourteen protocol suites, guest/diagram/parallel/replay/resume
regressions, strict Clippy and `make build`. Packaging and hosted gates follow.

### Sweep allocation binding and adapter-dispatched recovery

Official workload preparation now saves a private immutable reference before
controller launch: intended task/dataset, preparation identity and exact
project-local artifact directory. The workspace may be external; recovery
resolves the artifact-owning project rather than accidentally selecting the
workspace's project and SDK. A generic observer binds the reference to the
replica's existing start intent, configuration and original input digest under
`.checkpoint/allocated/`. Publication failure stops before a task controller
starts. Completion receipts seal that allocation as well as existing result
and evaluator evidence. Local/legacy sessions without allocations retain their
previous behavior.

The public `asb workload recover REFERENCE [--json]` command dispatches through
the workload trait; each original adapter chooses its own project-local SDK.
It validates bounded private reference files and project-local real artifact
paths, then consumes one bounded successful helper response. It does not select
an agent/provider, create a new attempt, grade a partial submission or authorize
a sweep retry. A no-op all-absent resource inspection leaves sealed evaluator
evidence unchanged. Help and both shell-completion catalogs include recovery.

New provider-free tests cover immutable/private references, traversal/symlink
rejection, publication failure before launch, exact selection/slot/input/intent
binding, changed metadata refusal and all five adapters' own SDK resolution,
including references stored inside a different project. The journal suite now
also proves that absent-resource observations do not create new evidence files.

An actual ASB CLI qualification passed in 3.8 seconds using a cached original
SWE-bench Lite environment and replay-only transport. It pinned and killed the
exact allocated controller child, invoked the public recovery command, read a
preserved partial file from the stopped container and verified that no new
attempt, submission or grade appeared. Artifacts are retained at
`target/benchmark-runtime/asb-controller-recovery-qmyuitf1`. This proves actual
controller-to-artifact discovery and CLI quiescence, not recovered grading or
complete interrupted-sweep reconciliation. Source/package/hosted and standard
mixed-sweep qualification gates are recorded after their completion.

The standard mixed original-task run first passed allocation-bound completion
and resume checks in 78.4 seconds. Final source verification passed 430 unit
tests, 87 offline CLI tests, all fourteen protocol suites, guest/diagram/
parallel/replay/resume regressions, strict Clippy and `make build`. Recovery
also consumes the owning project's proxy configuration, like ordinary launch.
The final actual controller-interruption/CLI check passed in 4.1 seconds at
`target/benchmark-runtime/asb-controller-recovery-606q8j29`.

An expanded mixed-task qualification then passed in 84.8 seconds at
`target/benchmark-runtime/parallel-original-tasks-p5j19rne`. Before resume it
invokes public CLI recovery against both already-cleaned-up attempts, observes
only absent resources, verifies that no new evaluator-evidence file is created,
then proves the original sealed attempts still resume with no extra provider
requests. It additionally checks the exact allocation/start-intent/receipt
and saved workspace-reference agreement. These remain warm-cache,
representative known-repair tests with a synthetic provider, not AI solving
or full-population evaluation. Package and hosted gates follow.

Remaining delivery work still includes interrupted outcome/evaluator handling,
native/proxy/source-image ownership recovery, full sweep-slot reconciliation,
population-scale presentation, cold-cache qualification, final review/PR merge,
exact-main CI and the new minor release. Allocation or quiescence records must
not be reinterpreted as grading evidence or permission to repeat failed tasks.

### Explicit retained-outcome receipt reconciliation

`asb sweep SWEEP.toml --resume --reconcile-completed` now repairs missing
replica/group receipts only for already-graded schema-2 outcomes with confirmed
cleanup and their original allocation/start intent. Original input, selection
and isolated configuration validation remain mandatory. The owning SWE-bench,
Terminal-Bench 1 or Harbor adapter checks its own retained task, submission,
SDK/image and official grading artifacts without executing tests, containers,
agents or providers. An existing resource-owner lock rejects concurrent work.
All candidate outcomes and existing receipts must validate before publishing
missing immutable receipts; artifact fingerprints must remain stable. Existing
receipts and raw evidence are never replaced. False grades remain false.

Reconciliation deliberately establishes current local artifact consistency,
not historical attestation of previously unsealed evidence. New receipts bind
that provenance marker, reused JSONL summaries expose it and comparison HTML
explains it. Already-sealed/legacy outcomes retain ordinary verification; no
ungraded, incomplete or cleanup-unconfirmed attempt becomes completed through
this option. Those require the still-unfinished interrupted-evaluation path.

Provider-free tests check both grades for all five adapters, modified/missing
grading artifacts, active owner locks, symlink/path escape, byte preservation,
Harbor reward-file precedence, empty SWE submissions and bounded protocol
responses. Rust checks preserve false grades, reject marker tampering and
prove that one bad candidate prevents receipt publication for all candidates.
Ordinary resume still refuses missing receipts.

The expanded real-Pi/synthetic-provider mixed SWE-bench Lite/Terminal-Bench 1
qualification passed in 103.5 seconds, with artifacts retained under
`target/benchmark-runtime/parallel-original-tasks-9s68v23j`. After successful
official grading and cleanup, the fixture archives both replica/group receipts,
checks ordinary-resume refusal, contradicts one retained grader result and
checks that no receipt is published, restores the original report, then
reconciles both original outcomes with no extra provider requests. Comparison
JSON/HTML preserves/discloses the provenance. This controlled metadata fault
injection is not a crashed-evaluator recovery test or an AI-solving result.

Source verification passed 432 unit tests, 87 offline CLI tests, all fifteen
protocol suites and guest/diagram/parallel/replay/resume regressions, strict
Clippy and `make build`. Package/hosted gates follow. Newly identified remaining
work: comparison currently drops completed outcome grades when optional
measurement records are absent; reporting must consume the private outcome
summary independently of optional collection. Existing unfinished-evaluation,
native/proxy/source-image recovery, full reconciliation, population-scale,
cold-cache, merge/main-CI and minor-release requirements remain unchanged.

### Task outcomes independent of optional measurement collection

Comparison now reads actual `.asb-completions` summaries when optional runtime
measurement records are absent. The generic reader binds bounded records to
their explicit replica slots, actual task/dataset/attempt and selected agent/
provider/model. It preserves reported grades, process/evaluator diagnostics,
execution time, steps and usage without inventing collector/host/series data.
Rows disclose `result_source` as `task-outcome`, `measurements` or `missing`.
If both records exist, every shared result field must agree. Duplicate slots
or attempts, changed task/dataset, malformed cohort, nonregular/oversized files
and contradictory measurements are rejected rather than mixed into a score.

Cleanup failure does not erase a separately confirmed grade, but remains
explicitly disclosed; reporting is not a resume permission or historical
attestation. Unknown grading scope, invalid baseline and old smoke exercises
remain unscored. A stopped writer's unpublished temporary record earns no
credit and does not erase completed sibling grades. Single-slot legacy
outcomes and uniquely measured old replica attempts remain readable; ambiguous
mapping never falls back to filesystem order. Even historical manifests with
no saved plan must declare bounded, valid replica counts.

Provider-free comparison tests cover all five original workload IDs, full and
partial population denominators, repeated/absent/ungraded attempts, false grades,
nonzero agent exits, cleanup failures, historical mapping and read-only conflict
rejection. The new replay-only CLI qualification exercises both default and
explicitly disabled measurement collection, separate process/task outcomes,
comparison regeneration, SVG/HTML and completed resume with no new clients.
Final source verification passed 440 unit tests, 87 offline CLI tests, all
fifteen protocol suites and guest/diagram/parallel/replay/resume/unmeasured
regressions, strict Clippy and `make build`.

The mixed original SWE-bench Lite/Terminal-Bench 1.0 qualification with real Pi,
known repairs from a synthetic provider and collection disabled passed in
93.2 seconds at `target/benchmark-runtime/parallel-original-tasks-n27nxgpa`.
It includes ordinary-resume refusal of missing receipts, contradictory-report
refusal, explicit receipt reconciliation without extra provider requests,
official grade/coverage and SVG/HTML checks, and scoped container-removal
verification. This is representative warm-cache integration, not AI-solving,
complete-population execution or interrupted-evaluator recovery. Measured-path,
package and hosted gates follow after completion.

The corresponding measured mixed-task qualification passed in 109.9 seconds
at `target/benchmark-runtime/parallel-original-tasks-dgn0hvd5`, including
collector samples, lifecycle/target scoping, agreement between measurement and
outcome records, official grades, reconciliation, coverage, graphs and cleanup.
Final compatibility review additionally covered the earliest schema-1 records,
which predate proxy-cleanup evidence: their grades remain reportable, while
missing cleanup evidence stays unconfirmed, never retroactively attested.
The final full source suite, strict Clippy and build all passed after this
legacy adjustment. Packaging and exact-head hosted verification follow.

The broader delivery requirements remain open: interrupted outcome/evaluator
handling, native/proxy/source-image ownership recovery, full sweep reconciliation,
population-scale presentation, cold-cache qualification, final review/PR merge,
exact-main CI and the new minor release.

### Owned source-build images and attempt-scoped official reports

Image ownership is now part of the durable resource journal. SWE-bench records
its unique source tag before invoking Buildx and passes the journal's ownership
labels into the build. Terminal-Bench 1 and Harbor use the shared Compose
bridge to replace local build outputs with per-attempt service tags, remove
extra upstream/shared output aliases and declare/label them before building.
Harbor's SDK OS validation resolves the same owned tag. Prebuilt images are
not claimed or changed. Normal cleanup checks all owned image identities before
untagging anything, uses `noprune` without force and records exact IDs/statuses.
Upstream/shared aliases, parent images and build caches remain untouched.

Crash recovery preserves declared images and reports their immutable IDs
alongside stopped/preserved task containers. Wrong daemon/labels/tag ownership
fail before resource mutation. This remains an observed quiescence report,
not permission to retry or a recovered evaluator grade. All five adapter
recovery routes accept bounded image identities, and retained-outcome
reconciliation checks image-cleanup evidence for declared build outputs.

Parallel runtime qualification exposed an additional real evaluator race:
the official SWE-bench instance report correctly graded an incorrect patch as
unresolved, then final report generation failed with ImageNotFound while
inventorying unrelated daemon images being removed by another task. The
owning adapter now invokes the unchanged official report formatter with its
optional Docker inventory disabled. Official grades, classifications and
denominators remain unchanged; owned cleanup is separately recorded. A pure
regression verifies that no unrelated daemon API is touched by reporting.

Actual correct/incorrect original Terminal-Bench 1.0 grading, owned build
labels and tag removal passed in 35.8 seconds (artifacts
`target/benchmark-runtime/original-terminal-qualification-mult3q57` and
`target/benchmark-runtime/original-terminal-qualification-603uv7iq`). Forced
original Harbor source fallback passed both grades and owned-image cleanup in
42.9 seconds (`target/benchmark-runtime/harbor-qualification-xp_0nn25` and
`target/benchmark-runtime/harbor-qualification-ssk5f0jw`). Original SWE source
building and known-good grades for Full/Lite/Verified plus an incorrect Lite
grade passed after the report repair in 36.0 seconds; the final expanded
qualification passed in 32.0 seconds at
`target/benchmark-runtime/original-swe-source-1_ufl73y`. It verifies all four
official summaries omit host inventories and preserves shared original images.

A real owner-SIGKILL test passed in 10.0 seconds at
`target/benchmark-runtime/resource-recovery-mbn1pw2i`: recovery stopped only the
recorded container, retained its partial file and owned committed image with
the same immutable ID, and left the live sibling/shared base image unchanged.
This qualifies durable image ownership after publication, not stopping an
in-flight BuildKit operation. Fixture cleanup removes only its own resources.

Provider-free regressions cover declaration-before-build, distinct service
outputs, absent late outputs, shared-alias preservation, all-owner validation
before deletion/stop, changed owner/tag/daemon refusal, immutable image IDs,
read-only grade reconciliation and all five SDK recovery routes. Final source
verification passed 440 unit tests, 87 offline CLI tests, all fifteen protocol
suites and guest/diagram/parallel/partial/replay/unmeasured regressions, strict
Clippy and `make build`. Normal CLI mixed-sweep/public-recovery and package/
hosted qualifications are recorded after completion.

The normal real-Pi/synthetic-provider mixed original SWE-bench Lite and
Terminal-Bench 1.0 sweep passed in 106.3 seconds at
`target/benchmark-runtime/parallel-original-tasks-pke4t82q`, including collectors,
official grades, SVG/HTML, current image-cleanup consistency, no-op public
recovery inspections and explicit receipt reconciliation/reuse without new
provider requests. The public controller-SIGKILL/recovery CLI qualification
also passed in 3.5 seconds at
`target/benchmark-runtime/asb-controller-recovery-onk9fa74`. These are
warm-cache representative integration checks, not AI-solving or complete
population execution. Package and exact-head hosted gates follow.

Remaining full delivery work includes interrupted evaluator/submission handling,
native/proxy owner recovery, complete sweep-slot reconciliation, population-scale
presentation, cold-cache qualification, final review/PR merge, exact-main CI
and the minor release. Source-image ownership is no longer an untracked gap;
image preservation alone is not complete interrupted-attempt recovery.

### Fresh-project public-source installation and first execution

The new opt-in `benchmark_live_cold_install.py` starts from an empty project,
home and package/download cache set, initializes through the public CLI,
declines optional host changes through EOF, and installs all five original
workloads through `asb workload install`. Their owning adapters automatically
install the three project-local official SDKs; no warm ASB tool installation
is copied in. It checks complete distinct instance populations and pinned
dataset identities, then installs Pi through `asb tool install` and verifies
the actual npm executable. SDK/data/client downloads use public sources and
require no LLM provider credential. The test is opt-in and skips in ordinary
offline CI; logs, step timings and population receipts stay project-local.

The complete fresh-project qualification passed in 151.3 seconds at
`target/benchmark-runtime/cold-original-benchmarks-p0k5nfud`. Full/Lite/Verified
installed 2,294/300/500 instances; original Terminal-Bench 1.0/2.1 installed
80/89. Initial Full plus SDK install took 58.1 seconds, subsequent Lite and
Verified 7.4 and 8.3 seconds, original Terminal-Bench 1 and 2 SDK/catalog installs
44.8 and 23.8 seconds, and Pi 8.4 seconds. Its version check passed.
Two preceding qualification-script mistakes were corrected (init intentionally
creates an empty tool directory, and npm Pi lives under node_modules/.bin,
not bin); these were fixture assumptions, not failed product installations.

The normal mixed original SWE-bench Lite / Terminal-Bench 1.0 qualification
then passed against that newly installed project in 101.2 seconds. Retained
artifacts are under
`target/benchmark-runtime/cold-original-benchmarks-p0k5nfud/parallel-original-tasks-lww_jb8f`.
It exercised real newly installed Pi clients, the fresh SDKs/catalogs, a
synthetic loopback provider with known repairs, official grades, independent
collectors, SVG/HTML comparison, no-op public resource recovery, missing-receipt
reconciliation and cached reuse with no extra provider requests. The underlying
host prerequisites and Docker image/build caches were reused, not purged.
This closes fresh-project SDK/catalog/client installation and representative
first execution qualification; it is not a cold-OS/Docker installation,
full-population run or claim that an AI solved those tasks.

The owned-image implementation commit `441a344c` passed packaged tests and
exact-head hosted Rust CI run `39513839617`, including installation/smoke checks.
The separately compiled local packaged full test suite also passed. Fresh-project
qualification is a new opt-in test/evidence increment; its own packaging and
hosted gates follow. Remaining full delivery work is interrupted evaluator/
submission handling, native/proxy owner recovery, complete sweep reconciliation,
population-scale presentation, final full-diff review/PR merge, exact-main CI
and the new minor release.

The fresh-install evidence commit `cfd5295d` exposed a latent hosted-test
ordering defect: ambiguous legacy records were correctly rejected, but the
first diagnostic depended on filesystem directory iteration order. The bounded
outcome reader now sorts paths solely to make validation diagnostics stable;
replica mapping still uses explicit identity or a unique measured task attempt,
never filenames. The regression additionally reverses filenames and proves the
entire valid comparison remains identical, then checks duplicate-attempt
rejection. It passed 50 direct repetitions, followed by the full 440-unit/
87-offline-CLI/fifteen-protocol source suite, strict Clippy and build. Packaging
and exact-head hosted verification follow this repair.

### Native gate and embedded tape owner-loss teardown

A dedicated child-subreaper diagnostic confirmed that the old native gate and
its client survived the ASB owner being killed. Linux native execution now
uses an exclusive gate with or without measurements. It binds the expected
parent process with a pidfd, not a spawning-thread death signal, and waits on
kernel descriptors for client completion, owner loss or cancellation. A small
separate watcher can wake the SIGSTOP-ed gate after owner loss. Cancellation
events also wake that watcher, and an explicit completion event releases it
before descendant cleanup. Terminal interrupt/hangup/termination signals do
not kill the watcher before it can wake its gate. Native resume waits for
confirmed stopped readiness; this additionally repairs an observed early-CONT
race on the unmeasured path. OwnedChild cancellation allows bounded gate
cleanup instead of immediately killing the subreaper. No shared runner signal
handler, global subreaper, host process-group signal, or privileged host change
is introduced. Native process-tree totals include supervisor/watcher overhead;
the new watcher is outside a subsequently attached CPU-accounting cgroup.

ASB's embedded tape proxy has a private stdin owner pipe, preserving the
original Node PID/readiness contract. EOF, including before startup, closes
the listener/connections and forbids further recording. Shutdown stops and
waits for only its own outbound subprocesses, escalating TERM to KILL when
necessary. Abnormal embedded proxy exits cannot confirm cleanup on either
finish or abort. Explicit legacy overrides keep their existing stdin and
lifecycle responsibilities; this does not claim to supervise arbitrary
external implementations or provider-owned bridges.

New provider-free regressions use dedicated subreaper fixtures and pinned
signals, not shared-runner cleanup. Native cases cover startup races, owner
death while paused/running, daemonized TERM-resistant helpers, spawning-thread
exit while its owner remains live, paused cancellation, runtime TERM/INT/HUP,
and terminal-style interrupts of the fixture's private session. Proxy cases
cover record/replay, EOF before readiness, outstanding requests and a
TERM-resistant synthetic curl. Public `asb run --replay` owner-SIGKILL checks
exercise the actual embedded startup wiring with and without measurements.
They require stopped/reaped clients/helpers/listeners, unchanged unrelated
siblings and no fabricated interrupted recording. All nine guest/lifecycle
tests passed. Final source verification also passed all 441 unit tests, 87
offline CLI tests, fifteen benchmark protocol suites and the remaining
diagram/parallel/partial/replay/unmeasured tests, strict all-target/all-feature
Clippy and `make build` (including rustdoc, complete diagrams and completions).
Package, real original-task and hosted qualification for this increment are
recorded after completion.

This closes the demonstrated native-gate/default-tape orphan gap, not the
remaining interrupted submission/evaluation recovery, complete sweep-slot
reconciliation, population-scale presentation, full-diff review, PR merge,
exact-main CI or minor-release delivery gates.

### Preserve incomplete environments instead of deleting partial work

Inspection found that controller EOF previously reached the normal adapter
teardown, which could delete an interrupted task's writable container layer
and Compose volumes. All five original workload registrations now distinguish
an explicit successful close from interrupted preparation/execution/evaluation.
The shared task protocol starts in interrupted state; only a close consumed
by its operation loop authorizes normal removal. Adapter exception unwinding
and preparation errors also preserve. Normal completed runs still use their
original destructive cleanup and require removal acknowledgement.

Each owning adapter preserves under its existing ResourceJournal owner lock.
The shared quiescence implementation checks the complete durable declaration,
unchanged owner journal, selected daemon and exact resource labels/names
before stopping any container. It retains writable layers, volumes, networks,
owned build tags, submissions and raw evaluator evidence. A private durable
`interruption.json` identifies preserved incomplete resources, explicitly not
a grade or permission to retry. A failed/unconfirmed stop cannot publish that
record or confirm cancellation cleanup. Rust cancellation accepts verified
stopped-preserved protocol status, but normal finish rejects it; incomplete
attempts cannot become completed results through that status alone.

New standard-library tests exercise all five registrations, normal/exception
unwinding, owner mismatch before stop, lock exclusion throughout preservation,
changed durable journals, unconfirmed stops, malformed/EOF/cancel input and
failed readiness publication. They require no destructive SDK calls, unchanged
partial work and existing false grades, and truthful preservation failures.
The real original-task qualifications passed with actual SDKs and Docker:
Full/Lite/Verified together in 119.480 seconds, original Terminal-Bench 1.0 in
18.518 seconds and original Terminal-Bench 2.1 in 12.667 seconds. Each writes
an unrelated partial file, obtains a real official unresolved grade, preserves
the attempt, reads the file from the stopped container and checks unchanged
submission/evaluator bytes. Fixture cleanup validates only its declared owners.
Artifacts are under the fresh project's `interrupted-original-m31xwvev`,
`interrupted-original-6xngpyyf`, `interrupted-original-3nnf0wq7`,
`interrupted-original-b_hqjps4` and `interrupted-original-dwfrp2_y` directories.

The actual task-server/harness qualification passed in 94.173 seconds at
`official-task-bridge-rjikiv0v` (direct harness signal evidence at
`official-harness-signal-8u9ysxau`). Normal/oracle completion removes resources;
EOF, SIGTERM and EOF during official evaluation stop/preserve the task while
still removing the owned active grader. These tests use synthetic commands or
known reference patches, never a live LLM, and are not complete-population or
AI-solving claims. They do not restore lost shell/process state or implement
recovered grading, outcome import or sweep-slot reconciliation. Those remain
delivery work together with reporting, final review, merge, main CI and release.

Final source verification passed 442 unit tests, 87 offline CLI tests, all
sixteen benchmark protocol suites and the remaining guest/diagram/parallel/
partial/replay/unmeasured checks, strict Clippy and `make build` including
rustdoc, complete diagrams and completions. Normal mixed original-task,
package and exact-head hosted qualifications are recorded after completion.

Follow-up inspection confirmed a second loss path: the generic runner always
called session finish even when the official evaluator returned an error.
Official sessions now remember only their latest task-matching completed grade;
finish without one cancels into stopped preservation and reports the retained
artifact location, rather than sending destructive normal close. A valid false
grade or empty submission remains completed and removes resources normally.
Missing/contradictory resolved flags and mismatched task identities cannot
enable removal, and a later evaluation failure clears an earlier completion
flag. Provider-free controller fixtures verify normal close versus EOF on
these paths without starting a client, provider, SDK or Docker. These changes
preserve ungraded work; they still do not manufacture a recovered outcome or
rerun an agent. The preceding preservation commit is `27380255`; its actual
mixed original-task qualification passed in 96.267 seconds at the fresh
project's `parallel-original-tasks-s172plqe` directory, and its local source/
package suites passed. Exact-head hosted CI for `27380255` passed in run
`39521575007`, including packaged tests and installation smoke checks; the
local-host job was skipped by repository configuration. The follow-up source
passed all 444 unit tests, 87 offline CLI tests, sixteen protocol suites and
the remaining integration checks, strict Clippy and `make build`. Its real
normal-path/package/hosted checks are recorded after completion.

### Inspect existing official grades without inventing lost execution data

The explicit public `asb workload recover REFERENCE --inspect-grade [--json]`
path now dispatches through the owning WorkloadAdapter for all five original
registrations. It acquires the inactive-owner lock before reading retained
evidence and validates the actual task/submission, SDK and official grading
artifacts through the existing adapter-owned verifiers. Closed attempts still
require their removed-resource evidence; stopped-preserved attempts require
a matching interruption record covering every declared resource. Missing,
incomplete, contradictory and ambiguous grades fail instead of producing a
fake failure/success. Default resource quiescence and closed-outcome
reconciliation retain their previous semantics.

The typed report scope is `retained-official-grade-not-execution-recovery`.
It contains the existing true/false task grade and explicit null process
success, exit code, elapsed time, tokens and steps. Recorded cleanup is not a
new Docker observation. This is current local artifact consistency, not a
historical execution attestation, imported outcome or sweep resume receipt.
No client, provider, evaluator or Docker operation starts. A private project
temporary runtime uses the installed SDK interpreter without replacing any
installed SDK helpers; retained artifacts and helpers stay unchanged, including
while another worker owns the attempt (which inspection refuses).

Standard-library fixtures cover true/false grades for every original adapter
in removed and stopped-preserved states, preservation of the existing strict
closed-outcome checks, unconfirmed/malformed interruption reports, changed
patches, missing/contradictory/ambiguous grades, active-owner refusal and
public CLI JSON/human routes through an externally located reference and
custom project tool directory. RPC validation rejects fabricated or missing
execution metrics, wrong identity/baseline/scope and incomplete grades.

Real retained SDK reports for all five originals passed public inspection in
0.777 seconds at the fresh project's `retained-grade-inspection-f25k8t7d`.
The qualification clones completed fixture evidence into canonical references
under the same project, preserves actual task identity/grades, and verifies
unchanged original/copy/installed-helper bytes. It does not claim those copies
were original ASB executions. The normal mixed original SWE-bench Lite /
Terminal-Bench 1.0 real-Pi/synthetic-provider qualification passed in 90.703
seconds at `parallel-original-tasks-jhfscq5j`, including inspection of completed
true grades before unchanged-receipt reuse, no new client requests, collectors,
SVG/HTML and explicit lost-receipt reconciliation. No qualification called a
live LLM or executed complete benchmark populations. Source/package/hosted
verification is recorded after completion. Actual recovered grading, outcome
import, full sweep-slot reconciliation, population presentation and the final
review/PR merge/main-CI/minor-release gates remain unfinished.

Final source verification for inspection passed 445 unit tests, 87 offline
CLI tests, all sixteen benchmark protocol suites and the remaining guest,
diagram, parallel/partial/replay/unmeasured regressions, strict Clippy and
`make build` (including rustdoc, complete diagrams and completions). Package
and exact-head hosted verification follow this increment.

### Import retained official grades into sweep reporting

Added explicit `asb sweep SWEEP.toml --import-grades [--json]`. Under the sweep
output lock it loads the original saved plan and binds each retained reference
to its configuration/replica and original allocation/start intent. It does not
run execution preflight, rotate results, require the obsolete executable for
report recovery, or start agents/providers/environments/evaluators. All candidate
grades are validated before publication. Records use private create-new
`.asb-imported-grades`, not normal outcomes or resume receipts. Existing grades
and measurements are never overwritten; identical repeated imports are unchanged.
Malformed late candidates and duplicate actual attempts prevent publication.

Compare incorporates imported true/false grades with explicit provenance. Lost
process status, timing, token/step counts and execution cohort remain unknown.
It revalidates original bindings and adapter evidence each time. Inspection
includes bounded hashes/lengths of exact artifact bytes, detecting edits even
when parsed JSON or the grade is unchanged. This is current local consistency,
not historical execution attestation. Per-attempt SDK metadata reinspection is
disclosed; population-scale reporting optimization remains necessary.

Provider-free public CLI tests cover all five adapters and both grades/multiple
replicas, reporting without installed agents or Docker, unchanged retries,
all-before-any validation, wrong intent/configuration/slot, missing grades,
measurement/outcome conflicts, active owners, duplicate attempts and edited
evidence. The mixed real-task qualification archives its own measured/outcome
files, imports both existing official grades, checks reporting without new
client requests, then restores the originals. Source/package/hosted results
are recorded after execution.

Remaining full delivery work is actual grading of unfinished retained submissions,
complete interrupted sweep-slot reconciliation/continuation, population-scale
presentation/qualification, full review, PR merge, exact-main CI and minor release.

Source verification passed 445 unit tests, 87 offline CLI tests, seventeen
benchmark protocol suites, nine guest/lifecycle checks and the remaining
diagram/parallel/partial/replay/unmeasured integrations, strict Clippy and
`make build` including rustdoc, complete diagrams and completions. The first
full-suite run exposed a diagram-extractor name collision: a generic helper
named `metadata` became a false target for filesystem method calls. Renaming
it to `original_binding` restored the complete graph test without pruning
relations or extending its deadline. The final full suite passed.

The mixed original SWE-bench Lite/Terminal-Bench 1.0 real-Pi/synthetic-provider
qualification passed in 110.397 seconds at the fresh project's
`parallel-original-tasks-ov6f3n77`. It includes existing resume/reconciliation
checks plus real official grade import, unknown execution metrics, explicit
imported provenance and comparison/SVG/HTML generation without any additional
client requests. Its own archived execution files and reports were restored;
the import-only report/evidence remains in `grade-import-qualification`.
These are representative known repairs, not full-population or AI-solving
qualification. Package and exact-head hosted checks follow this increment.

The source package verified successfully, and separately compiled packaged
tests passed the same complete offline suites. Real retained grade inspection
for all five originals passed in 0.913 seconds at
`retained-grade-inspection-b2aqini_`, including unchanged source/copy/SDK bytes
with the new evidence manifests. Review additionally tightened comparison:
an imported grade can never be combined with a normal measurement for its
slot, even if every overlapping value matches. A public CLI regression covers
this conflict and preservation of both records. The import commit `ff560436`
is signed and pushed; exact-head hosted qualification follows the final guard.

### Grade retained original SWE-bench submissions

The adapter contract now exposes explicit `grade_retained`, reached through
`asb workload recover REFERENCE --grade [--json]`. Original Full/Lite/Verified
adapters bind the installed pinned catalog to the retained task/dataset/base,
verify frozen patch bytes and original immutable image, and run only their
official evaluator. No agent or original task environment is restarted.
Already-completed true/false grades are validated/reused, never replaced or
rerun until they pass. Failed earlier grader directories remain available;
private create-new `selected-evaluation.json` names the validated completed
grader explicitly. Read-only inspection and sweep grade import use that same
selection and evidence, never a latest-directory heuristic. Retired unfinished
grader journals are separately locked/quiesced; missing ownership or an active
grader is not silently ignored. Failed/cancelled grading preserves the patch
and grader output without selecting or fabricating a grade.

The grading helper observes its controller lifetime pipe and SIGTERM for
cancellation. The first real qualification exposed Python's buffered-stdin
daemon shutdown lock failure after returning completed grading responses.
Raw descriptor reads fix this; the regression now covers clean successful
completion with stdin still open in addition to EOF and SIGTERM cancellation.
The original failed qualification artifacts remain at
`retained-original-grading-aetxibce`; that run was not accepted as a passing CLI
qualification. A fresh fixed-binary qualification and complete source/package/
hosted checks follow. Provider-free tests cover all three populations and both
grades, unchanged original evidence, completed-grade preservation, malformed/
missing old evaluator ownership, cancellation, SDK/catalog/patch mismatches,
explicit selection and import with preserved failed predecessors.

This completes neither retained Terminal-Bench grading nor interrupted slot
continuation. Those, population-scale presentation/qualification, full review,
merge, exact-main CI and minor release remain part of the unchanged full goal.

The fresh public CLI qualification passed in 168.843 seconds at
`retained-original-grading-iav6j4hg`: actual ungraded frozen patches for all
three original SWE-bench populations received their official unresolved grades,
and a known-correct Lite patch received its official resolved grade. Each
original task container remained stopped through grading, repeated `--grade`
requests preserved every artifact byte without another evaluation, and read-only
inspection returned the identical report with unknown execution metrics. The
fixture then removed only its positively-owned resources and retained reports.
This qualifies retained-patch grading, not AI solving/full-population scores,
retained Terminal-Bench grading, or interrupted slot continuation. Final source,
package and exact-head hosted verification follow.

Review also covered a lost wrapper result with already-present official raw
reports. That is not proof the tests never finished: explicit grading refuses
to rerun that evaluator and preserves its potential grade for outcome
reconstruction. A provider-free regression verifies this refusal. Reconstructing
such raw results remains recovery work; a completed normalized false grade is
never retried, and an incomplete wrapper is never used to discard a raw grade.

Final source verification passed 445 unit tests, 87 offline CLI tests, eighteen
benchmark protocol suites, nine guest/lifecycle checks and the remaining
diagram/parallel/partial/replay/unmeasured integrations. Strict Clippy,
formatting and `make build` (rustdoc, complete diagrams, interactive views and
completions) passed. Package and exact-head hosted qualification follow.

### Retained original Terminal-Bench 1.0 environment grading

The Terminal family dispatches explicit retained grading through its owning
adapter/runtime. Original 1.0 now supports the first verifier invocation for a
captured-before-verifier environment: installed definition/dataset/SDK, original
container/image and exact declared ownership must agree. Shared retained-resource
support holds the original attempt descriptor, validates the whole population,
and resumes only existing owned containers/services without Compose up/build,
replacement volumes or any agent. Normal completion, SDK failure and cooperative
owner cancellation stop/preserve those resources; no grade is selected before
cleanup confirmation. Tests cannot be silently repeated: normal and recovered
1.0 grading publish a durable start marker before invoking the official SDK.
Already-started tests need outcome reconstruction. Lost-agent-shell tasks are
refused before restart instead of using an inequivalent replacement shell.

The original task topology is now persisted for both Terminal SDKs. Restoration
retains aliases and explicit static addresses without forcing old dynamic IPs;
recovery can reconnect the main container after a scoped agent network override.
Only the original container's membership changes; unrelated networks/containers
are not removed. Recovered logs/results use a unique directory/session, preserving
the original submission and completed grades. Read-only verification additionally
requires the actual environment baseline kind/source revision/definition hash
for both versions, rather than accepting an invented Git-task baseline.

Provider-free tests cover true/false grading through original SDK seams, unchanged
completed outcomes, lost shells, definition/capture mismatch, failure cleanup,
non-retry markers, network options, exact resource identity/ownership, cooperative
cancellation and unrelated-container survival. Import/reporting tests cover a
recovered 1.0 result and tampered cleanup records. CLI diagnostics now preserve
safe helper failure classes and owned-runtime errors without printing SDK bodies.
The first real qualification found a JSON catalog-path string passed into the
SDK's Path-based catalog reader; conversion and a string-input regression fix
that failure. Failed artifacts are retained separately and are not passing proof.

Real public retained grading passed both false/true hello-world states in
45.121 seconds at `retained-terminal-grading-u2r7cufl`. The extended network-override
qualification passed in 43.650 seconds at `retained-terminal-grading-dlpclkof`,
including original aliases, retained file contents, stopped preservation, unchanged
repeat grading/inspection and unknown execution metrics. Normal Harbor correct/
incorrect SDK grading passed in 40.014 seconds (see
`target/benchmark-terminal-grading-harbor-regression.log`). Normal mixed original
SWE-bench Lite/Terminal-Bench 1.0 real-Pi/synthetic-provider qualification passed
in 95.425 seconds at `parallel-original-tasks-mpd2nqe9`, including collectors,
SVG/HTML, cached resume, explicit reconciliation and grade-only imports. No test
used a live LLM provider, and these are not full-population AI-solving claims.

Remaining full-goal work includes retained Harbor environment grading, raw-outcome
reconstruction and already-started verifier recovery, complete interrupted-slot
continuation, population-scale presentation/qualification, full review, PR merge,
exact-main CI and minor release. Source/package/hosted gates follow this increment.

Final source verification passed 445 unit tests, 87 offline CLI tests, nineteen
benchmark protocol suites, nine guest/lifecycle checks and the remaining
diagram/parallel/partial/replay/unmeasured integrations. Six retained Terminal
SDK-seam tests, twenty-three shared resource tests and ten retained-artifact
checks (the public CLI case needs its compiled-binary opt-in) are included in
those suites; nine public import/reporting cases also passed separately.
Strict Clippy, formatting and `make build` including rustdoc, full diagrams,
interactive views and completions passed. Source packaging and separately
compiled packaged/hosted tests follow.

### Workload-owned service lifetime through evaluation

The population audit identified original tasks that explicitly require live
background services during verification. Default guest quiescence killed those
services at principal exit, and the controller's fallback reset would likewise
discard them. A generic adapter-declared process lifetime now distinguishes
quiescent submissions from services delegated to the owning private workload
PID namespace through evaluation. Native/SWE execution keeps its old contract;
the Terminal family chooses service retention. No runner branch names a task.

The controller probes the actual guest namespace, verifies owned/private scope
and gives its guest driver a per-execution token. Principal-end and ready frames
bind the paused gate PID/token, with elapsed time and timeout status checked
against bounded private metadata before accepting the handoff. Execution ends
before grading, while inherited service output is drained rather than closed
with SIGPIPE. Client capture is bounded and frozen; later service logs do not
become fabricated usage/response data. The idle subreaper reaps exited adopted
services. Original SDK cleanup stops/removes the namespace and joins held output
transport; malformed/unconfirmed execution retains the reset/preservation path.

Provider-free protocol tests cover split frames, wrong owner/PID, inconsistent
metadata, repeated/out-of-order boundaries, bounded output, lifecycle publication
once, private namespace rejection and service/transport lifetime. Rust checks
cover default quiescence plus refused controller-namespace retention and frozen
service-pipe draining. Actual original `jupyter-notebook-server` qualification
passed in 70.905 seconds at `original-service-lifetime-e0tycyj3`: a synthetic
client launched the known reference through ASB's registered agent adapter, the
background HTTPS server remained reachable after client exit, all four original
SDK verifier checks passed, and only then were the owned containers/transport
removed. No live LLM was contacted. This is service-lifetime qualification, not
AI task solving, full-population qualification, or proof that arbitrary agent
interactive-shell state is preserved. Exact-source regression/hosted checks
follow; the full delivery checklist remains unchanged and incomplete.

The idle service supervisor additionally reaps adopted exited children without
signalling live ones. Repeated original Jupyter qualification passed in 57.330
seconds at `original-service-lifetime-yht6huw8`. Mixed real-Pi/synthetic-provider
SWE-bench Lite/original Terminal-Bench execution passed in 96.489 seconds at
`parallel-original-tasks-iq4yc04b`, including independent grades, lifecycle
collection, SVG/HTML, unchanged cached resume, reconciliation and grade imports.
Final source gates passed 447 unit tests, 87 offline CLI tests, nineteen benchmark
protocol suites, ten guest/lifecycle checks and the remaining diagram/parallel/
partial/replay/unmeasured integrations. Strict Clippy, formatting and `make build`
(rustdoc, complete diagrams, interactive views and completions) passed. The
retained Harbor grading/reconstruction/continuation and final full-goal delivery
requirements remain unfinished; package and exact-head hosted checks follow.

### Retained original Harbor 2.1 first-verifier grading

Original 2.1 retained grading now invokes Harbor's official verifier against the
preserved original environment instead of an unimplemented stub. Installed
dataset/SDK/definition and captured container/image/network identity must match.
The shared original-owner lock and resource verifier cover all declared services
before any restart; no environment, volume, agent or provider is created. A
Harbor environment adapter translates only SDK execution/copy operations to
those verified immutable container IDs, never Compose project discovery or
up/build/down. Official test upload, shell construction and binary reward
parsing remain in the SDK. Existing log mounts are not replaced: current
verifier outputs are downloaded to the fresh recovered-evaluation directory.
Transport output is bounded; error/cancellation kills and drains its own CLI
child so a full asyncio pipe cannot strand teardown. SDK success/failure and
owner cancellation stop and preserve the original containers, and completed
grades are selected only after cleanup confirmation.

Normal Harbor grading also publishes a durable pre-verifier start record.
Pre-verifier captures are explicit; an already-started verifier or raw reward
requires reconstruction rather than another side-effecting test invocation.
Completed true/false grades are reused unchanged. Inspection and grade imports
follow the explicit selected evaluator and validate its preserved cleanup.
Execution success/time/tokens remain unknown for retained-only outcomes.

Provider-free fixtures cover both grades, legacy/completed preservation,
catalog/definition/capture mismatch, pre- and mid-grading cancellation, SDK
failure, non-retry markers, immutable transport commands, forbidden resource
creation, output bounds, full-pipe teardown and both original versions' imports.
Real public CLI qualification passed in 47.028 seconds at
`retained-harbor-grading-vgdcu26k`: original `cancel-async-tasks` incorrect and
known-reference states received their official false/true grades; original
submissions, repeated grading and inspection stayed unchanged, resources were
stopped, and only positively-owned fixture resources were subsequently removed.
No agent or live provider was used, and no full-population AI score is claimed.
The preceding service-lifetime commit's hosted CI run 39538101517 passed.
Final current-source/package/hosted verification follows. Raw-outcome recovery,
already-started verifier reconstruction, interrupted-slot continuation,
population-scale presentation/qualification, final review, merge/main CI and
the new minor release remain in the unchanged full goal.

Final-source retained Harbor public CLI qualification passed again in 44.793
seconds at `retained-harbor-grading-cy0fiayd`; normal Harbor false/true SDK grading
passed in 40.950 seconds. The full source suite passed 447 unit tests, 87 offline
CLI tests, twenty benchmark protocol suites, ten guest/lifecycle checks and the
remaining integrations. Eight Harbor retained/transport cases and nine public
import/compare cases are included. Strict Clippy, formatting and `make build`
with rustdoc, diagrams, interactive views and completions passed. Packaged-crate
and exact-head hosted verification follow; the full delivery checklist remains
open.

### Existing official SWE-bench outcome reconstruction

Explicit retained grading can now reconstruct a missing/failed wrapper result
from already-completed official reports instead of refusing or rerunning the
tests. It takes the original evaluator's inactive owner lock and validates its
task-specific resource journal/cleanup, full frozen dataset row against the
installed pinned task, candidate bytes, SDK and immutable image. The original
instance report and population summary must agree on a completed true/false
outcome. No Docker daemon, evaluator, client or provider is started by this
path. Original inputs, reports and any failed wrapper remain unchanged.

A separate metadata-only evaluator directory holds bounded copies and a source
manifest, including the original wrapper's bytes or absence. Private staging
keeps an interrupted metadata copy from masquerading as another official test
invocation. A complete derived publication missed before selection can be
reconciled without creating another grade. Current local inspection and
import/compare revalidate original source bytes, declared ownership/cleanup and
copied reports; source edits or replaced formerly-missing wrappers are errors.
No execution status/time/tokens are reconstructed or historically attested.

Provider-free tests cover both grades and missing/failed wrappers for all three
populations, active owners, incomplete/mismatched data, changed SDK/image,
malformed provenance, source edits, interrupted staging/selection and grade
imports. Ordinary official retained grading and completed-grade reuse remain
regressions. Runtime qualification and final source/package/hosted gates follow.
The prior Harbor grading commit's hosted CI run 39539643992 passed. Terminal
verifier outcome reconstruction, interrupted-slot continuation, population
qualification/presentation, final review, merge/main CI and minor release remain
in the full unchanged goal.

Real public CLI reconstruction qualification passed in 165.627 seconds at
`retained-original-grading-wwls14jp`: the original Full/Lite/Verified unresolved
cases and a known-correct Lite case were graded once with the official SDK,
then only their ASB wrapper/selection were archived as a controlled loss fixture.
Reconstruction succeeded with a deliberately unavailable Docker endpoint;
original SDK bytes, repeated results/inspection and stopped task state stayed
unchanged. No live LLM was used. This exercises actual official reports, not a
claim of full-population AI solving or recovery of unknowable crash gaps.
Final source gates passed 447 unit tests, 87 offline CLI tests, twenty benchmark
protocol suites, ten guest/lifecycle checks and the remaining integrations.
Twelve retained SWE grading/reconstruction checks and ten public import/compare
checks are included. Strict Clippy, formatting and `make build` with rustdoc,
diagrams, interactive views and completions passed. Packaged-crate and exact-head
hosted checks follow; the full delivery checklist remains incomplete.

### Original Terminal SDK-return reconstruction and delivery audit

Both original Terminal SDK adapters now publish a durable create-new SDK-return
observation before their final result. The observation binds parsed facts to
the actual task/SDK, durable start marker and raw verifier pane/reward bytes,
including which reward format was absent. A lost/failed normalized wrapper can
be reconstructed in a separate metadata-only evaluator directory without SDK,
Docker, services, agents or providers. Private staging and explicit selection
handle interrupted copying/publication without another verifier invocation.
Original bytes are not replaced. Inspection/import/compare validate both copied
and original evidence, wrapper bytes/absence and confirmed source cleanup.

Outcomes without a trustworthy published SDK return remain unknown: the adapter
does not infer a grade from success-looking logs or silently repeat side effects.
That physical observability limit is not historical attestation or an execution
metric. Both false and true grades are preserved identically.

Provider-free contracts cover both versions, both grades, normal/recovered
locations, missing/failed wrappers, changed observations/raw data, cancellation,
missing cleanup and interrupted staging/selection. Eleven import/compare cases
also pass. Actual original SDK/public CLI qualifications passed in 43.484 seconds
at `retained-terminal-grading-iem6axqu` and 45.092 seconds at
`retained-harbor-grading-kptz_8ma`, each for correct/incorrect states, then lost
wrapper reconstruction with a deliberately unavailable Docker endpoint. Original
bytes, repeated reports and stopped state remained unchanged. No live LLM was
used. Prior SWE reconstruction hosted CI run 39541287011 passed.

See [the delivery audit](FULL_BENCHMARK_AUDIT.md) for the actual requirement map,
implemented evidence and remaining release gates. The objective is unchanged:
safe started-slot continuation, population-scale qualification/presentation,
full review, PR merge, exact-main CI and the new minor release still remain.
Final current-source/package/hosted checks follow this increment.

Final source verification passed 447 unit tests, 87 offline CLI tests, twenty-one
benchmark protocol suites, ten guest/lifecycle checks and remaining integrations.
The new SDK-return reconstruction suite covers five contracts across both
original versions and all normal/recovered/missing/failed outcome variants;
eleven import/compare checks pass. Strict Clippy, formatting and `make build`
including rustdoc, complete diagrams, interactive views and completions passed.
Package and exact-head hosted verification follow. The audit's remaining
continuation, population qualification and final delivery gates stay open.

### Started-slot grade-only closure and complete-population planning

Explicit `sweep --resume --reconcile-completed` can now close an allocated,
started slot with a known official grade but no ordinary run record. This is a
separate immutable grade-only receipt, not a normal execution completion. It
binds the original intent/allocation/selection, current declared inputs and
preserved per-slot outputs; adapter-owned inspection independently confirms
the official outcome. Both false and true grades are reused without another
client, provider request or evaluator. Unknown process/timing/token/step/cohort
observations stay null. Reuse returns a non-success status disclosing unknown
execution, rather than inventing process success for a resolved task.

Comparison revalidates the original SDK grade and presents its explicit source.
Partial measurement files are preserved and downloadable, but are not parsed
as final measurements or embedded as final execution charts. Compatible
report-only imports remain intact and are deduplicated only for presentation.
Resume additionally rechecks current inputs and preserved outputs; reporting
does not claim historical integrity or current-executable equivalence. Changed
bindings/grades/outputs, duplicate attempts, conflicting ordinary outcomes and
unknown grades cannot silently authorize another attempt. Candidates are all
validated before any new closure is published. Tests also check a closed false
grade alongside a positively reserved unstarted sibling; possibly-used sibling
outputs still reject admission.

Provider-free contracts cover this state separately from real SDK qualification.
All five reporting adapters pass both grades with two slots each and incomplete
execution measurements. The public mixed-original-task qualification initially
caught an omitted null cohort field; that JSON consistency issue was repaired.
Its repeat passed in 108.767 seconds at `parallel-original-tasks-84_4idxm`:
real Pi clients, synthetic provider, original SWE-bench Lite and Terminal-Bench
1.0 official graders, measured execution, loss of fixture run records, explicit
closure, repeated resume and regenerated comparison. No further provider
requests occurred during recovery. It is not an AI-solving score or exhaustive
environment execution.

The complete-population planning/reporting qualification also passed in 1.15
seconds at `18e07187c984c27d-a26f2-0`. It loaded the actual installed pinned
catalogs through their adapters, built/reloaded all 3263 selected instances
(2294/300/500/80/89), generated comparison and verified complete selection with
zero evaluated attempts and null full-population scores. No agent, provider,
Docker or evaluator was executed. Final disposition review, source/package/
hosted verification, PR merge, exact-main CI and the minor release remain
delivery gates; full-population selection is not full-population AI performance.

The corresponding measurements-disabled public mixed-task qualification passed
in 99.966 seconds at `parallel-original-tasks-zfsvemmi`, including grade-only
closure, repeat resume and comparison without new provider requests. The
packaged crate passed 454 unit tests, 87 offline CLI tests, 21 benchmark protocol
suites and 10 guest/lifecycle checks plus remaining integrations. These are
provider-free tests, not full-population scores.

The enlarged function graph exposed Graphviz 2.43's clustered hierarchical
`init_rank` failure during the final build. That failed renderer was stopped;
the build was not counted as successful. Function-flow diagrams now use packed
module layout, preserving every node, module box, directed edge and label;
the trait/UML renderer is unchanged. Complete-source graph regressions pass in
under two seconds and `make build` succeeds. Absolute/relative source paths are
normalized identically so the regression exercises the actual build graph.
Final source/hosted checks and delivery remain to be recorded.

### Final implementation review and minor-release preparation

The implementation gates above were reviewed against owning adapters, typed
session/identity/selection contracts, install recipes, retained runtime evidence
and provider-free regressions. A missing grade or incomplete ordinary outcome
refuses reuse; owned-resource recovery plus a fresh sweep is the explicit new
attempt disposition, preserving prior results/reports in history. Missing
observations are not an implementation requirement to invent evidence.

The review found that non-interactive setup could not specify an instance even
though the workload/setup TUIs could. Generic `--trial INSTANCE` and optional
`--trial-set FILE` now round-trip into benchmark configuration and use the same
catalog/selection validation as planning. Missing/unknown instances, disabled
trial-set entries, invalid paths and stale catalogs fail before replacing an
existing configuration. Provider-free CLI regressions cover all five synthetic
catalog shapes, configured local directories and no SDK/provider execution.
README and evaluation guidance now describe actual original-benchmark execution
instead of retaining the obsolete smoke-only warning for these five IDs.

A final measured public-CLI sweep passed for each workload with real Pi,
deterministic synthetic-provider repairs, official grading, usage/lifecycle
records, SVG/HTML and scoped teardown: Full 64.173s, Lite 63.899s, Verified
63.782s, original Terminal 1.0 46.089s and original Terminal 2.1 56.010s.
The first Terminal refresh caught an outdated fixture assertion expecting
pre-grading quiescence; its correction verifies adapter-owned service lifetime
and independently checks that SDK-owned task containers are removed afterward.
This changes the fixture contract, not grading or benchmark task semantics.
These remain representative known repairs, not full-population AI scores.

Workspace/package version is prepared as 0.2.0. Source tests and strict Clippy
pass with the new setup selection; the complete five-workload runtime sequence
is being repeated using public scripted setup first. Exact release-head package,
hosted, merge/main-CI and publication checks remain required. Signed pre-release
head 923ffa28 passed hosted run 39546156258 before these final setup/docs changes.
The two delivery boxes intentionally remain open until those hosted actions
are verified; implementation checks alone do not authorize a release claim.
