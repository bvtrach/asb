#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

# A separate, bounded profile avoids interfering with ordinary build artifacts.
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
output=target/coverage
mkdir -p "$output"
# A failed checker/build must never leave an earlier green result looking current.
# Preserve unrelated evidence stored alongside these four generated reports.
rm -f -- "$output/all.json" "$output/all-lines.txt" \
  "$output/production-lines.txt" "$output/summary.json"
python3 scripts/test_check_coverage.py
# Discard execution profiles AND old workspace object maps. Version/feature
# changes create new binary hashes; retaining obsolete maps distorts coverage
# even with fresh profiles. Dependency compilation caches remain reusable.
cargo llvm-cov clean --workspace
cargo llvm-cov --locked --workspace --all-targets --no-report
cargo llvm-cov report --json --output-path "$output/all.json"
cargo llvm-cov report --text --output-path "$output/all-lines.txt"

host=$(rustc -vV | sed -n 's/^host: //p')
llvm_cov="$(rustc --print sysroot)/lib/rustlib/$host/bin/llvm-cov"
mapfile -t profiles < <(find target/llvm-cov-target -maxdepth 1 -name '*.profdata' -type f)
if [[ ${#profiles[@]} != 1 ]]; then
  echo 'Expected exactly one merged LLVM coverage profile.' >&2
  exit 1
fi
# Only the non-test executable defines the production line denominator. The
# full report above supplies counts, including coverage gained by unit tests.
"$llvm_cov" show \
  -instr-profile="${profiles[0]}" \
  target/llvm-cov-target/debug/asb \
  -ignore-filename-regex='/rustc/|/\.cargo/|/\.rustup/' \
  -show-line-counts-or-regions=false -show-line-counts \
  > "$output/production-lines.txt"
python3 scripts/check_coverage.py \
  --all-lines "$output/all-lines.txt" \
  --production-lines "$output/production-lines.txt" \
  --aggregate "$output/all.json" --output "$output/summary.json"
