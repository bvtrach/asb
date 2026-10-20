"""ASB's pinned SWE-bench materializer; no agent/provider-specific behavior.

Evaluator-only rows stay outside agent workspaces. The public catalog contains
task identity and short descriptions, never the gold patch or test program.
The official SDK is imported only for operations that need it, so protocol
tests can run entirely offline using Python's standard library.
"""

import argparse
import hashlib
import importlib.metadata
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import tempfile
import uuid


class CatalogError(ValueError):
    pass


MAX_METADATA_BYTES = 4 * 1024 * 1024
MAX_CATALOG_BYTES = 16 * 1024 * 1024
MAX_PATCH_BYTES = 4 * 1024 * 1024
HARNESS_REVISION = "02e7a74ffd0b707aab73d203fe87bdc7c76afc8e"


def sdk_identity(distribution=None):
    """Record actual installed code identity rather than claiming the recipe pin."""
    distribution = distribution or importlib.metadata.distribution("swebench")
    try:
        direct = json.loads(distribution.read_text("direct_url.json") or "{}")
        revision = direct.get("vcs_info", {}).get("commit_id")
    except (TypeError, ValueError):
        revision = None
    if revision != HARNESS_REVISION:
        raise CatalogError("installed SWE-bench SDK does not match its pinned source revision; run `asb tool install swebench`")
    return {"revision": revision, "version": distribution.version}


def read_limited(path, limit):
    with Path(path).open("rb") as source:
        content = source.read(limit + 1)
    if len(content) > limit:
        raise CatalogError("installed task metadata exceeds its bounded read limit")
    return content


def encoded(value):
    return (json.dumps(value, ensure_ascii=False, sort_keys=True) + "\n").encode()


def atomic_write(path, content, mode=0o600):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix=".asb-write-", dir=path.parent)
    try:
        os.fchmod(descriptor, mode)
        with os.fdopen(descriptor, "wb") as output:
            output.write(content)
            output.flush()
            os.fsync(output.fileno())
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def build_catalog(rows, workload, source, revision, split, destination, validate=None):
    """Validate all rows before publishing a catalog; preserve previous revisions."""
    if not re.fullmatch(r"[0-9a-fA-F]{40,64}", revision):
        raise CatalogError("dataset revision must be a resolved commit, not a floating name")
    destination = Path(destination)
    private = destination / "evaluation-data" / revision
    if private.parent.is_symlink() or private.is_symlink():
        raise CatalogError("private evaluator data directory must not be a symlink")
    private.mkdir(parents=True, exist_ok=True, mode=0o700)
    os.chmod(private.parent, 0o700)
    os.chmod(private, 0o700)
    entries, seen = [], set()
    for row in rows:
        task_id = row.get("instance_id")
        if not isinstance(task_id, str) or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,255}", task_id):
            raise CatalogError("dataset row has no valid bounded instance_id")
        if task_id in seen:
            raise CatalogError(f"duplicate dataset instance: {task_id}")
        seen.add(task_id)
        for field in ("repo", "problem_statement", "base_commit"):
            if not isinstance(row.get(field), str) or not row[field].strip():
                raise CatalogError(f"instance {task_id} has no nonempty {field}")
        if not re.fullmatch(r"[0-9a-fA-F]{40}", row["base_commit"]):
            raise CatalogError(f"instance {task_id} has an invalid base_commit")
        agent_instruction(row)
        if validate is not None:
            try:
                validate(row)
            except (KeyError, TypeError, ValueError) as error:
                detail = type(error).__name__
                if isinstance(error, KeyError) and error.args and re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]{0,63}", str(error.args[0])):
                    detail += ": missing field " + str(error.args[0])
                raise CatalogError(f"instance {task_id} is incompatible with the official evaluator ({detail})") from error
        payload = encoded(row)
        if len(payload) > MAX_METADATA_BYTES:
            raise CatalogError(f"instance {task_id} exceeds the 4 MiB metadata limit")
        filename = hashlib.sha256(task_id.encode()).hexdigest() + ".json"
        metadata = private / filename
        if metadata.is_symlink():
            raise CatalogError(f"pinned metadata for {task_id} must not be a symlink")
        if metadata.exists() and read_limited(metadata, MAX_METADATA_BYTES) != payload:
            raise CatalogError(f"pinned metadata changed for instance {task_id}; retained old evidence")
        if not metadata.exists():
            atomic_write(metadata, payload)
        description = row["problem_statement"].strip().splitlines()[0][:180]
        entries.append({
            "id": task_id,
            "description": description,
            "repo": row["repo"],
            "base_commit": row["base_commit"],
            "metadata": str(metadata.relative_to(destination)),
            "metadata_sha256": hashlib.sha256(payload).hexdigest(),
        })
        if len(entries) > 10000:
            raise CatalogError("task catalog exceeds the 10000-instance safety limit")
    if not entries:
        raise CatalogError("selected dataset split contains no tasks")
    entries.sort(key=lambda entry: entry["id"])
    catalog = {
        "schema_version": 1,
        "workload": workload,
        "dataset": {"source": source, "revision": revision, "split": split},
        "tasks": entries,
    }
    content = encoded(catalog)
    immutable = destination / f"catalog-{revision}-{split}.json"
    if len(content) > MAX_CATALOG_BYTES:
        raise CatalogError("task catalog exceeds the 16 MiB limit")
    if immutable.exists() and read_limited(immutable, MAX_CATALOG_BYTES) != content:
        raise CatalogError("pinned catalog changed; retained the previous catalog")
    if not immutable.exists():
        atomic_write(immutable, content)
    atomic_write(destination / "catalog.json", content)
    return catalog


def read_task(catalog_path, task_id):
    """Bind private evaluator metadata to one installed catalog instance."""
    catalog_path = Path(catalog_path)
    catalog = json.loads(read_limited(catalog_path, MAX_CATALOG_BYTES))
    if catalog.get("schema_version") != 1:
        raise CatalogError("unsupported task catalog schema")
    selected = [entry for entry in catalog["tasks"] if entry["id"] == task_id]
    if len(selected) != 1:
        raise CatalogError("task must occur exactly once in the installed catalog")
    entry = selected[0]
    return catalog, read_metadata(catalog_path, task_id, entry)


def read_metadata(catalog_path, task_id, entry):
    catalog_path = Path(catalog_path)
    relative = Path(entry["metadata"])
    root = catalog_path.parent.resolve()
    path = root / relative
    if relative.is_absolute() or ".." in relative.parts or path.is_symlink() or not path.resolve().is_relative_to(root):
        raise CatalogError("task metadata must stay inside its installed catalog")
    payload = read_limited(path, MAX_METADATA_BYTES)
    if hashlib.sha256(payload).hexdigest() != entry["metadata_sha256"]:
        raise CatalogError("task metadata checksum changed; reinstall the pinned dataset")
    row = json.loads(payload)
    if (row["instance_id"], row["repo"], row["base_commit"]) != (task_id, entry["repo"], entry["base_commit"]):
        raise CatalogError("task metadata does not match its catalog identity")
    return row


def verify_catalog(catalog_path):
    catalog = json.loads(read_limited(catalog_path, MAX_CATALOG_BYTES))
    tasks = catalog.get("tasks")
    if not isinstance(tasks, list) or not tasks or len(tasks) > 10000:
        raise CatalogError("installed catalog has no valid task population")
    ids = [task["id"] for task in tasks]
    if len(ids) != len(set(ids)):
        raise CatalogError("installed catalog contains duplicate tasks")
    for entry in tasks:
        read_metadata(catalog_path, entry["id"], entry)
    return {"status": "verified", "task_count": len(ids)}


def agent_instruction(row):
    """Explicit allowlist: never send the gold patch or hidden tests to agents."""
    prompt = (
        f"Resolve this issue in repository {row['repo']} at base commit {row['base_commit']}.\n"
        "Make the necessary changes in the task repository and run relevant tests.\n\n"
        + row["problem_statement"]
    )
    if len(prompt.encode()) > 64 * 1024:
        raise CatalogError("task instruction exceeds the 64 KiB agent input limit")
    return prompt


def run_harness(arguments, directory, log, timeout):
    """One isolated evaluator process; timeout terminates its own process group."""
    environment = os.environ.copy()
    environment["PYTHONDONTWRITEBYTECODE"] = "1"
    environment["LITELLM_LOCAL_MODEL_COST_MAP"] = "True"
    with subprocess.Popen(arguments, cwd=directory, env=environment, stdin=subprocess.DEVNULL,
                          stdout=log, stderr=log, start_new_session=True) as process:
        try:
            return process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            # Give the controller's finally block time to remove only this
            # attempt's container before terminating a non-responsive helper.
            try:
                os.killpg(process.pid, signal.SIGTERM)
                process.wait(timeout=20)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
            except ProcessLookupError:
                process.wait()
            raise


def evaluate_submission(catalog_path, task_id, patch_path, artifacts, timeout, executor=run_harness):
    """Grade a frozen submission using upstream, never agent exit or source heuristics.

    Every invocation gets a new run ID and a new directory. The official SDK's
    cached result for an earlier patch can therefore never satisfy this attempt.
    The executor seam is only for provider-free protocol tests.
    """
    catalog, row = read_task(catalog_path, task_id)
    if not isinstance(timeout, int) or not 1 <= timeout <= 86400:
        raise CatalogError("evaluation timeout must be between 1 and 86400 seconds")
    patch = read_limited(patch_path, MAX_PATCH_BYTES)
    try:
        patch_text = patch.decode("utf-8")
    except UnicodeDecodeError as error:
        raise CatalogError("submission patch must be UTF-8") from error
    artifacts = Path(artifacts).absolute()
    artifacts.mkdir(mode=0o700, parents=True, exist_ok=False)
    run_id = "asb-" + uuid.uuid4().hex
    result = {
        "schema_version": 1, "workload": catalog["workload"], "task_id": task_id,
        "dataset": catalog["dataset"], "run_id": run_id,
        "patch_sha256": hashlib.sha256(patch).hexdigest(),
        "harness_revision": HARNESS_REVISION,
        "resolved": False, "outcome": "evaluation_error", "reason": None,
    }
    atomic_write(artifacts / "submission.jsonl", encoded({
        "instance_id": task_id, "model_name_or_path": "asb",
        "model_patch": patch_text,
    }))
    # The evaluator reads this exact frozen row, not a floating upstream dataset.
    atomic_write(artifacts / "dataset.json", encoded([row]))
    if not patch_text.strip():
        result.update(outcome="unresolved", reason="empty_submission")
    else:
        arguments = [sys.executable, str(Path(__file__).absolute()), "harness",
                     "--dataset_name", str(artifacts / "dataset.json"), "--split", catalog["dataset"]["split"],
                     "--instance_ids", task_id, "--predictions_path", str(artifacts / "submission.jsonl"),
                     "--max_workers", "1", "--open_file_limit", "4096",
                     "--timeout", str(timeout), "--run_id", run_id]
        with (artifacts / "harness.log").open("xb") as log:
            try:
                status = executor(arguments, artifacts, log, timeout + 120)
                result["harness_exit_code"] = status
                report_dir = artifacts / "logs/evaluation" / run_id
                summary_path = report_dir / "results.json"
                instance_path = report_dir / "asb" / task_id / "report.json"
                if not summary_path.is_file() or not instance_path.is_file():
                    result["reason"] = "official_report_missing"
                else:
                    summary = json.loads(read_limited(summary_path, MAX_METADATA_BYTES))
                    report = json.loads(read_limited(instance_path, MAX_METADATA_BYTES))
                    if (summary.get("total_instances") != 1
                            or summary.get("submitted_ids") != [task_id]
                            or summary.get("completed_ids") != [task_id]
                            or set(report) != {task_id}
                            or not isinstance(report[task_id].get("resolved"), bool)
                            or summary.get("resolved_ids") != ([task_id] if report[task_id]["resolved"] else [])):
                        raise CatalogError("official report does not match the submitted task")
                    resolved = report[task_id]["resolved"]
                    if resolved and report[task_id].get("patch_successfully_applied") is not True:
                        raise CatalogError("official report claims resolution without an applied patch")
                    image = json.loads(read_limited(artifacts / "image-identity.json", MAX_METADATA_BYTES))
                    sdk = json.loads(read_limited(artifacts / "sdk-identity.json", MAX_METADATA_BYTES))
                    if (image.get("declared_image") != row.get("image")
                            or not re.fullmatch(r"sha256:[0-9a-f]{64}", image.get("image_id", ""))
                            or sdk.get("revision") != HARNESS_REVISION):
                        raise CatalogError("official evaluator image or SDK identity does not match the attempt")
                    result.update(resolved=resolved, outcome="resolved" if resolved else "unresolved",
                                  reason=None if resolved else "official_tests_not_resolved",
                                  image=image, sdk=sdk,
                                  official_summary=str(summary_path.relative_to(artifacts)),
                                  official_report=str(instance_path.relative_to(artifacts)))
            except subprocess.TimeoutExpired:
                result.update(outcome="evaluation_timeout", reason="official_harness_deadline_exceeded")
            except (CatalogError, KeyError, TypeError, ValueError, OSError):
                result.update(outcome="evaluation_error", reason="invalid_official_report")
    atomic_write(artifacts / "evaluation.json", encoded(result))
    return result


def install_catalog(arguments):
    sdk_identity()
    from huggingface_hub import HfApi
    from datasets import load_dataset
    from swebench.harness.utils import make_test_spec

    # Public benchmark data needs no token, even if the enclosing user has one.
    info = HfApi(token=False).dataset_info(
        repo_id=arguments.dataset, revision=arguments.revision, timeout=30
    )
    rows = load_dataset(
        arguments.dataset,
        revision=info.sha,
        split=arguments.split,
        token=False,
        cache_dir=str(arguments.cache),
    )
    catalog = build_catalog(
        rows, arguments.workload, arguments.dataset, info.sha, arguments.split,
        arguments.output, validate=make_test_spec,
    )
    return {
        "status": "installed", "dataset": catalog["dataset"],
        "task_count": len(catalog["tasks"]), "manifest": "catalog.json",
    }


def official_harness(arguments):
    """Run the pinned SDK with scoped Docker transport and no root Python."""
    from asb_docker import DockerSession
    from swebench.harness import run_evaluation
    import docker.errors
    identity = sdk_identity()
    atomic_write(Path.cwd() / "sdk-identity.json", encoded(identity))
    if (not re.fullmatch(r"asb-[0-9a-f]{32}", arguments.run_id)
            or len(arguments.instance_ids) != 1
            or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,255}", arguments.instance_ids[0])):
        raise CatalogError("official evaluation needs one bounded task and a fresh ASB run identity")
    def interrupted(_signal, _frame):
        raise InterruptedError("ASB evaluation was cancelled")
    signal.signal(signal.SIGTERM, interrupted)
    with DockerSession() as client:
        owned_name = f"sweb.eval.{arguments.instance_ids[0].lower()}.{arguments.run_id}"
        original_create = run_evaluation.create_container
        def create_container(test_spec, docker_client, run_id, logger):
            container = original_create(test_spec, docker_client, run_id, logger)
            try:
                image = container.image
                atomic_write(Path.cwd() / "image-identity.json", encoded({
                    "declared_image": test_spec.image,
                    "image_id": image.id,
                    "repository_digests": image.attrs.get("RepoDigests", []),
                    "architecture": image.attrs.get("Architecture"),
                    "os": image.attrs.get("Os"),
                }))
                return container
            except BaseException:
                # Creation succeeded but provenance capture failed; do not
                # leak that newly owned container or accept an unbound score.
                container.remove(force=True)
                raise
        def cleanup_container(_client, container, logger):
            if container is None:
                return
            if container.name.lstrip("/") != owned_name:
                raise CatalogError("refused cleanup of a container outside the current attempt")
            try:
                container.remove(force=True)
                logger.info("Removed ASB attempt container through scoped Docker transport")
            except docker.errors.NotFound:
                pass
        try:
            run_evaluation._docker_client = lambda: client
            run_evaluation.create_container = create_container
            # This SDK revision shells out to an unprivileged Docker CLI for
            # cleanup. Route its equivalent operation through the same scoped
            # client as creation, avoiding permission failures and leftovers.
            run_evaluation.cleanup_container = cleanup_container
            run_evaluation.main(
                dataset_name=str(arguments.dataset_name), split=arguments.split,
                instance_ids=arguments.instance_ids, predictions_path=str(arguments.predictions_path),
                max_workers=arguments.max_workers, open_file_limit=arguments.open_file_limit,
                run_id=arguments.run_id, timeout=arguments.timeout, rewrite_reports=False, modal=False,
            )
        finally:
            # Upstream cleans normal outcomes. This also covers controller
            # exceptions and graceful cancellation; never enumerate/remove
            # unrelated containers or prune host images.
            name = owned_name
            cleanup = {"container": name, "status": "absent"}
            try:
                container = client.containers.get(name)
                container.remove(force=True)
                cleanup["status"] = "removed"
            except docker.errors.NotFound:
                pass
            except Exception as error:
                cleanup.update(status="error", error_class=type(error).__name__)
            atomic_write(Path.cwd() / "cleanup.json", encoded(cleanup))
    return {"status": "evaluated", "run_id": arguments.run_id}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="operation", required=True)
    catalog = commands.add_parser("catalog")
    catalog.add_argument("--workload", required=True)
    catalog.add_argument("--dataset", required=True)
    catalog.add_argument("--revision", default="main")
    catalog.add_argument("--split", default="test")
    catalog.add_argument("--output", type=Path, required=True)
    catalog.add_argument("--cache", type=Path, required=True)
    commands.add_parser("identity")
    verify = commands.add_parser("verify")
    verify.add_argument("--catalog", type=Path, required=True)
    evaluate = commands.add_parser("evaluate")
    evaluate.add_argument("--catalog", type=Path, required=True)
    evaluate.add_argument("--task", required=True)
    evaluate.add_argument("--patch", type=Path, required=True)
    evaluate.add_argument("--artifacts", type=Path, required=True)
    evaluate.add_argument("--timeout", type=int, default=1800)
    harness = commands.add_parser("harness")
    harness.add_argument("--dataset_name", type=Path, required=True)
    harness.add_argument("--split", required=True)
    harness.add_argument("--instance_ids", nargs="+", required=True)
    harness.add_argument("--predictions_path", type=Path, required=True)
    harness.add_argument("--max_workers", type=int, required=True)
    harness.add_argument("--open_file_limit", type=int, required=True)
    harness.add_argument("--run_id", required=True)
    harness.add_argument("--timeout", type=int, required=True)
    arguments = parser.parse_args(argv)
    try:
        if arguments.operation == "catalog":
            result = install_catalog(arguments)
        elif arguments.operation == "identity":
            result = {"status": "ready", "sdk": sdk_identity()}
        elif arguments.operation == "verify":
            result = verify_catalog(arguments.catalog)
        elif arguments.operation == "evaluate":
            result = evaluate_submission(arguments.catalog, arguments.task, arguments.patch,
                                         arguments.artifacts, arguments.timeout)
        else:
            result = official_harness(arguments)
    except Exception as error:
        # Do not echo arbitrary upstream response bodies or credential-bearing
        # URLs. Detailed library diagnostics stay in the installer's private log.
        result = {"status": "error", "error_class": type(error).__name__}
        if isinstance(error, CatalogError):
            result["reason"] = str(error)
        if isinstance(error, ImportError):
            result["missing_module"] = getattr(error, "name", None)
        response = getattr(error, "response", None)
        if response is not None:
            result["http_status"] = getattr(response, "status_code", None)
        print(json.dumps(result))
        return 1
    print(json.dumps(result))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
