"""Offline regression tests for the mandatory production coverage gate."""

import unittest
import os
from pathlib import Path
import subprocess
import tempfile
import re
import json

from check_coverage import check, executable_lines, dependency_path_regex


def report(*counts):
    return "/repo/src/module.rs:\n" + "".join(
        f"{line:5}|{count:>7}|source\n" for line, count in enumerate(counts, 1)
    )


def aggregate(covered=90, count=100):
    return {"data": [{"totals": {"lines": {"covered": covered, "count": count}}}]}


def metadata(root):
    return {"workspace_members": ["workspace"], "packages": [
        {"id": "cached", "source": "registry+fixture", "manifest_path": str(root / "Cargo.toml")},
        {"id": "workspace", "source": None, "manifest_path": "/workspace/Cargo.toml"},
        {"id": "local", "source": None, "manifest_path": "/local/Cargo.toml"},
    ]}


class CoverageGateTests(unittest.TestCase):
    def test_dependency_paths_follow_custom_cargo_home_without_excluding_product(self):
        for home in ("/fixture/.cargo/registry/src/pkg", "/fixture/cache.with+[characters] cargo/git/checkouts/pkg"):
            value = metadata(Path(home))
            pattern = dependency_path_regex(value)
            self.assertIsNotNone(re.search(pattern, home + "/src/lib.rs"))
            for path in ("/workspace/src/main.rs", "/local/src/lib.rs",
                         "/product/.cargo/src/main.rs", "/product/src/main.rs",
                         home + "-other/src/lib.rs"):
                self.assertIsNone(re.search(pattern, path), path)

    def test_nested_workspace_and_local_paths_fail_closed_including_symlinks(self):
        target = Path(__file__).resolve().parent.parent / "target"
        target.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="coverage-overlap-", dir=target) as name:
            cached = Path(name) / "cached"
            nested = cached / "nested"
            nested.mkdir(parents=True)
            alias = Path(name) / "alias"
            alias.symlink_to(nested, target_is_directory=True)
            for package_index in (1, 2):
                for location in (nested, alias):
                    value = metadata(cached)
                    value["packages"][package_index]["manifest_path"] = str(location / "Cargo.toml")
                    with self.assertRaisesRegex(ValueError, "hide workspace or local"):
                        dependency_path_regex(value)

    def test_dependency_discovery_rejects_invalid_metadata(self):
        for source, path in (("unknown+fixture", "/fixture/Cargo.toml"),
                             ("git+fixture", "relative/Cargo.toml")):
            value = metadata(Path("/fixture"))
            value["packages"][0].update(source=source, manifest_path=path)
            with self.assertRaises(ValueError):
                dependency_path_regex(value)

    def test_dependency_regex_works_with_real_llvm_and_symlink_cache(self):
        target = Path(__file__).resolve().parent.parent / "target"
        target.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="coverage-paths-", dir=target) as name:
            root = Path(name)
            cached = root / "cache.with+[characters] cargo" / "package"
            cached.mkdir(parents=True)
            linked = root / "linked cache"
            linked.symlink_to(cached, target_is_directory=True)
            local = root / "local dependency"
            local.mkdir()
            for directory in (cached, local):
                (directory / "lib.rs").write_text("pub fn called() -> u64 { 42 }\n")
            source = root / "main.rs"
            source.write_text(
                f"#[path={json.dumps(str(cached / 'lib.rs'))}] mod cached;\n"
                f"#[path={json.dumps(str(local / 'lib.rs'))}] mod local;\n"
                "fn main() { assert_eq!(cached::called(), local::called()); }\n"
            )
            binary = root / "fixture"
            subprocess.run(["rustc", "-C", "instrument-coverage", str(source), "-o", str(binary)],
                           check=True, capture_output=True)
            raw = root / "fixture.profraw"
            subprocess.run([str(binary)], env={**os.environ,"LLVM_PROFILE_FILE":str(raw)}, check=True)
            sysroot = subprocess.check_output(["rustc","--print","sysroot"], text=True).strip()
            version = subprocess.check_output(["rustc","-vV"], text=True)
            host = next(line.removeprefix("host: ") for line in version.splitlines() if line.startswith("host: "))
            tools = Path(sysroot) / "lib/rustlib" / host / "bin"
            merged = root / "fixture.profdata"
            subprocess.run([str(tools / "llvm-profdata"), "merge", "-sparse", str(raw), "-o", str(merged)],
                           check=True, capture_output=True)
            pattern = dependency_path_regex(metadata(linked))
            self.assertIsNotNone(re.search(pattern, str(linked / "lib.rs")))
            result = subprocess.check_output([
                str(tools / "llvm-cov"), "show", f"-instr-profile={merged}", str(binary),
                f"-ignore-filename-regex={pattern}", "-show-line-counts-or-regions=false", "-show-line-counts",
            ], text=True)
            lines = executable_lines(result)
            self.assertNotIn((str(cached / "lib.rs"), 1), lines)
            self.assertIn((str(local / "lib.rs"), 1), lines)
            self.assertIn((str(source), 3), lines)

    def test_failed_checker_cannot_leave_stale_green_reports(self):
        repository = Path(__file__).resolve().parent.parent
        target = repository / "target"
        target.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="coverage-failed-checker-", dir=target) as name:
            root = Path(name)
            scripts = root / "scripts"
            scripts.mkdir()
            script = scripts / "check-coverage.sh"
            script.write_text((repository / "scripts/check-coverage.sh").read_text())
            output = root / "target/coverage"
            output.mkdir(parents=True)
            reports = ["all.json","all-lines.txt","production-lines.txt","summary.json"]
            for filename in reports:
                (output / filename).write_text("stale green result")
            proof = output / "required-main-checks.json"
            proof.write_text("unrelated proof must survive")
            binaries = root / "bin"
            binaries.mkdir()
            for name in ("dirname","mkdir","rm"):
                (binaries / name).symlink_to(Path("/usr/bin") / name)
            (binaries / "python3").symlink_to("/bin/false")
            completed = subprocess.run(
                ["/bin/bash",str(script)], cwd=root,
                env={"PATH":str(binaries),"HOME":str(root)},
                capture_output=True,text=True,timeout=10,
            )
            self.assertNotEqual(completed.returncode,0)
            self.assertFalse(any((output / filename).exists() for filename in reports))
            self.assertEqual(proof.read_text(),"unrelated proof must survive")

    def test_workspace_clean_removes_old_version_instrumented_objects(self):
        repository = Path(__file__).resolve().parent.parent
        target = repository / "target"
        target.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="coverage-clean-", dir=target) as name:
            root = Path(name)
            (root / "src").mkdir()
            (root / "home").mkdir()
            (root / "src/lib.rs").write_text(
                "pub fn answer() -> u64 { 42 }\n"
                "#[test] fn works() { assert_eq!(answer(),42); }\n"
            )
            original_home = Path.home()
            environment = {
                "PATH": os.environ["PATH"],
                "HOME": str(root / "home"),
                "CARGO_HOME": str(root / "cargo-home"),
                "RUSTUP_HOME": os.environ.get("RUSTUP_HOME",str(original_home / ".rustup")),
                "CARGO_TARGET_DIR": str(root / "normal-build"),
                "CARGO_LLVM_COV_TARGET_DIR": str(root / "coverage-build"),
                "CARGO_BUILD_JOBS": "2",
                "CARGO_PROFILE_DEV_DEBUG": "0",
                "CARGO_PROFILE_TEST_DEBUG": "0",
                "CARGO_INCREMENTAL": "0",
                "CARGO_TERM_COLOR": "never",
                "LLVM_PROFILE_FILE": str(root / "own-profile-%p-%m.profraw"),
            }
            if "RUSTUP_TOOLCHAIN" in os.environ:
                environment["RUSTUP_TOOLCHAIN"] = os.environ["RUSTUP_TOOLCHAIN"]
            manifest = root / "Cargo.toml"
            def package(version):
                manifest.write_text(
                    f"[package]\nname='coverage_clean_fixture'\nversion='{version}'\n"
                    "edition='2024'\n[workspace]\n"
                )
            def run(*arguments):
                completed = subprocess.run(
                    ["cargo","llvm-cov",*arguments,"--manifest-path",str(manifest),"--offline"],
                    cwd=root, env=environment, capture_output=True, text=True, timeout=60,
                )
                self.assertEqual(completed.returncode,0,completed.stdout+completed.stderr)
            package("0.0.1")
            run("--no-report")
            objects = list((root / "coverage-build/debug/deps").glob("coverage_clean_fixture-*"))
            old_binaries = [path for path in objects if path.is_file() and os.access(path,os.X_OK)]
            self.assertTrue(old_binaries,"fixture must create an actual instrumented test executable")
            package("0.0.2")
            run("clean","--workspace")
            self.assertFalse(any(path.exists() for path in old_binaries),
                             "old versioned coverage maps must not remain eligible for reports")
            run("--no-report")
            self.assertTrue(list((root / "coverage-build/debug/deps").glob("coverage_clean_fixture-*")))
            self.assertFalse(any(path.exists() for path in old_binaries),
                             "fresh version must not recreate the old instrumented binary hashes")

    def test_real_compiler_preserves_never_called_production_function_in_map(self):
        # Test LLVM's actual dead-code-map behavior, not just file presence or a
        # hand-written report. This fixture is outside the product coverage map.
        repository = Path(__file__).resolve().parent.parent
        target = repository / "target"
        target.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="coverage-checker-", dir=target) as name:
            root = Path(name)
            source = root / "fixture.rs"
            source.write_text(
                "#![allow(dead_code)]\n"
                "#[inline(never)]\n"
                "fn never_called(x: u64) -> u64 {\n"
                "    x + 42\n"
                "}\n"
                "fn main() { println!(\"called\"); }\n"
            )
            binary = root / "fixture"
            subprocess.run(["rustc", "-C", "instrument-coverage", "-C", "opt-level=0",
                            "--crate-name", "coverage_checker_fixture", str(source),
                            "-o", str(binary)], check=True, capture_output=True)
            profile = root / "fixture.profraw"
            subprocess.run([str(binary)], env={**os.environ,"LLVM_PROFILE_FILE":str(profile)},
                           check=True, capture_output=True)
            sysroot = subprocess.check_output(["rustc","--print","sysroot"], text=True).strip()
            version = subprocess.check_output(["rustc","-vV"], text=True)
            host = next(line.removeprefix("host: ") for line in version.splitlines()
                        if line.startswith("host: "))
            tools = Path(sysroot) / "lib/rustlib" / host / "bin"
            merged = root / "fixture.profdata"
            subprocess.run([str(tools / "llvm-profdata"),"merge","-sparse",str(profile),
                            "-o",str(merged)], check=True, capture_output=True)
            report_text = subprocess.check_output([
                str(tools / "llvm-cov"),"show",f"-instr-profile={merged}",str(binary),
                "-show-line-counts-or-regions=false","-show-line-counts"
            ],text=True)
            # A single-file LLVM show omits its filename header.
            lines = executable_lines(f"{source}:\n" + report_text)
            self.assertIn((str(source),4),lines,"uncalled body must remain in denominator")
            self.assertFalse(lines[(str(source),4)])
            self.assertTrue(lines[(str(source),6)])

    def test_test_bodies_cannot_inflate_production_coverage(self):
        all_lines = report("1", "0", *(["1"] * 98))
        production = report("1", "0")
        result = check(all_lines, production, aggregate(99), 90)
        self.assertFalse(result["passed"])
        self.assertEqual(result["production"]["percent"], 50)

    def test_exact_threshold_passes_but_fraction_below_fails(self):
        production = report(*(["0"] * 1000))
        self.assertTrue(check(report(*(["1"] * 900 + ["0"] * 100)), production,
                              aggregate(), 90)["passed"])
        self.assertFalse(check(report(*(["1"] * 899 + ["0"] * 101)), production,
                               aggregate(), 90)["passed"])

    def test_aggregate_must_pass_independently(self):
        self.assertFalse(check(report("1"), report("1"), aggregate(89), 90)["passed"])

    def test_missing_or_empty_maps_fail_closed(self):
        for all_lines, production in [("", ""), ("", report("1"))]:
            with self.assertRaises(ValueError):
                check(all_lines, production, aggregate(), 90)

    def test_instantiation_counts_union_and_blank_source_is_not_executable(self):
        lines = executable_lines(report("0", "", "1.2k") + report("2", "", "0"))
        self.assertEqual(lines, {("/repo/src/module.rs", 1): True,
                                 ("/repo/src/module.rs", 3): True})

    def test_invalid_aggregate_fails_closed(self):
        for value in (aggregate(0, 0), aggregate(2, 1), aggregate(-1)):
            with self.assertRaises(ValueError):
                check(report("1"), report("1"), value, 90)


if __name__ == "__main__":
    unittest.main()
