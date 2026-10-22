# Main a32 hosted failure diagnosis (not a repair or qualification)

Exact base a32db3a1152509afb454df50e10b043bbf0ae3f7, version0.2.44.
AGENTS and instruction files match the fully read c725 DEVELOPMENT/ARCHITECTURE.
Typed48, installer and delivery worktrees/caches remain untouched. No product
change, build or runtime has occurred in this diagnostic checkpoint.

Run39711305901 job119603026328 used Ubuntu26.04 image20261018.168.1 and failed
five library tests (1206passed/5failed/13ignored). Complete original transcript:
/home/martin/projects/asb-delivery-pr311/target/pr311-postmerge-hosted-failed.log.

1. launch/tests.rs174 copies /bin/echo to a non-UTF8 basename. The actual
   returned bytes say `<unknown binary name> 0.10.0 (multi-call binary)` with
   an applet list rather than the requested raw argument. This is consistent
   with a multicall dispatcher selecting its applet from the renamed argv0,
   not proof that ASB changed the original command. A candidate test-only fix
   retains basename echo beneath a non-UTF8 parent directory, preserving an
   actual non-UTF8 executable path and argument; the explicit non-UTF8 arg0
   shell case stays unchanged. Controlled dispatcher evidence is required first.
2. perfetto.rs1331 and runtime_inputs.rs483 fail actual runtime discovery with
   the bounded public helper-failed classification. It deliberately omits raw
   helper stderr and does NOT establish the underlying Python exception or
   failing dependency. The former probes a copy of /bin/true named unshare;
   the latter combines current Process, Python(site=false), shebang script and
   a missing optional file. Do not assume either is merely a fixture problem.
3. runner_phases.rs2576 cannot read one of plan.json, benchmark.toml or started
   receipt; sweep_phases.rs51 cannot read an expected JSON file. Input sealing
   calls runtime discovery before initial snapshot construction and can be
   unavailable independently of grades. A shared upstream helper failure could
   explain missing optional seals/receipts, but the failed log does not prove
   this link. Missing artifacts are not permission to weaken these assertions.

Proposed bounded diagnostic phase, subject to independent approval: copy ONLY a
verified-idle owned HOME cache into this new project's own target; build actual
ASB first, freeze/hash and run the five exact owning filters without source edits.
Run a project-local driver that loads the unchanged production Python Discovery
definitions without invoking the public catch/redaction wrapper, then tests each
hardcoded owned-fixture dependency independently. Record operation category and
exception class first; any full diagnostic traceback stays in the owned private
fixture log, never in public errors. Use a finite subprocess timeout and actual
owned subprocess custody/cleanup for the harness; no live provider/SDK/network,
privileges, host settings, real credential values or inherited-input logging.
Keep the original limits, shell environment policy and generic orchestration.
No blind CI retry, applet policy expansion, raw error disclosure, skip or budget
increase is proposed. If local host cannot reproduce image-specific behavior,
report that limit and request a narrow owned diagnostic in the actual hosted
environment rather than guessing a production change. New source and design
review precede any repair implementation or public promotion.

Image source: https://github.com/actions/runner-images/blob/ubuntu26/20261018.168/images/ubuntu/Ubuntu2604-Readme.md

## Proposed first source checkpoint (unrun)

Only the non-UTF8 launch fixture now keeps basename echo beneath its non-UTF8
directory. It additionally checks the actual native route's collector-facing
descriptor retains the exact raw program and argument bytes; execution still
uses the same gate path, and the explicit non-UTF8 shell arg0 case is unchanged.
A private test-only ASB_TEST_MULTICALL_ECHO_IMAGE override permits the same test
to copy an already-installed BusyBox multicall image during controlled local
qualification; absent that explicit override it uses the actual host /bin/echo.
No new tool dependency/skip is imposed on ordinary CI and no product environment
or executable policy changes. This proves the relevant dispatch mechanism, not
the historical hosted uutils image identity. The other four failing cases remain
unrepaired and unqualified pending actual helper-operation diagnostics.

## Executed local focus and next hosted evidence (source only)

Actual ASB built FIRST13.04s in this diagnostic checkout's own HOME target,
seeded only with verified-idle hotfix debug dependencies (original untouched).
Actual/frozen target/qualification/hosted-diagnosis-d9921fc8-asb SHA256:
`0c174b0fc766af6b0545f377febfcb5f10aa58a4c6b6dec2e315ef7bcbfd7462`.
Before/after hashes match; no source edit/relink occurred during runtime.
Default launch and the SAME test with actual /usr/bin/busybox copied as echo
both passed1/0/0, including descriptor/raw argument/explicit-arg0 assertions.
This controls a real multicall mechanism, not the historical uutils image.
Each other exact owning filter also passed1/0/0: perfetto path shadowing,
runtime discovery, unmeasured capture sweep and sweep presentation failures.
All six logs use target/hosted-diagnosis-*; all handles are terminal. Thus the
four hosted failures are NOT reproduced or solved on this Ubuntu24 local host.

The initial temporary diagnostic workflow below was source-reviewed but NOT run;
its unsafe outer-timeout ownership gap is superseded by the next section.
The diagnostic workflow is additive; normal CI stays intact.
It builds actual ASB first on ubuntu-latest, then uses a120s owned command-group
watchdog with5s TERM-to-KILL escalation and a5min outer job bound. The static
script imports unchanged actual production definitions, not main's catch/redaction
wrapper, and tests nine fixed benign declarations with a shared90s probe budget.
Actual probe IO caps/pre-established child custody remain production-owned;
the command-group watchdog covers only this diagnostic's descendant subprocesses,
which do not request new sessions. Exact loader output is captured BEFORE its
unchanged production parser, bounded at1MiB, into private mode0600 files under
an owned mode0700 target fixture. Public rows include only fixed labels, stages,
exception classes, lengths and hashes; full tracebacks remain fixture files.
No provider/config/credential/environment values are dumped. Artifacts retain
only this benign diagnostic fixture for3days; no host source tree is uploaded.
The Process probe observes the actual diagnostic Python, while the built-ASB
program probe observes ASB file metadata, not a running Rust libtest; that scope
limitation is explicitly emitted, and original owning tests remain required.
The workflow/script have NOT run yet. Source review precedes any push/hosted
execution; no runner pin, skip, limit relaxation or production change is proposed.

## Revised owned harness and exact image reproduction (source only)

Root rejected the timeout/KILL wrapper: it could destroy the Python cleanup
owner without observing its admitted children's actual settlement. No execution
or hosted push used that candidate. The replacement is a private Linux library
test using existing ProcessLaunch/Admission/ManagedGate. Admission establishes
the real guardian and fallback keeper before Python spawn; null stdio eliminates
new pump/read threads. Its100s caller deadline requests only normal owned gate
cancellation. Exit is accepted only through ManagedGate's matching guardian
exit plus QUIESCENT. Timeout/observation failure stays an error with wait-only
guardian custody, never SIGKILL of an admitted guardian or guessed PID cleanup.
Every namespace is retained, including failed/uncertain and successful cases.
An unrelated separately admitted gated sentinel must remain live until its own
explicit cleanup before assertions. Diagnostic summaries are read only after
confirmed helper settlement, capped64KiB; no cleanup path recursively removes
diagnostic bytes. The unchanged original normal CI checks are still run, without
skips; the temporary owning diagnostic also runs as an ordinary Linux test.

The script now resides in the packaged crate's fixtures, receives explicit
crate/image/owned-root arguments, and hashes ASB in64KiB chunks under a256MiB
cap rather than read_bytes. Exact loader captures retain at most256files/16MiB
aggregate; each remains≤1MiB, and tracebacks are capped64KiB each. Summary and
trace/loader files are0600 under the owned0700 namespace. The original nine
declarations, production definitions/limits and redaction remain unchanged.

Authoritative comparison supplied by root: successful PR311 run39710024928 job
119598882816 used Ubuntu24.04.5 image ubuntu-24.04/20261018.339.1 (synthetic merge
fbe742a, tree equal00ee/c725 integration); identical-tree postmain39711305901
used Ubuntu26.04.1 image ubuntu-26.04/20261018.168.1 and failed five cases. This
establishes different environments, not the missing exception's exact cause.
The diagnostic-only job therefore requests official supported ubuntu-26.04;
normal checks remain ubuntu-latest. This is reproduction, not pinning normal CI
to Ubuntu24 to bypass failures. Label source:
https://github.com/actions/runner-images/blob/main/README.md#available-images
No runtime/build has run on this revised source; review precedes qualification.
