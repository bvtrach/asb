# Bounded ELF metadata discovery

This repair separates two existing responsibilities: discovering a native
runtime's loader/library identity and copying its complete payload into an owned
guest. Metadata inspection must not apply guest payload size limits to debug
executables. Guest staging remains bounded to 256 MiB per file and 384 MiB per
bundle; this is not a limit increase, stripped-debug workaround or trust waiver.

## Evidence and design

Hosted main run 39711305901 and integration run 39719742793 failed native runtime
discovery and downstream continuation/checkpoint fixtures. The helper's redacted
error did not identify its internal stage. Locally, a genuine libtest executable
with a sparse nonloaded tail above 256 MiB reproduced the same `helper failed`
discovery error before the repair. The repaired production discovery path accepts
that exact class of executable. This reproducer establishes the size defect; it
does not assert an observed byte size for the earlier hosted binary.

During this work, PR #312 merged the observed Ubuntu 26 loader-format and
offline-Python-prerequisite repairs into main. Those are retained unchanged;
this is a separate oversized-metadata defect and is not presented as the
established cause of the historical hosted failures.

`asb_runtime.elf_identity` now reads only ELF64 identity/program/interpreter
headers from one pinned regular descriptor. A shared parser preserves the full
payload reader's format/type/version/architecture validation and bounds all
offsets against the observed file length. Reads are at most 64 header bytes,
2048 fixed 56-byte program-header prefixes and one 4096-byte interpreter. A large
declared stride changes seek offsets, never an allocation or read length.
Duplicate, malformed, relative or unterminated interpreters are rejected.
Same-descriptor size/device/inode/mtime/ctime observations must remain stable.
Target programs are never executed to discover their identity.

The complete `elf` reader retains its payload cap, full-byte return value and
pre-parse mutation check. It uses the same header parser on the already-open
descriptor and checks stability again afterward, without allocating another
payload-sized metadata buffer. Runtime declaration discovery uses the metadata
reader; complete keyed input fingerprints and guest staging remain independent.

## Extensible qualification

Python tests cover oversized sparse metadata with fixed read counts and unchanged
guest refusal, maximum count/stride without proportional allocation, malformed
header/range/interpreter matrices, short reads, replacement and mid-inspection
mutation. Existing full-payload and library-discovery tests remain active.

An actual Rust subprocess regression copies the running libtest executable,
retains at least its original length, appends a sparse tail if needed, and runs
the real process/Python/script discovery test through the actual ASB helper.
Only a dedicated execed creator opens that fixture writable; it is joined before
launch so unrelated test forks cannot inherit a writer and cause ETXTBSY. All
process environments are isolated and no provider, agent, evaluator or privileged
host setting is involved. Original guest/runtime opt-in qualifications remain
separate; a skipped opt-in is not a passing live qualification.

Required delivery gates follow DEVELOPMENT.md: formatting, strict Clippy, full
tests/build, documentation/diagrams, package/extracted installation and hosted
exact-head verification. Coverage remains an optional diagnostic, not a newly
introduced merge threshold.
