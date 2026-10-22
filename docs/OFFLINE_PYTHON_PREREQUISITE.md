# Offline Python prerequisite and hosted fixture diagnosis

Scope: TEST/CI prerequisites, not a change to product initialization or a
production bootstrap skip flag. Baseline975ad96827cb844f932ecd973f979f9b18255ab5
retains the accepted stripped-loader repair and its local proof. Installer
aa122d4a and its cache/evidence are parked separately and unchanged.

## Observed hosted failures

Normal Ubuntu26 CI39715179418 passed its library tests, then offline_cli failed
four cases (94passed/4failed/5ignored). The retained complete log is in the
separate diagnosis checkout. SOCKS fixtures received a greeting but reached EOF
after returning method selection, before the CONNECT request. The HTTPS fixture
received a different request prefix than its expected provider CONNECT. The
reset fixture's supposedly empty .asb-tools directory was nonempty.

Source263bdb5/975ad968 calls python_runtime::ensure during project::init after
persisting the detected proxy. Compatible means real Python3.12 or3.13; absent
compatible interpreters can bootstrap uv under .asb-tools/uv/cache, then install
Python under .asb/python. All three failing proxy listeners bind BEFORE init
but do not accept/serve until AFTER init. Unexpected bootstrap traffic could
therefore be queued, expired and consumed as if it were provider traffic.
The hosted log does not include actual init output or unexpected CONNECT target;
historical bootstrap-target attribution and actual hosted Python version remain
unproven. No connection is to be skipped or accepted under a widened protocol.

## Controlled evidence fixture

The owning Linux python_runtime fixture reuses helper_exchange's bounded native
gate, 64KiB request/256KiB response and30s exchange allowance. Its harness is
REAL Python3.12 or3.13, never a globally faked compatible runtime. It invokes the actual
current built ASB and checks two isolated projects. Fake-only3.14 PATH/no uv
uses a marker-only curl which records actual bootstrap arguments and exits55;
it opens NO socket and downloads nothing. Init output and marker/cache paths
must prove the bootstrap attempt and .asb-tools population. A paired control
exposes that real supported version ahead of the same fake-only PATH, creates
the actual project venv and compares it with the observed harness version,
with no curl marker or uv namespace. ASB_TEST_PYTHON_BOOTSTRAP_ASB can select an
explicit absolute frozen ASB fixture; otherwise the current built binary beside
the libtest is selected. A local3.12 run does not qualify native3.13.
Both children have private cleared environments, empty stdin and finite waits;
only LLVM_PROFILE_FILE instrumentation may be inherited. All stdout/stderr,
observed JSON and markers are intentionally retained after success or failure.
Owned-tree quiescence precedes interpretation. This establishes bootstrap/tools
mutation causality, not the unobserved old hosted proxy's precise target.
This source fixture has NOT been executed; build-FIRST/source review is required.

## Approved repair plan after evidence

Provision real Python3.12 for the GitHub-hosted ordinary and packaged test paths
with the official setup-python action. Document real Python3.12/3.13 with venv
support and Node22 as offline-test prerequisites, rather than relying on init to
download an interpreter. A test-only factual early prerequisite guard should
reject an absent/incompatible/broken supported runtime with a precise remedy
before ordinary Project init can bootstrap. It must not fake the version or
silently skip tests. Keep intentional incompatible-Python/fake-uv/fake-only-PATH
fixtures authoritative; the existing real uv download remains explicitly opt-in.

The native runner must be checked read-only for an already usable interpreter
and tool-cache permissions. Do not assume setup-python can install there, use
sudo, change host settings or weaken job isolation. Its job may need only a
bounded real-runtime prerequisite check. Explicit PATH fixture overrides remain
last in Project invocation; fixture prepend-plus-host paths retain the real
runtime, while intentionally fake-only paths keep their negative semantics.

No production code, hosted retries, provider/SDK/real download, full-suite or
release claim is included in this controlled evidence checkpoint. The removed
90% coverage gate remains removed.

## Controlled evidence qualification

Root reviewed da4f7106 and approved the supported-runtime/frozen-path alignment
in signed a5a1ab9b. Actual FIRST build in the exclusive new HOME cache passed
13.02s; only the verified-idle OWN installer debug artifacts had been copied,
without modifying the parked installer source/cache or borrowing a peer cache.
Actual/frozen ASB SHA256 before, after focus and after strict checks matched:
`b7f383b30c6dda784cce498768c369e229dbd8c3fe5379f06a77175ec422b909`.
The frozen copy is target/qualification/offline-python-a5a1ab9b-asb.

The exact controlled pair passed1/0/0 in0.18s after22.28s libtest compilation.
Its retained evidence is
target/python-bootstrap-offline/18e0bc6a5d4b8b67-323133-0/observed.json,
with separate init.stdout/init.stderr and bootstrap-arguments files. Actual
fake-only3.14 init returned0 with a Python warning naming the attempted official
uv archive and controlled curl55 failure; the marker includes actual download
argv and a .asb-tools/uv/cache staging output. That tools directory was nonempty.
Paired real3.12 init returned0 without the warning, created an actual version3.12
venv, never invoked inert curl and left tools empty/no uv namespace. Declined
host prompts and unavailable sudo caused no host changes. No socket, external
download, provider or SDK was used. This proves bootstrap/tools mutation under
the controlled conditions; it still does not identify the unobserved historical
CI proxy request or assert CI's actual interpreter version.

Sequential existing controls passed: Python boundary2/0/0 and path1/0/0
(each0.00s), actual fake-uv bootstrap1/0/0(0.12s), missing-runtime warning1/0/0
(0.02s), and incompatible tool-venv preservation1/0/0(1.86s). Together these
are7 distinct top-level focused tests,0failures/0ignored. The separate real
download opt-in was NOT run. Strict workspace/all-target/all-feature Clippy
passed15.26s, fmt/diff checks passed. No source changes occurred during runtime.
No native3.13, broad full-suite, current hosted or release qualification is claimed.

Read-only native-runner checks found asb-local-ci.service using gha-asb with
ProtectSystem=strict, ProtectHome=yes, NoNewPrivileges=yes, read-only /opt and
only its own /srv/data/ci-runners/asb bind writable. Standard hostedtoolcache
does not exist; no right to provision it is assumed. Existing globally readable
/usr/bin/python3.12 reports3.12.3 and imports venv. No sudo, service/account
change or host install was performed. Prefer a bounded prerequisite check on
the native job, official provisioning only on the GitHub-hosted job. CI and
test-guard repair remains source work AFTER this observed evidence checkpoint.
