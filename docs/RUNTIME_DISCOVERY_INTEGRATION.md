# Runtime discovery integration (staged 0.2.38)

Source-only integration combines reviewed runtime discovery `f287d0e3` with
installer/recording/artifact/publication stack `9e1089d4`. Runtime Rust exchange,
Python helper/input code and focused protocol tests are unchanged by the merge.
The shared ManagedGate change remains the reviewed narrow API accepting its
owned command's pipe inputs; no collector custody work is included.

The historical 98-test focused result and red reproductions remain tied to
their original source and executable. This combined source is unbuilt and
unqualified. It needs independent integration review, fresh focused/full tests,
lint/build/docs, PR CI, merge, both post-merge jobs and a signed release.
Non-Unix legacy Rust/Python fallback routes remain outside the bounded Unix
runtime qualification. No coverage threshold or live-provider requirement is
introduced.
