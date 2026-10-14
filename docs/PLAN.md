# Development plan

## Goal and definition of done

Agent Systems Benchmark (ASB) will be a Rust Linux terminal framework that
measures the sustainable concurrency of real AI coding agents under explicit
task-quality, response-latency and system-resource bounds. It will support
OpenCode, OpenDesk CLI, aider and Codex through independent adapters; required
native x86_64 and pinned QEMU-emulated AArch64 where applicable; optional future
native ARM64; mainstream Linux families plus openEuler; versioned
software-engineering workloads; and deterministic provider-response replay.

A result is publishable only when the run manifest, immutable workload and
scorer, exact agent/model/platform identities, raw observations, failure
denominators, uncertainty, replay mode and artifact hashes are available.
Unsupported and unmeasured cells remain visible. ASB must never infer native
kernel support from a container or cross-build.

The product repository contains code, protocol, tests and user documentation.
It is MIT licensed. Development, toolchains, caches, worktrees and tests remain
on the configured second drive of the remote host. The public files contain no
host alias or private absolute path.

## Design principles

- New product code is safe Rust 2024 on pinned 1.93.0. Rust compiler warnings,
  Clippy warnings and rustdoc warnings fail. Invalid states use checked types
  and explicit errors.
- Built-in extensions use Rust traits. External extensions use a bounded,
  version-negotiated JSON-RPC stdio protocol rather than Rust dynamic-library ABI.
- Separate agent, model/provider, workload, scorer, execution environment,
  platform, trial and job identities.
- Independent immutable grading decides correctness. Agent output or exit zero
  cannot certify success.
- Latency includes queueing in open-loop runs; failures, timeouts and censored
  attempts remain in reported denominators.
- Replay controls model responses while executing the real agent and workload
  tools. It does not claim deterministic OS timing.
- Privileged/native measurement is capability-gated and isolated from public
  pull-request code. CI and benchmarks do not contend for the same reservation.
- Every external dataset/tool has pinned provenance and an explicit license,
  portability and semantic-preservation review.

## Delivery sequence

### Phase 0: public foundation

The bootstrap publishes the repository with a Rust workspace, minimal checked
domain primitives, documentation, coordination framework and CI, and installs
complete Rust, workflow, documentation and supply-chain gates. These establish
the only supported path for later parallel workers.

Exit: both public main branches, required checks green on exact heads, live
coordination doctor green, branch protection/configuration recorded, and no
private state or credentials in source/history/artifacts.

### Phase 1: contracts and execution substrate

The contracts phase freezes protocol v1 and schemas, adds process groups, bounded
I/O, deadlines and cancellation, rootless isolation, resource leases and cgroups,
and atomic run journals, artifact integrity and recovery. Content-addressed
experiment identity is defined early so all later components record compatible
evidence.

These tasks may proceed in separate worktrees after the protocol v1 schemas
stabilize. Shared Cargo/schema changes require coordinator review.

Exit: synthetic external plugin interoperation; crash/cancel/orphan/disk-full
negative paths; no duplicate side effects after recovery; comparable manifests
identify every required confounder.

### Phase 2: measurement and experiment engine

Portable process/cgroup/procfs collectors and optional perf/eBPF diagnostics are
implemented, statistics and SLO decisions are validated, and closed/open-loop
scheduling, warmup, repetition and adaptive boundary refinement are added.
Enforceable budgets, pass^k and mixed-workload fairness, and privacy-safe causal
trace export complete the engine.

Exit: a synthetic service with known limits reproduces expected capacity and
failure behavior; measurement overhead is bounded; missing metrics produce
inconclusive decisions; cost unknown is never zero; hand-calculated statistics
match.

### Phase 3: client adapters and local workloads

The four real client adapters are implemented and independently tested. Seven
small offline engineering workloads are delivered through the public extension
API. Grading is protected and offline rescoring is enabled.

Adapter work can proceed in parallel because each adapter owns a module and uses
one shared conformance suite. Live claims apply only to the exact pinned client
version, protocol mode, distribution and architecture tested.

Exit: every supported client completes, fails and cancels on the same isolated
fixtures; graders resist modification; tool/protocol failure is distinct from
semantic failure.

### Phase 4: response record and replay

Buffr, mitmproxy, WireMock, VCR.py, llmtape, promptecho, agrepl, ACP recording
and inference simulation are compared. Immutable redacted cassettes, strict
fail-closed per-session replay, immediate/fixed/original/seeded pacing and
service headroom are validated, and each real adapter is proven through live
record then network-denied replay.

Exit: provider dialect conformance for OpenAI Chat Completions, OpenAI Responses
and Anthropic Messages as individually supported; parallel identical requests
cannot consume each other's events; unmatched requests cannot escape to network;
stream/tool/cancel/error trajectories and artifact grading reproduce.

### Phase 5: platform and workload breadth

Current distro/image/agent availability is pinned and required native x86_64 and
pinned QEMU-emulated AArch64 evidence is supplied, with native ARM64 retained as
optional future qualification and openEuler support labeled per cell. SWE-bench,
Aider Polyglot and Terminal-Bench are added; code-generation controls, validity
and portability, performance/reproducibility workloads, and evolving long-horizon
tasks are evaluated.

Exit: every matrix cell says planned, build-only, simulated, native-tested or
unsupported; reference solutions pass on every claimed native platform; adapted
tasks disclose semantic changes; speedups use paired host-local uncertainty.

### Phase 6: integrations, UX and assurance

Optional CSB integration is decided using a real round trip and prohibits nested
schedulers/double metrics. Stable terminal/JSON commands and executable quickstart
and extension examples are delivered. Bounded Kani/Loom assurance, contract
artifact checks, and run/recovery/replay temporal models with TLA+, Alloy and
executable state exploration are added, plus fuzzing, mutation and lifecycle fault
campaigns. Distributed workers arrive later without turning cross-host clocks
into latency, and the first release is qualified.

Exit: offline quickstart runs; packaged native artifacts install cleanly; exact
source/SBOM/provenance/checksums accompany reproducible releases; controlled
experiments support stated capacity claims.

## Command and result model

The planned CLI is:

```text
asb doctor
asb plan EXPERIMENT.toml
asb run EXPERIMENT.toml
asb sweep EXPERIMENT.toml
asb record EXPERIMENT.toml
asb replay RUN
asb compare RUN...
asb report RUN...
```

Only help/version exist at bootstrap. Documentation and capability output must
derive from implemented registrations so planned commands cannot appear as
working features. Human progress goes to stderr; stable machine output goes to
stdout with versioned schemas and meaningful exit codes.

## Parallel work and integration

After foundation, parallel lanes are runtime/storage, metrics/statistics,
individual adapters, workloads, replay, platforms and assurance. A worker claims
one task, branch and worktree. Interface freeze tasks precede fan-out. Integration
is serialized by the coordinator and gated on exact-tree CI. Workers publish
small signed DCO commits and never merge their own unreviewed state.

Heavy native tests acquire explicit host resource leases. Public PRs run only on
disposable GitHub-hosted or ephemeral workers. Credential-bearing live-provider
tests are manual/scheduled in controlled environments; sanitized summaries, not
raw prompts or secrets, become public evidence.

## Initial quality targets

Bootstrap: format, Clippy, tests, rustdoc, release build and CLI smoke on native
GitHub x86_64 plus pinned QEMU-emulated AArch64; coordinator strict typing, 20 fault tests and at least 95%
branch-aware coverage. Later gates add cargo-llvm-cov with 90% overall and 95%
for core/protocol/replay, cargo-deny/audit, actionlint/zizmor/Gitleaks, schema/docs
checks and immutable Action pins, plus proofs, model tests, fuzz/mutation and
retained counterexamples. Floors may rise after stable evidence;
they do not fall to merge a feature.

## Key risks

- Agent CLIs differ in endpoint control, structured events and authentication.
  Probe exact versions and expose honest per-capability support.
- Replay services can become the bottleneck. Calibrate and record their queueing,
  CPU and achieved pacing at loads above the client sweep.
- Public benchmarks can be contaminated or gamed. Preserve holdouts, protected
  graders, exposure metadata, independently versioned scores and audit trails.
- External workload images often lack native arm64 parity. Rebuild only with
  oracle evidence; otherwise keep the cell unsupported.
- System performance is sensitive to topology, governor, caches, kernel and
  background work. Content-address configurations, pair comparisons and label
  contamination rather than normalizing it away.
- Distributed execution can hide host-local saturation. Keep capacity scoped per
  host/configuration and defer distributed coordination until the single-host
  model is verified.
