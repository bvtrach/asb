# Benchmark planning and measurement policy

ASB resolves backend, host platform, task input modalities, requested policies,
collector targets, phase caps, repetition identity and declared admission
reservations before starting tools, providers or workload environments. Setup's
CLI/TUI publishers use the same resolver. Unsupported requirements fail before
execution; no remote backend is advertised. Current Docker execution requires a
same-architecture Linux x86_64 or aarch64 ASB runtime. Image/runtime validation
and local target ownership checks remain mandatory at execution.

Existing TOML files retain their defaults. Optional generic configuration:

```toml
[planning]
backend = "native"
required_policies = ["owned-process"]

[planning.budgets]
execution_seconds = 900

[planning.reservation]
cpu_millis = 2000
memory_mib = 8192
disk_mib = 16384

[planning.repetition]
index = 2
total = 5
```

Repetition identifies a distinct selected configuration. It does not create
extra executions: select each repeated configuration explicitly. Its replicas
are still controlled by `concurrency`; sweep `jobs` bounds concurrent distinct
configurations and `max_active_attempts` bounds their summed replica counts.

For resource admission, a sweep manifest can additionally contain:

```toml
configs = ["one.toml", "two.toml"]

[resources]
cpu_millis = 8000
memory_mib = 32768
disk_mib = 65536
```

Every configuration then needs an explicit per-replica reservation. ASB admits
its whole replica group only when its summed CPU/memory/disk requests and count
slots fit; FIFO permits stay held through preparation, grading and cleanup.
Overflow, unknown costs, empty budgeted dimensions and individually impossible
groups fail before preserving old sweep results. These are operator-declared
host scheduling reservations, not measurements or newly enforced task limits.
The adapters retain ownership of Docker/Compose limits. SWE's declared admission
envelope includes its 8 GiB source build (execution containers use 4 GiB).
Harbor requirements are read from explicit numeric fields in pinned task TOML;
missing SDK defaults remain unknown. Original Terminal-Bench YAML/Compose
requirements are not guessed. Known module requirements reject under-reservation.

New sweep entries contain a resolved `policy`, and the sweep records its resource
capacity. Continuation compares those operational choices, task requirements,
collector selection, host platform, repetition and existing module identity.
Historical plans lacking a policy remain readable for comparison but cannot
silently resume as a newly planned experiment. Runtime dispatch revalidates the
saved policy before per-attempt startup and instantiates only the frozen adapter
set. Collector dependency expansion is preserved without enabling optional
collectors merely because an automatic reporting helper exists.

Standalone and sweep attempts publish private create-new policy records under
`.asb/execution-plans/` before provider/collector startup, then publish an
additional record with actual task identity and effective controller caps before
the agent executes. Run logs, JSONL and completion summaries retain the policy,
effective caps and record reference even without measurement collection.
Instructions, credentials and attachment payloads never enter policy records.

`execution_seconds` caps the native process or guest driver. Official task
execution defaults are taken from the controller's ready invocation, including
fractional seconds, and recorded separately from requested overrides.
`environment_preparation_seconds` caps the controller-ready phase only; it is
supported for official Docker sessions. `evaluation_seconds` caps the official
controller's evaluation request/wait, with its existing transport allowance.
An upstream verifier can retain a tighter deadline. These are controller caps,
not measured durations. Cleanup and transport have separate existing allowances.
Native preparation/evaluation overrides are rejected. A blanket
`preparation_seconds` is not accepted; `agent_preparation_seconds` is explicitly
unsupported because local bootstrap and guest installation do not yet share a
configurable end-to-end deadline.

Collector descriptors distinguish agent-process, whole-container, host-context
and run-observation targets and execution versus run scope. Declarations do not
prove host permission or successful collection: existing capability probes and
verified PID/generation/cgroup attachment still decide that. Unmeasured Docker
runs cannot require the measurement-only `verified-local-target` policy.
The CLI measurement-directory override participates in negotiation even when
configuration disables measurements. Unavailable results remain unavailable.

Task invocation now carries structured instructions and bounded attachment
references (relative artifact, MIME type, byte length and SHA-256). Registered
agents declare input modalities. Current transports support text only and reject
image/audio inputs; introducing an attachment type does not qualify multimodal
execution or authorize dropping attachments.

## Remaining qualification and extensions

This change supplies explicit repeat identity, not automatic repeat generation
or statistical aggregation. End-to-end agent-preparation deadlines, original
Terminal-Bench YAML/Compose requirement extraction, SDK-default requirement
resolution and payload transport for additional input modalities remain separate
implementation work. None is advertised as currently supported. Real installed
SDK/Docker runtime qualification remains opt-in; provider-free tests cover
admission, policy rejection, protocol budgets, input validation, frozen collector
selection and persistence without changing host privileges.
