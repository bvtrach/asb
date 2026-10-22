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

Root's independent complete source review approved the owning replacement.
Before its first compile, a narrow followup imports Read locally on Linux and
adds explicit test-only ASB_TEST_RUNTIME_DIAGNOSTIC_ASB for a previously built
frozen image. Default CI keeps its existing expected debug layout; this is not
universal executable discovery. Both paths require an absolute existing file,
and the script records the actual image's bounded streaming hash. Actual-FIRST
build/freeze remains mandatory. No compile failure is claimed from source review.

## Revised owning diagnostic local focus, source f3d6f5e3

Actual ASB built FIRST13.30s, then froze as
target/qualification/hosted-owned-f3d6f5e3-asb, SHA256
`0c174b0fc766af6b0545f377febfcb5f10aa58a4c6b6dec2e315ef7bcbfd7462`.
The exact owned diagnostic passed1/0/0 in0.37s using its explicit frozen-image
override. All nine fixed declarations returned ok on LOCAL Ubuntu24;106 bounded
loader files and the private summary remain under the printed0700 fixture.
The gate-settled marker follows actual matching exit/QUIESCENT and the separate
sentinel's survival/cleanup. Summary mode0600 was independently checked. No
hosted26 cause or four-failure repair is inferred from this local control.
Default and real BusyBox gated launch controls also passed1/0/0 each. Actual and
frozen before/after/final hashes match; source/executable stayed frozen during
runtime, and all handles are terminal. Post-runtime exact libtest was preserved
as qualification/hosted-owned-f3d6f5e3-libtests, SHA256
`3e4f4632eb4e020a21e1864c4448a863f347654a664172973d3a8df789b5c2ae`;
there is no pre-compilation libtest stability claim.

Strict locked/offline workspace/all-target/all-feature Clippy -Dwarnings passed
15.79s; fmt/diffcheck passed. Unfiltered actionlint reported ONLY the existing
asb-local-ci label and its older catalog's unknown ubuntu-26.04 label; full
unfiltered transcript is retained. A second actionlint invocation ignores ONLY
those two exact verified label warnings and passed. No other warning class is
suppressed and no repository lint config or normal CI command is removed.
Logs/digests use target/hosted-owned-*. This is focused harness qualification,
not a full suite or Ubuntu26 diagnosis. No revised-source compile/runtime failure
occurred; the historical postmerge five-failure log remains preserved. Independent
review precedes any push/temporary hosted diagnostic execution or repair claim.

## Actual Ubuntu26 evidence and parser regression candidate (not yet run)

Draft PR312 exact c58bb894 ran diagnostic39714125840 job119612076264 on actual
Ubuntu26.04.1 image20261018.168.1. Harness succeeded with matching gate/sentinel
marker, but six declarations recorded ExecutionError: shell, Python-no-site,
shebang, current Python process, built ASB and combined. Every private traceback
ends in unchanged asb_runtime.py217 library_paths rejecting loader metadata.
Copied true and bare Python program succeeded. All15 retained loader captures
match their summary's lengths and SHA256 digests. This is actual failing-host
production-parser evidence, not a generic guessed Python/site issue.

Loader002/010/011/012/013/015 begin with one anonymous mapping line, for example
tab-space `(0x000072fb98000000)` before valid vDSO/library/loader entries in013.
The committed fixture loader-ubuntu26-asb.txt is EXACT261-byte013, SHA256
`2b5ef0db4ee89223c334d3eab68a7a1dbe8c6f503d3426844484153fce59847d`.
Primary glibc rtld.c1925-1941 can start its trace at main_map and uses the
name/address output template; the GNU introspection manual explicitly allows
empty main-executable l_name. This supports treating the header as main-object
metadata, not inventing a library/path from an address; it does not establish
which distro change/default introduced the observed output.
https://raw.githubusercontent.com/bminor/glibc/master/elf/rtld.c
https://sourceware.org/glibc/manual/latest/html_node/Dynamic-Linker-Introspection.html

Normal PR312 CI39714125778 also failed,1208passed/4failed/13ignored; the launch
case now passes, while all four original runtime/seal-dependent cases still
fail. Complete failed log and diagnostic artifact remain under this target.
No unchanged-head CI retry, downstream-success claim or production edit follows.

Root approved a narrow design: accept only ONE exact first-nonempty anonymous
header `(0x1..16hex)`, require at least one normal resolved library entry after
it, never turn it into library identity, and preserve all malformed/unresolved/
conflicting/named/population checks. The source-only regression checkpoint adds
the exact captured positive golden, allowed-address variants and14 rejecting
cases; original legacy parser assertions remain unchanged. Independent review
precedes deterministic pure-Python baseline RED, then any production delta and
actual Ubuntu26 rerun of all four owning filters. No tests have run on this new
candidate; PR312 remains draft and no repair qualification is claimed.

## Deterministic baseline RED and unrun parser correction

At signed e2ab9817, the approved pure offline captured-format positive ran1test
and errored at UNCHANGED production asb_runtime.py217, exactly the hosted
ExecutionError. The separately selected14 negative controls plus unchanged
legacy parser assertions passed2outer/0. Logs are
target/loader-ubuntu26-baseline-red-correct-filter.log and
target/loader-ubuntu26-baseline-controls.log. An initial operator command used
wrong class RuntimeBundle (actual RuntimeTests); that loader-error transcript
is separately preserved in loader-ubuntu26-baseline-red.log and is NOT the
intended RED or a product failure. No unchanged-source CI retry occurred.

The proposed production correction only recognizes one exact first-nonempty
`(0x1..16hex)` anonymous mapping, does not insert it in results, and requires at
least one ordinary resolved library afterward. Once any nonempty line has been
seen, an anonymous line still reaches the unchanged strict rejection path.
Static/legacy input, normal library grammar, unresolved/conflicting/named checks,
64-entry bound, actual program/process binding, capture budgets, loader invocation
and redaction remain unchanged. This source has NOT been built or run; independent
root review precedes fixed-source Python/actual ASB/gated owning qualification
and any push. All original hosted/local diagnostics remain immutable evidence.

Root independently approved the complete parser delta, golden and baseline RED.
A final negative case places vDSO BEFORE the anonymous header and ordinary
library: it must reject because first-nonempty is stricter than first-resolved.
The temporary Ubuntu26 workflow now selects all four EXACT original owning
tests, checks their1passed/0failed/0ignored footer and attempts every distinct
filter once before returning a nonzero aggregate if any fails. This is not a
retry or permission to substitute diagnostic-harness success for regression
success. Normal CI remains unchanged. Root requires removal of temporary
workflow/harness/script AFTER actual26 success and before final full/package
qualification; immutable artifacts, permanent captured parser tests/fixture and
incremental0 policy remain. No new fixed-source runtime claim is made here.

## Fixed-source local focus, source10ad5f13 (Ubuntu26 rerun pending)

After product/workflow source approval, actual ASB built FIRST13.48s and froze
as target/qualification/loader-fixed-10ad5f13-asb, SHA256
`c375acf525fe23dc1fdd896c98cdb6ca3456fdaeb7e677137981b3ab783a346f`.
Complete Python runtime/parser suite passed5/0/0 with both actual-ASB opt-ins
enabled; complete runtime-input suite passed9/0/0. All four original exact Rust
owning filters passed1/0/0 each, then default/actual BusyBox launch controls
passed1/0/0 each:20focused outer passes total, no ignored cases. All15 rejecting
parser subcases and unchanged legacy assertions remain intact. This local
control does NOT replace actual Ubuntu26 proof for the four original failures.

Actual/frozen before/after/final hashes match. Post-runtime exact libtest is
preserved as qualification/loader-fixed-10ad5f13-libtests, SHA256
`b5b19066cd91c89f5c1ad1e2380c995a0064fe1871a4857e423ab0250f4e204c`.
Strict locked/offline workspace/all-target/all-feature Clippy -Dwarnings passed
15.33s; fmt/diffcheck and actionlint (ONLY the two independently verified label
catalog warnings excluded) passed. All handles are terminal; no source edit
or executable relink occurred during runtime. Logs use target/loader-fixed-*;
all baseline RED, hosted four-failure logs and real metadata artifacts remain.
Root authorized a draft-only push after these gates. Actual26 four-filter
success, removal/review of temporary diagnostics, final full/package/current-main
qualification and both postmerge jobs remain required before merge/release.

## Actual Ubuntu26 repaired regression proof and diagnostic retirement

Exact4d04924d ran39715179497 job119615456855 on Ubuntu26.04.1 image
20261018.168.1 (synthetic f2e590f merge into actual a32). All nine declarations
now report ok. More importantly, ALL FOUR original full exact owning filters
passed independently1/0/0: perfetto0.14s, runtime-inputs0.27s, unmeasured
capture sweep12.34s and sweep presentation23.01s. Root independently retrieved
the full job log and accepted this failing-host obligation. This is not broad
full/package or actual postmerge clearance. The old five/four-failure logs,
strict baseline RED, original15 loader captures and successful repaired artifact
remain immutable locally; every captured loader length/digest was verified.

After actual26 success and with all own runtime handles terminal, the temporary
diagnostic workflow, private Rust diagnostic test and Python script are removed
as instructed. They remain recoverable in signed Git/PR history and downloaded
artifacts; no evidence, source-local diagnostic namespace or cache was deleted.
runtime_inputs.rs is byte-identical to actual main a32. Net-vs-a32 source now
contains ONLY15-line Python parser correction,17-line permanent launch-fixture
correction,33-line permanent parser tests plus exact261-byte captured fixture,
the independently reviewed5-line CI incremental0 policy and this evidence ledger.
Cargo remains unreleased0.2.44. Normal CI is not runner-pinned or weakened;
there is no new runtime helper, diagnostic entry point, optional env hook beyond
the permanent test-only BusyBox image opt-in, skip or budget relaxation.

This stripped source checkpoint has NOT been built or run. Independent net-diff
review precedes fresh actual-FIRST/frozen final full/package qualification and
any new push. PR312 stays draft and root owns eventual merge/postmerge/release.
