# Bpftrace prerequisite diagnostic repair: source checkpoint

Owning installer49 follow-up from immutable595a8754, in new isolated checkout
`/home/martin/projects/asb-bpf-prerequisite49`. No build/runtime/version/push/PR
has run here. Llmtape50 cbdf and its cache, raw failed fixture and frozen hashes
are unchanged. This is not combined qualification or a release.

## Observed failure and actual contract

Combined50 proxy56/gate32/installer104 passed, then tools focus stopped at the
existing missing-curl assertion (tools14passed/1failed/2live opt-ins ignored).
Its actual diagnostic incorrectly blamed source download/network failure even
though the exact inherited fake-only PATH had no curl. Native gate target127 is
not a Command::spawn NotFound error. No actual network or package command ran.
The fixture had preserved the useful missing-prerequisite contract; broadening
its assertion would conceal a usability regression. Old log, binary/libtest
hashes and raw root remain in the llmtape50 checkout unchanged.

## Narrow owning production change

Each Linux curl/tar/cmake stage checks factual executable availability with
tools::host_executable at its inherited PATH/current cwd. It uses the resulting
absolute PathBuf in the SAME ProcessScope prepared command and existing stage
capture/ownership/budget machinery. It does not use install::host_executable's
system-directory fallback or infer absence from any returned status. A selected
executable can still change/fail after lookup; that remains an ordinary actual
execution failure, not false missing-tool proof.

Checks are stage-local: cached source requires no curl or tar. CMake build
selection is checked again at its own stage. Existing OperationContext checks
before/after lookup preserve typed cancellation/deadline Abort instead of a
new ordinary prerequisite failure after operation stop. No scope permission,
settlement, producer, abort or public installer API is replaced. Non-Linux keeps
its native direct program/PATHEXT selection; it gains no Linux PATH preflight.
Missing tools regain dependency-named distro repair commands through the owning
BPF helper. A real executable returning127 keeps its existing phase-failure
diagnostic. Source-directory create failure names its exact source path rather
than losing the owning context in generic output I/O.

## Source regression matrix awaiting independent review

All19 existing fake-command cases remain with unchanged required assertions.
Seven additional cases cover cached source with curl/tar absent, a raw relative
PATH at the captured cwd, four real marker-writing exit127 commands (download,
extract, configure and build), and cancelled operation with curl absent. The
Linux positive cases assert every actual command argv0 is an absolute path in
the exact owned command directory. Missing/127 cases assert exact phase progress,
actual command count, no later phase, no built/published executable and no
compatibility metadata. Missing tools must name their particular dependency and
repair commands; source-blocked must name its exact source directory. Exit127
must NOT acquire missing-tool text. Cancelled absence remains typed Abort with
zero command calls.

The existing raw fixture Command::output collection and broader custom installer
publication faults are unchanged, not newly bounded or cleared by this repair.
Only formatting and diff checks have run. Actual-FIRST/frozen focus, strict lint
and any later current-stack merge require independent source approval first.

## First owning49 qualification and authorized diagnostic follow-up

Source798 actual-FIRST passed13.97s and its libtest build passed24.58s. The
initial tools command mistakenly selected a copied historical50 libtest rather
than Cargo's newly reported75b0f629 image. Its old missing-curl reobservation,
wrongly named frozen copy and manifests are retained, explicitly NOT798 runtime
evidence. Correct75b0/frozen2340f0ff and actual/frozenbffa424b were then selected
and verified before the actual owning focus. That run passed all new controls
and source-blocked before the existing bin-blocked assertion failed: generic
`File exists` lacked owning creation context. Tools14/1/2 is NOT qualification.
The complete logs/namespaces and matching after-failure hashes remain unchanged.

Root authorized only context repair at the existing root/bin creation sites.
The blocked binary directory now reports its exact path, and its original
assertion is strengthened to require that path and owning message. Root/source
directory creation also retains exact context. Copy/metadata diagnostics,
publication ordering/atomicity and custom52 obligations are untouched. Fresh
actual-FIRST and the exact Cargo-reported libtest must precede another focus;
no successful result is claimed here.
