"""Actual native and guest A/B artifact isolation, entirely local fixtures."""
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time

ASB = Path(sys.argv[1]).resolve()
CRATE = Path(__file__).resolve().parents[1]
ARTIFACTS = CRATE / "target"
ARTIFACTS.mkdir(exist_ok=True)
ROOT = Path(tempfile.mkdtemp(prefix="agent-artifact-repro-", dir=ARTIFACTS))
PERMISSION_JOURNALS = []


def restore_fixture_permissions():
    # Keep evidence, but end the permission fault after all scenario assertions.
    # Only the exact directories recorded before our fake client's chmod qualify;
    # never recursively chmod a tree or follow a replacement symlink.
    journals = PERMISSION_JOURNALS[:]
    PERMISSION_JOURNALS.clear()
    failures = []
    for journal in journals:
        if not journal.exists():
            continue
        for line in journal.read_text().splitlines():
            try:
                entry = json.loads(line)
                path = Path(entry["path"])
                assert path.is_relative_to(journal.parent), "foreign permission journal path"
                descriptor = os.open(path, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
                try:
                    actual = os.fstat(descriptor)
                    assert (actual.st_dev, actual.st_ino) == (int(entry["dev"]), int(entry["ino"])), "permission target replaced"
                    os.fchmod(descriptor, entry["mode"])
                    assert os.fstat(descriptor).st_mode & 0o777 == entry["mode"]
                finally:
                    os.close(descriptor)
            except Exception as error:
                failures.append(str(error))
    assert not failures, ("fixture permission restoration failed", failures)


def project(name, scenario="normal"):
    root = ROOT / name
    (root / ".asb").mkdir(parents=True)
    PERMISSION_JOURNALS.append(root / "permission-faults.jsonl")
    (root / ".asb/project.toml").write_text(
        "tools_dir='tools'\nworkloads_dir='workloads'\nworkspace_dir='workspace'\n"
    )
    shutil.copytree(CRATE / "assets/workloads", root / "workloads")
    shutil.copytree(CRATE / "assets/workloads/fixtures", root / ".asb/fixtures")
    binary = root / "tools/mini-swe/bin/mini"
    binary.parent.mkdir(parents=True)
    node = shutil.which("node")
    assert node
    # The second invocation emits nothing. A seals the workspace against
    # unlink, exercising the existing ignored remove_file failure naturally.
    binary.write_text(f"#!{node}\n" + r"""
const fs=require('node:fs'), path=require('node:path');
const index=process.argv.indexOf('--output');
if (index<0 || !process.argv[index+1]) throw Error('missing private artifact output');
const destination=path.resolve(process.argv[index+1]);
const home=path.resolve(process.env.HOME);
if (!destination.startsWith(home+path.sep) || !destination.endsWith(path.sep+path.join('mini-swe','trajectory.json')))
 throw Error('artifact output outside fixture home');
const marker=path.join(process.env.HOME,'executions');
function permissionFault(directory,mode) {
 const before=fs.statSync(directory,{bigint:true});
 fs.appendFileSync(path.join(home,'permission-faults.jsonl'),JSON.stringify({
  path:directory,dev:String(before.dev),ino:String(before.ino),mode:Number(before.mode & 0o777n)
 })+'\n',{mode:0o600});
 fs.chmodSync(directory,mode);
}
const previous=fs.existsSync(marker)?fs.readFileSync(marker,'utf8'):'';
fs.appendFileSync(marker,'called\n');
if (!previous) {
 fs.mkdirSync(path.dirname(destination),{recursive:true});
 if (process.env.ASB_ARTIFACT_SCENARIO==='symlink') {
   const target=path.join(process.env.HOME,'foreign.json');
   fs.writeFileSync(target,'{"answer":"foreign bytes"}');fs.symlinkSync(target,destination);
 } else if (process.env.ASB_ARTIFACT_SCENARIO==='malformed') {
   fs.writeFileSync(destination,'{"partial":');
 } else if (process.env.ASB_ARTIFACT_SCENARIO==='oversized') {
   fs.writeFileSync(destination,'x'.repeat(16*1024*1024+1));
 } else fs.writeFileSync(destination,JSON.stringify({trajectory_format:'mini-swe-agent-1.1',
 info:{model_stats:{api_calls:4}},messages:[{role:'assistant',
 content:'src/cli.py src/registry.py lookup',
 extra:{response:{usage:{prompt_tokens:13,completion_tokens:21}}}}]}));
 permissionFault(process.cwd(),0o555);
 if (process.env.ASB_ARTIFACT_SCENARIO==='cleanup') permissionFault(path.dirname(destination),0o500);
}
if (process.env.ASB_ARTIFACT_NATIVE) {
 fetch(process.env.OPENAI_BASE_URL+'/chat/completions',{method:'POST',headers:{'content-type':'application/json'},
 body:JSON.stringify({model:'fixture',messages:[],tools:[{type:'function',function:{name:'fixture',parameters:{type:'object'}}}]})})
 .then(async response=>{if(!response.ok)throw Error('fixture replay failed');await response.text();})
 .catch(()=>{process.exitCode=9;});
}
""")
    binary.chmod(0o700)
    config = {
        "agent": "mini-swe", "provider": "openrouter", "model": "fixture",
        "workload": "repository-navigation", "workspace": str(root / "workspace"),
        "log_file": str(root / "run.jsonl"),
    }
    (root / "workspace").mkdir()
    (root / "workspace/.asb-mini-swe.traj.json").write_text("foreign legacy workspace file")
    return root, config


def environment(root, scenario="normal"):
    return {"HOME": str(root), "PATH": os.environ["PATH"], "LANG": "C.UTF-8",
        "ASB_ARTIFACT_SCENARIO":scenario}


def native(scenario="normal"):
    root, config = project("native-"+scenario)
    config_path = root / "benchmark.toml"
    config_path.write_text("".join(f"{key}={json.dumps(value)}\n" for key, value in config.items()))
    if scenario == "publication-loss":
        (root / "run.jsonl").mkdir()
    if scenario == "cleanup-logfail":
        (root / "run.jsonl").mkdir()
        with config_path.open("a") as stream:
            stream.write(f"[measurements]\nenabled=true\ndirectory={json.dumps(str(root / 'measurements'))}\ncollectors=[]\n")
    # The actual client contacts only ASB's loopback replay fixture.
    tape = root / "tape.jsonl"
    tape.write_text(json.dumps({"request":{"method":"POST","url":"/v1/chat/completions",
        "body":json.dumps({"model":"fixture","messages":[],"tools":[{"type":"function","function":{"name":"fixture","parameters":{"type":"object"}}}]})},"response":{"status":200,"headers":{},"body":"{}"}})+"\n")
    records = []
    for _ in range(2):
        if (root / "workspace").exists():
            (root / "workspace").chmod(0o755)
        env = environment(root,"cleanup" if scenario.startswith("cleanup") else scenario)
        env["ASB_ARTIFACT_NATIVE"] = "1"
        result = subprocess.run([str(ASB),"run",str(config_path),"--replay","--cassette",str(tape),"--json"],
            cwd=root,env=env,capture_output=True,timeout=30)
        (root / f"invocation-{len(records)}.stderr").write_bytes(result.stderr)
        (root / f"invocation-{len(records)}.stdout").write_bytes(result.stdout)
        if scenario == "publication-loss":
            assert result.returncode != 0, "blocked result publication must remain an error"
            record = json.loads(result.stdout)
            assert record["workload_success"] is True, record
            assert record["token_usage"]["total_tokens"] == 34, record
            assert record["steps"] == 4, record
            assert record["benchmark_outcome"]["cleanup_confirmed"] is True, record
            assert "log" in result.stderr.decode().lower(), result.stderr
            assert not (root / "measurements").exists(), "measurements are disabled"
            assert not list(root.glob("**/.asb-completions/*.json")), "no receipt after blocked log"
            assert (root / "executions").read_text().splitlines() == ["called"], "never retry the client"
            raw = list(root.glob("**/mini-swe/frozen-trajectory.json"))
            assert len(raw) == 1, "native publication failure needs a durable frozen raw handoff"
            frozen = json.loads(raw[0].read_text())
            assert frozen["info"]["model_stats"]["api_calls"] == 4
            message = frozen["messages"][0]
            assert message["content"] == "src/cli.py src/registry.py lookup"
            usage = message["extra"]["response"]["usage"]
            assert usage["prompt_tokens"] + usage["completion_tokens"] == 34
            assert raw[0].stat().st_mode & 0o777 == 0o600
            assert raw[0].parent.stat().st_mode & 0o777 == 0o700
            assert str(raw[0].relative_to(raw[0].parents[2])) in result.stderr.decode(), "locatable attempt identity/path required"
            assert not list(root.glob("**/mini-swe/trajectory.json")), "original scratch consumed only after handoff"
            return
        if scenario == "cleanup-logfail":
            assert result.returncode != 0
            record = json.loads((root / "measurements/run.json").read_text())
            assert record["workload_success"] == (len(records)==0)
            assert "log" in result.stderr.decode().lower(), result.stderr
            records.append(record)
        else:
            records.append(json.loads((root / "run.jsonl").read_text().splitlines()[-1]))
    assert records[0]["token_usage"]["total_tokens"] == 34, records[0]
    assert records[0]["steps"] == 4, records[0]
    assert records[0]["workload_success"] is True, records[0]
    assert records[1]["token_usage"]["total_tokens"] is None, (
        "native B inherited A usage", records[1]["token_usage"])
    assert records[1]["steps"] is None, "native B inherited A steps"
    assert records[1]["workload_success"] is False, "native B inherited A answer"
    if scenario.startswith("cleanup"):
        assert "scratch settlement failed" in records[0]["failure_reason"], records[0]
        assert records[0]["benchmark_outcome"]["cleanup_confirmed"] is True
        assert records[0]["workload_success"] is True
        assert list(root.glob("**/mini-swe/trajectory.json")), "retain failed cleanup evidence"
    if scenario == "cleanup-logfail":
        assert not list(root.glob("**/.asb-completions/*.json")), "failed log publication grants no outcome receipt"
    assert (root / "executions").read_text().splitlines() == ["called","called"]
    assert (root / "workspace/.asb-mini-swe.traj.json").read_text() == "foreign legacy workspace file"


def guest(scenario="normal"):
    root, config = project("guest-"+scenario)
    records = []
    for index in range(2):
        (root / "workspace").chmod(0o755)
        run = root / "private" / f"attempt-{index}"
        request = {"config":config,"connection":{"interface":"open_ai_chat",
            "authentication":"bearer","base_url":"http://127.0.0.1:1/v1",
            "credential":"fixture"},"prompt":"offline task","project_root":str(root),
            "runtime_dir":str(root / "private"),"run_dir":str(run)}
        if scenario == "publication" and index == 0:
            (run / "agent-result.json").mkdir(parents=True)
        env = environment(root,scenario)
        env["ASB_GUEST_AGENT_REQUEST"] = json.dumps(request)
        child = subprocess.Popen([str(ASB),"__asb-agent"],cwd=root,env=env,
            stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        try:
            ready = child.stdout.readline()
            assert ready.strip() == f"__ASB_GATE__:{child.pid}".encode(), ready
            deadline = time.monotonic()+5
            while "\nState:\tT" not in Path(f"/proc/{child.pid}/status").read_text():
                assert time.monotonic() < deadline, "guest never stopped"
                time.sleep(.005)
            child.send_signal(signal.SIGCONT)
            _, stderr = child.communicate(timeout=10)
            (root / f"invocation-{index}.stderr").write_bytes(stderr)
            if scenario == "publication" and index == 0:
                assert child.returncode != 0, "publication blocker must fail"
                raw = json.loads((run / "mini-swe/trajectory.json").read_text())
                assert raw["info"]["model_stats"]["api_calls"] == 4, "failed publication must retain raw evidence"
                return
            assert child.returncode == 0, stderr
            records.append(json.loads((run / "agent-result.json").read_text()))
        finally:
            if child.poll() is None:
                child.kill()
                child.wait(timeout=5)
    if scenario in ["symlink","malformed","oversized"]:
        assert records[0]["artifact_error"], records[0]
        assert records[0]["tree_quiesced"] is True
        assert records[0]["final_response"] is None
        assert records[0]["token_usage"]["total_tokens"] is None
        if scenario == "symlink":
            assert (root / "foreign.json").read_text() == '{"answer":"foreign bytes"}'
            assert (root / "private/attempt-0/mini-swe/trajectory.json").is_symlink()
        return
    assert records[0]["final_response"] == "src/cli.py src/registry.py lookup", records[0]
    assert records[0]["token_usage"]["total_tokens"] == 34, records[0]
    assert records[0]["steps"] == 4, records[0]
    assert records[1]["final_response"] is None, ("guest B inherited A answer", records[1])
    assert records[1]["token_usage"]["total_tokens"] is None, "guest B inherited A usage"
    assert records[1]["steps"] is None, "guest B inherited A steps"
    if scenario == "cleanup":
        assert "scratch settlement failed" in records[0]["artifact_error"]
        assert records[0]["tree_quiesced"] is True
        assert (root / "private/attempt-0/mini-swe/trajectory.json").is_file()
    assert (root / "executions").read_text().splitlines() == ["called","called"]
    assert (root / "workspace/.asb-mini-swe.traj.json").read_text() == "foreign legacy workspace file"


failures = []
cases = [("native",native),("guest",guest)]
if "--native-publication-repro" in sys.argv:
    cases = [("native-publication-loss",lambda:native("publication-loss"))]
if "--extended" in sys.argv:
    cases += [("native-"+kind,lambda kind=kind:native(kind)) for kind in ["cleanup","cleanup-logfail","publication-loss"]]
    cases += [("guest-"+kind,lambda kind=kind:guest(kind)) for kind in ["cleanup","publication","symlink","malformed","oversized"]]
for name, operation in cases:
    try:
        try:
            operation()
        finally:
            restore_fixture_permissions()
        print(f"{name}: PASS",flush=True)
    except Exception as error:
        failures.append(name)
        print(f"{name}: FAIL {error}",flush=True)
print(f"Retained fixture evidence: {ROOT}",flush=True)
sys.exit(bool(failures))
