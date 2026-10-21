"""Opt-in owned container qualification; fixture client, no provider or grading."""
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile
import time
from types import SimpleNamespace
import unittest
import uuid

sys.dont_write_bytecode = True
sys.path.insert(0,str(Path(__file__).parents[1]/"assets/benchmarks"))


@unittest.skipUnless(os.environ.get("ASB_TEST_BENCHMARK_ASB") and os.environ.get("ASB_TEST_STAGING_IMAGE")
    and os.environ.get("ASB_TEST_SWECATALOG_ROOT"),"requires frozen ASB and explicitly selected cached container image")
class ConfiguredUserQualification(unittest.TestCase):
    def test_nonroot_runtime_reuse_root_cwd_and_actual_driver_keep_user_and_environment(self):
        from asb_docker import DockerSession
        from asb_execution import ExecutionError,ManagedDockerWorkspace
        from asb_runtime import bundle
        from asb_staging import prepare_guest_roots,root_execute,stage_runtime,upload
        from asb_material_root import prepare as prepare_material
        binary = Path(os.environ["ASB_TEST_BENCHMARK_ASB"]).absolute()
        binary_hash = hashlib.sha256(binary.read_bytes()).hexdigest()
        sources = sorted((Path(__file__).parents[1]/"assets/benchmarks").glob("asb_*.py"))
        hashes = {path.name:hashlib.sha256(path.read_bytes()).hexdigest() for path in sources}
        artifacts = Path(tempfile.mkdtemp(prefix="configured-user-staging-",
            dir=Path(os.environ["ASB_TEST_SWECATALOG_ROOT"])))
        print(f"Retained nonroot staging qualification: {artifacts}",flush=True)
        attempt = "asb-"+uuid.uuid4().hex
        success,container = False,None
        with DockerSession() as client:
            try:
                image = client.images.get(os.environ["ASB_TEST_STAGING_IMAGE"])
                container = client.containers.create(image.id,command=["/bin/sh","-c","exec tail -f /dev/null"],
                    user="1001:1002",working_dir="/",environment={"HOME":"/tmp","ASB_USER_MARKER":"preserved"},
                    network_mode="none",labels={"asb.owner":"asb","asb.attempt":attempt},
                    mem_limit=512*1024**2,nano_cpus=10**9,pids_limit=128)
                container.start(); container.reload()
                workspace = ManagedDockerWorkspace.attach(client,container,attempt,"/")
                task = SimpleNamespace(client=client,workspace=workspace,attempt=attempt,artifacts=artifacts,
                    cancelled=None,journal=SimpleNamespace(record=dict(resources=[]),labels=dict(container.attrs["Config"]["Labels"])),
                    invocation=dict(identity=dict(workload="terminal-bench-1.0",task_id="fixture",
                        attempt_id=attempt,dataset=None)))
                def checked(arguments):
                    result = container.exec_run(arguments)
                    if result.exit_code != 0:
                        raise ExecutionError("configured-user qualification command failed")
                    return result.output
                deadline = time.monotonic()+180
                def privileged(arguments):
                    return root_execute(task,arguments,deadline=deadline)
                files,_ = bundle(binary,image.attrs["Architecture"])
                owner = stage_runtime(task,binary,files,checked,privileged,deadline=deadline)
                self.assertEqual(owner,(1001,1002))
                # The capture-to-bootstrap handoff must verify and reuse bytes,
                # not silently restage or chown an existing runtime.
                self.assertEqual(stage_runtime(task,binary,files,checked,privileged,deadline=deadline),owner)
                # The observer, unlike the client, must see protected root
                # material. Its privilege is scoped to this owned fixture.
                privileged(["/bin/sh","-c","mkdir -m 700 /root/asb-staging-fixture && printf initial > /root/asb-staging-fixture/input"])
                with self.assertRaises(ExecutionError):checked(["cat","/root/asb-staging-fixture/input"])
                sampler = prepare_material(task,[],binary,deadline)
                baseline = sampler()
                self.assertEqual(baseline,sampler())
                privileged(["/bin/sh","-c","printf changed > /root/asb-staging-fixture/input"])
                changed_material = sampler()
                self.assertNotEqual(baseline["files"]["sha256"],changed_material["files"]["sha256"])
                identity,receipt = prepare_guest_roots(task,owner,checked,privileged)
                checked(["mkdir","-p","/opt/asb-project/.asb","/opt/asb-project/.asb-tools/pi/bin"])
                upload(container,"/opt/asb-project/.asb",[("project.toml",
                    b"tools_dir='.asb-tools'\nworkloads_dir='workloads'\nworkspace_dir='workspace'\n",0o600)],owner=owner)
                fixture = b'''#!/bin/sh
test "$(id -u):$(id -g):$PWD:$HOME:$ASB_USER_MARKER" = '1001:1002:/:/tmp:preserved' || exit 91
test -z "$ASB_GUEST_AGENT_REQUEST" || exit 92
printf '%s\\n' '{"type":"result","result":"nonroot-root-cwd-fixture"}'
'''
                upload(container,"/opt/asb-project/.asb-tools/pi/bin",[("pi",fixture,0o755)],owner=owner)
                guest = dict(task_identity=identity,owned_instrumentation=receipt,
                    inherit_working_directory=True,config=dict(agent="pi",provider="openrouter",model="fixture",
                        workload="terminal-bench-1.0",workspace="/",concurrency=1),
                    connection=dict(interface="open_ai_chat",authentication="bearer",
                        base_url="http://127.0.0.1:1/v1",credential="fixture-not-a-real-key"),
                    prompt="fixture client; no benchmark score",project_root="/opt/asb-project",
                    runtime_dir="/opt/asb-private",run_dir="/opt/asb-private/attempt")
                result = workspace.execute(["/opt/asb-runtime/asb","__asb-agent"],
                    environment={"ASB_GUEST_AGENT_REQUEST":json.dumps(guest)},gate="application",
                    preparation_timeout=90,timeout=15)
                self.assertEqual(result["exit_code"],0,result["stderr"].decode(errors="replace"))
                metadata = json.loads(checked(["cat","/opt/asb-private/attempt/agent-result.json"]))
                self.assertEqual(metadata["final_response"],"nonroot-root-cwd-fixture")
                self.assertEqual(metadata["execution_workspace"],"/")
                self.assertEqual(metadata["owned_instrumentation"],receipt)
                self.assertIs(metadata["tree_quiesced"],True)
                self.assertIsNone(metadata["execution_failure"])
                self.assertEqual(checked(["/bin/sh","-c","printf '%s:%s:%s' \"$(id -u)\" \"$(id -g)\" \"$HOME\""]),b"1001:1002:/tmp")
                self.assertEqual(hashes,{path.name:hashlib.sha256(path.read_bytes()).hexdigest() for path in sources})
                self.assertEqual(binary_hash,hashlib.sha256(binary.read_bytes()).hexdigest())
                evidence = dict(scope="owned-container-nonroot-staging-fixture",
                    container=container.id,image=image.id,owner=owner,source_hashes=hashes,binary_sha256=binary_hash,
                    result=metadata,provider_called=False,original_sdk=False,grader_started=False)
                evidence["protected_material"] = dict(baseline=baseline["files"]["sha256"],
                    changed=changed_material["files"]["sha256"],configured_user_denied=True)
                success = True
            finally:
                if container is not None:
                    container.stop(timeout=3)
                    if success:
                        from docker.errors import NotFound
                        container.remove()
                        with self.assertRaises(NotFound):client.containers.get(container.id)
                    else:print(f"Preserved stopped qualification container: {container.id}",flush=True)
        evidence["cleanup"] = "removed-confirmed"
        (artifacts/"qualification.json").write_text(json.dumps(evidence,sort_keys=True)+"\n")


if __name__ == "__main__":unittest.main()
