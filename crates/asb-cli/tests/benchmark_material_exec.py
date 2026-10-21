"""Material transport faults without Docker, credentials, agents or providers."""
import copy
import io
import json
from pathlib import Path
import subprocess
import sys
import threading
import time
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).parents[1] / "assets/benchmarks"))
from asb_material import MaterialError
import asb_material_exec as subject


def observation(selective=False):
    return {"schema_version":3, "scope":"declared-root-material-observation-v3",
        "sha256":"a"*64, "entries":2, "bytes":12, "coverage":{
            "file_content":not selective, "posix_metadata":True, "visible_inode_equivalence":True,
            "extended_attributes":False, "visible_extended_attributes":True,
            "complete_namespace":False, "atomic_snapshot":False,
            "image_anchored_selection":selective}}


def emit(value):
    return [sys.executable, "-c", "import sys; sys.stdin.buffer.read(); print(sys.argv[1])", json.dumps(value)]


class TransportTests(unittest.TestCase):
    def test_private_observation_requires_matching_exact_schema_and_coverage(self):
        for selective in [False, True]:
            roots = [{"name":"filesystem", "path":"/"}]
            if selective:
                roots[0].update(content_paths=[], image_sha256="b"*64)
            expected = observation(selective)
            self.assertEqual(subject.observe(emit(expected), roots, deadline=time.monotonic()+3), expected)
            bad = []
            for field, value in [("schema_version",True), ("entries",True), ("entries",100001),
                                  ("bytes",-1), ("sha256","B"*64), ("unknown","fixture-secret")]:
                item = copy.deepcopy(expected)
                item[field] = value
                bad.append(item)
            for field in expected["coverage"]:
                item = copy.deepcopy(expected)
                item["coverage"][field] = not item["coverage"][field]
                bad.append(item)
            for item in bad:
                with self.assertRaises(MaterialError) as error:
                    subject.observe(emit(item), roots, deadline=time.monotonic()+3)
                self.assertNotIn("fixture-secret", str(error.exception))

    def test_overflow_stdout_stderr_bad_status_and_unread_input_are_bounded(self):
        scripts = ["import sys; sys.stdout.write('x'*200000)",
                   "import sys; sys.stderr.write('fixture-secret'*20000)",
                   "import sys; sys.stderr.write('fixture-secret'); sys.exit(1)",
                   "import time; time.sleep(3)"]
        for script in scripts:
            started = time.monotonic()
            with self.assertRaises(MaterialError) as error:
                subject.run([sys.executable,"-c",script], b"x"*1024**2,
                    deadline=started+0.15)
            self.assertLess(time.monotonic()-started, 1.5)
            self.assertNotIn("fixture-secret",str(error.exception))

    def test_cancelled_and_oversized_calls_never_start_the_transport(self):
        cancelled = threading.Event()
        cancelled.set()
        for kwargs in [{"cancelled":cancelled}, {"deadline":time.monotonic()-1},
                       {"payload":b"x"*(1024**2+1)}, {"output_limit":65537}]:
            parameters = {"deadline":time.monotonic()+3, **kwargs}
            with self.assertRaises(MaterialError) as error:
                subject.run(["/must-not-start-fixture-secret"], **parameters)
            self.assertNotIn("could not start",str(error.exception))
            self.assertNotIn("fixture-secret",str(error.exception))

    def test_inflight_cancellation_stops_only_the_owned_cli_child(self):
        cancelled = threading.Event()
        timer = threading.Timer(0.1,cancelled.set)
        unrelated = subprocess.Popen([sys.executable,"-c","import time;time.sleep(5)"])
        timer.start()
        try:
            with self.assertRaises(MaterialError):
                subject.run([sys.executable,"-c","import time;time.sleep(5)"],
                    deadline=time.monotonic()+3, cancelled=cancelled)
            self.assertIsNone(unrelated.poll())
        finally:
            timer.join()
            unrelated.kill()
            unrelated.wait()

    def test_an_inherited_output_pipe_cannot_extend_the_observation_deadline(self):
        # The fixture descendant exits by itself; no unowned process discovery
        # or process-group signalling is needed to test inherited pipe lifetime.
        script = "import subprocess,sys;subprocess.Popen([sys.executable,'-c','import time;time.sleep(.5)'])"
        started = time.monotonic()
        with self.assertRaises(MaterialError):
            subject.run([sys.executable,"-c",script], deadline=started+0.1)
        self.assertLess(time.monotonic()-started, 1)

    def test_unreapable_transport_is_an_explicit_bounded_preservation_failure(self):
        child = type("StalledChild",(),{})()
        child.stdin, child.stdout, child.stderr = io.BytesIO(),io.BytesIO(),io.BytesIO()
        child.poll = lambda:None
        child.kill = lambda:None
        def wait(timeout):
            self.assertEqual(timeout,1)
            raise subprocess.TimeoutExpired("owned-fixture",timeout)
        child.wait = wait
        with patch.object(subject.subprocess,"Popen",return_value=child):
            with self.assertRaisesRegex(MaterialError,"could not be reaped.*preservation"):
                subject.run(["unused-fixture"], deadline=time.monotonic()+0.05)


if __name__ == "__main__":
    unittest.main()
