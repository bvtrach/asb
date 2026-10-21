"""Actual ASB controller interruption and adapter-dispatched CLI recovery.

Uses a cached original task and replay-only transport, never a live provider.
"""
import io
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tarfile
import tempfile
import time
import unittest

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).parents[1] / "assets/benchmarks"))


@unittest.skipUnless(os.environ.get("ASB_TEST_BENCHMARK_ASB") and os.environ.get("ASB_TEST_SWECATALOG_ROOT"),
                     "requires explicit original task, ASB binary and Docker opt-in")
class LiveControllerRecovery(unittest.TestCase):
    def test_recorded_asb_attempt_can_be_quiesced_after_its_controller_is_killed(self):
        from asb_docker import DockerSession
        root = Path(os.environ["ASB_TEST_SWECATALOG_ROOT"]).absolute()
        binary = Path(os.environ["ASB_TEST_BENCHMARK_ASB"]).absolute()
        artifacts = Path(tempfile.mkdtemp(prefix="asb-controller-recovery-",dir=root))
        print("Retained ASB recovery qualification: " + str(artifacts),flush=True)
        config, workspace = artifacts / "benchmark.toml", artifacts / "workspace"
        config.write_text("agent='pi'\nworkload='swe-bench-lite'\ntrial='astropy__astropy-12907'\nprovider='cli2key'\nmodel='test-model'\n"
                          + "workspace=" + json.dumps(str(workspace)) + "\nlog_file=" + json.dumps(str(artifacts / "run.log")) + "\n")
        tape = artifacts / "recording.jsonl"
        tape.write_text(json.dumps({"request":{"method":"POST","url":"/v1/responses","body":json.dumps({"model":"test-model","input":"fixture","tools":[{"type":"function"}]})},
            "response":{"status":200,"headers":{"content-type":"application/json"},"body":"{}"}}) + "\n")
        environment = {"PATH":os.environ.get("PATH",""),"HOME":os.environ.get("HOME",""),
                       "PYTHONDONTWRITEBYTECODE":"1","NPM_CONFIG_OFFLINE":"true"}
        # Docker host/context selection is infrastructure, not provider auth.
        for key in ["DOCKER_HOST","DOCKER_CONTEXT","DOCKER_CONFIG","DOCKER_TLS_VERIFY","DOCKER_CERT_PATH"]:
            if key in os.environ:
                environment[key] = os.environ[key]
        process = subprocess.Popen([binary,"run",config,"--replay","--cassette",tape,"--json"],cwd=root,
            env=environment,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        owned, descriptor, journal, reference_path = None, None, None, None
        with DockerSession() as client:
            try:
                deadline = time.monotonic() + 60
                while time.monotonic() < deadline:
                    if process.poll() is not None:
                        out, err = process.communicate()
                        self.fail("ASB exited before recoverable allocation: " + err.decode(errors="replace"))
                    references = list((workspace / ".asb-attempts").glob("*.json"))
                    if references:
                        reference_path = references[0]
                        reference = json.loads(reference_path.read_bytes())
                        path = Path(reference["artifacts"]) / "resources.json"
                        try:
                            journal = json.loads(path.read_bytes())
                        except (FileNotFoundError,json.JSONDecodeError):
                            time.sleep(0.02)
                            continue
                        declarations = [item for item in journal["resources"] if item["kind"] == "containers"]
                        if declarations:
                            try:
                                candidate = client.containers.get(declarations[0]["name"])
                            except Exception as error:
                                if getattr(error,"status_code",None) != 404:
                                    raise
                            else:
                                labels = candidate.attrs.get("Config",{}).get("Labels",{}) or {}
                                if candidate.attrs["State"]["Running"] and labels.get("asb.recovery") == journal["token"]:
                                    owned = candidate
                                    break
                    time.sleep(0.02)
                self.assertIsNotNone(owned,"owned task environment did not become observable")
                # Pin the exact child lifetime before signalling it. No name-
                # based or host-wide process search is used.
                children = Path(f"/proc/{process.pid}/task/{process.pid}/children").read_text().split()
                for child in children:
                    candidate_fd = os.pidfd_open(int(child))
                    try:
                        arguments = Path(f"/proc/{child}/cmdline").read_bytes().split(b"\0")
                        parent = next(line.split()[1] for line in Path(f"/proc/{child}/status").read_text().splitlines() if line.startswith("PPid:"))
                        if parent == str(process.pid) and os.fsencode(reference["artifacts"]) in arguments and b"--artifacts" in arguments:
                            descriptor = candidate_fd
                            break
                    finally:
                        if descriptor != candidate_fd:
                            os.close(candidate_fd)
                self.assertIsNotNone(descriptor,"allocated controller was not an owned direct child")
                self.assertEqual(owned.exec_run(["/bin/sh","-c","printf retained > /tmp/asb-recovery-partial"]).exit_code,0)
                signal.pidfd_send_signal(descriptor,signal.SIGKILL)
                out, err = process.communicate(timeout=45)
                (artifacts / "asb.stdout").write_bytes(out)
                (artifacts / "asb.stderr").write_bytes(err)
                self.assertNotEqual(process.returncode,0)
                recovered = subprocess.run([binary,"workload","recover",reference_path,"--json"],cwd=root,
                    env=environment,capture_output=True,timeout=60)
                self.assertEqual(recovered.returncode,0,recovered.stderr.decode())
                report = json.loads(recovered.stdout)
                self.assertEqual(report["attempt"],journal["attempt"])
                self.assertEqual(report["resources"][0]["status"],"stopped-preserved")
                self.assertEqual(report["scope"],"observed-resource-quiescence-not-a-retry-receipt")
                owned.reload()
                self.assertFalse(owned.attrs["State"]["Running"])
                chunks, _ = owned.get_archive("/tmp/asb-recovery-partial")
                with tarfile.open(fileobj=io.BytesIO(b"".join(chunks))) as archive:
                    self.assertEqual(archive.extractfile("asb-recovery-partial").read(),b"retained")
                self.assertEqual(len(list((workspace / ".asb-attempts").glob("*.json"))),1,"recovery must not create a new attempt")
                self.assertFalse((Path(reference["artifacts"]) / "submission.json").exists(),"resource recovery must not fabricate a submission or grade")
            finally:
                if descriptor is not None:
                    try:
                        signal.pidfd_send_signal(descriptor,signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                if process.poll() is None:
                    # Failure cleanup is limited to pinned direct children of
                    # this fixture's still-live ASB process, never host names.
                    children = Path(f"/proc/{process.pid}/task/{process.pid}/children").read_text().split()
                    for child in children:
                        child_fd = None
                        try:
                            child_fd = os.pidfd_open(int(child))
                            parent = next(line.split()[1] for line in Path(f"/proc/{child}/status").read_text().splitlines() if line.startswith("PPid:"))
                            if parent == str(process.pid):
                                signal.pidfd_send_signal(child_fd,signal.SIGKILL)
                        except (ProcessLookupError,FileNotFoundError):
                            pass
                        finally:
                            if child_fd is not None:
                                os.close(child_fd)
                    process.kill()
                    process.wait(timeout=5)
                if descriptor is not None:
                    os.close(descriptor)
                process.stdout.close()
                process.stderr.close()
                if owned is not None:
                    labels = owned.attrs.get("Config",{}).get("Labels",{}) or {}
                    if labels.get("asb.attempt") != journal["attempt"] or labels.get("asb.recovery") != journal["token"]:
                        raise AssertionError("fixture refused cleanup of another owner's container")
                    owned.remove(force=True)


if __name__ == "__main__":
    unittest.main()
