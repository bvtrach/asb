"""Durable, exact-name Docker ownership and non-destructive quiescence.

This is resource recovery, never permission to retry or manufacture a grade.
No host-wide enumeration, pruning, PID signalling or volume deletion occurs.
"""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import uuid


class RecoveryError(RuntimeError):
    pass


def resource_name(kind, name, attempt):
    pattern = (r"[a-z0-9]+(?:[._-][a-z0-9]+)*(?:/[a-z0-9]+(?:[._-][a-z0-9]+)*)*:[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}"
               if kind == "images" else r"[A-Za-z0-9][A-Za-z0-9_.-]{0,254}")
    return (kind in ["containers", "networks", "volumes", "images"] and isinstance(name, str)
            and len(name) <= 255 and re.fullmatch(pattern, name) is not None and attempt in name)


def build_image_tag(attempt, service):
    return "asb-workload:" + attempt + "-" + hashlib.sha256(service.encode()).hexdigest()[:16]


def owned_resource(client, declaration, labels):
    kind, name = declaration["kind"], declaration["name"]
    try:
        resource = getattr(client, kind).get(name)
    except Exception as error:
        if getattr(error, "status_code", None) != 404:
            raise RecoveryError("could not inspect a declared Docker resource") from None
        return None
    actual = (resource.attrs.get("Config", {}).get("Labels") if kind in ["containers", "images"]
              else resource.attrs.get("Labels")) or {}
    if kind == "images":
        name_matches = (name in (resource.attrs.get("RepoTags") or [])
                        and isinstance(resource.id,str) and re.fullmatch(r"sha256:[0-9a-f]{64}",resource.id))
    else:
        name_matches = resource.name.lstrip("/") == name
    if not name_matches or any(actual.get(key) != value for key, value in labels.items()):
        raise RecoveryError("declared resource name now identifies a different owner")
    return resource


def private_file(directory, name, flags):
    descriptor = os.open(name, flags | os.O_NOFOLLOW | os.O_NONBLOCK, 0o600, dir_fd=directory)
    metadata = os.fstat(descriptor)
    if not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != os.getuid() or metadata.st_mode & 0o077:
        os.close(descriptor)
        raise RecoveryError("resource journal requires private regular files")
    return descriptor


def open_directory(path):
    descriptor = os.open(path, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    metadata = os.fstat(descriptor)
    if metadata.st_uid != os.getuid() or metadata.st_mode & 0o077:
        os.close(descriptor)
        raise RecoveryError("resource journal requires a private attempt directory")
    return descriptor


def publish(directory, name, value):
    temporary = ".resource-" + uuid.uuid4().hex
    descriptor = private_file(directory, temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL)
    try:
        with os.fdopen(descriptor, "w") as output:
            json.dump(value, output, sort_keys=True)
            output.write("\n")
            output.flush()
            os.fsync(output.fileno())
        os.rename(temporary, name, src_dir_fd=directory, dst_dir_fd=directory)
        os.fsync(directory)
    finally:
        try:
            os.unlink(temporary, dir_fd=directory)
        except FileNotFoundError:
            pass


def daemon_identity(client):
    identity = client.info().get("ID")
    if not isinstance(identity, str) or not 1 <= len(identity) <= 256:
        raise RecoveryError("Docker did not report a bounded daemon identity")
    return identity


def lock_directory(directory):
    descriptor = private_file(directory, "resources.lock", os.O_RDWR | os.O_CREAT)
    try:
        fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
        return descriptor
    except BaseException:
        os.close(descriptor)
        raise RecoveryError("attempt resources still have an active owner") from None


class ResourceJournal:
    def __init__(self, client, path, attempt):
        if not re.fullmatch(r"asb-[0-9a-f]{32}", attempt):
            raise RecoveryError("resource journal requires a unique attempt identity")
        self.directory = open_directory(path)
        self.lock = None
        try:
            self.lock = lock_directory(self.directory)
            # Never replace historical resource ownership with a fresh token.
            descriptor = private_file(self.directory, "resources.json", os.O_WRONLY | os.O_CREAT | os.O_EXCL)
            os.close(descriptor)
            self.record = {"schema_version": 1, "attempt": attempt,
                           "daemon": daemon_identity(client), "token": uuid.uuid4().hex,
                           "resources": []}
            publish(self.directory, "resources.json", self.record)
        except BaseException:
            self.close()
            raise

    @property
    def labels(self):
        return {"asb.owner": "asb", "asb.attempt": self.record["attempt"],
                "asb.recovery": self.record["token"]}

    def declare(self, kind, name):
        value = {"kind": kind, "name": name}
        if not resource_name(kind,name,self.record["attempt"]):
            raise RecoveryError("resource declaration is outside this attempt")
        if value not in self.record["resources"]:
            if len(self.record["resources"]) >= 64:
                raise RecoveryError("attempt resource declarations exceed their limit")
            self.record["resources"].append(value)
            publish(self.directory, "resources.json", self.record)
        return self.labels

    def compose(self, configuration):
        for name, service in configuration.get("services", {}).items():
            service.setdefault("labels", {}).update(self.declare("containers", service["container_name"]))
            build = service.get("build")
            if build:
                if isinstance(build,str):
                    build = service["build"] = {"context":build}
                if not isinstance(build,dict):
                    raise RecoveryError("task image build is not a Compose build definition")
                image = build_image_tag(self.record["attempt"],name)
                service["image"] = image
                # Avoid publishing upstream/shared tags from an owned build.
                build["tags"] = [image]
                build.setdefault("labels",{}).update(self.declare("images",image))
        for kind in ["networks", "volumes"]:
            for resource in configuration.get(kind, {}).values():
                if resource.get("external"):
                    raise RecoveryError("cannot journal an external task resource")
                resource.setdefault("labels", {}).update(self.declare(kind, resource["name"]))
        return configuration

    def cleanup_images(self, client):
        if self.lock is None:
            raise RecoveryError("image cleanup requires its active resource owner")
        if daemon_identity(client) != self.record["daemon"]:
            raise RecoveryError("image cleanup selected a different Docker daemon")
        declared = [item for item in self.record["resources"] if item["kind"] == "images"]
        # Inspect the complete set before removing any tag. No force/prune or
        # image-ID deletion: user/upstream tags and build caches stay untouched.
        owned = [(item,owned_resource(client,item,self.labels)) for item in declared]
        results = []
        for item,image in owned:
            result = dict(item,status="absent")
            if image is not None:
                client.images.remove(item["name"],noprune=True)
                result.update(status="removed",image_id=image.id)
            results.append(result)
        if results:
            publish(self.directory,"image-cleanup.json",{"schema_version":1,"attempt":self.record["attempt"],
                "scope":"owned-build-tags-only","resources":results})
        return results

    def close(self):
        if self.lock is not None:
            os.close(self.lock)
            self.lock = None
        if self.directory is not None:
            os.close(self.directory)
            self.directory = None


def quiesce(client, path):
    """Stop declared containers; retain their files, volumes and build images.

    An absent resource is an observation, not proof that a daemon-side pending
    create cannot finish later. This result alone never authorizes rescheduling.
    """
    directory, lock = open_directory(path), None
    try:
        lock = lock_directory(directory)
        descriptor = private_file(directory, "resources.json", os.O_RDONLY)
        with os.fdopen(descriptor, "rb") as source:
            content = source.read(1024 * 1024 + 1)
        if len(content) > 1024 * 1024:
            raise RecoveryError("resource journal exceeds its size limit")
        record = json.loads(content)
        if (not isinstance(record, dict) or set(record) != {"schema_version", "attempt", "daemon", "token", "resources"}
                or record["schema_version"] != 1
                or not isinstance(record["attempt"], str) or not re.fullmatch(r"asb-[0-9a-f]{32}", record["attempt"])
                or not isinstance(record["token"], str) or not re.fullmatch(r"[0-9a-f]{32}", record["token"])
                or record["daemon"] != daemon_identity(client)
                or not isinstance(record["resources"], list) or len(record["resources"]) > 64):
            raise RecoveryError("resource journal identity or selected Docker daemon does not match")
        labels = {"asb.owner":"asb", "asb.attempt":record["attempt"], "asb.recovery":record["token"]}
        owned, seen = [], set()
        # Validate the entire population before mutating any resource.
        for declaration in record["resources"]:
            if not isinstance(declaration, dict) or set(declaration) != {"kind", "name"}:
                raise RecoveryError("invalid resource journal declaration")
            kind, name = declaration["kind"], declaration["name"]
            if not resource_name(kind,name,record["attempt"]) or (kind,name) in seen:
                raise RecoveryError("invalid or duplicate attempt resource")
            seen.add((kind, name))
            resource = owned_resource(client,declaration,labels)
            owned.append((declaration, resource))
        outcomes = []
        for declaration, resource in owned:
            status = "absent"
            if resource is not None:
                status = "preserved"
                if declaration["kind"] == "containers":
                    resource.stop(timeout=2)
                    resource.reload()
                    if resource.attrs.get("State", {}).get("Running") is not False:
                        raise RecoveryError("owned container did not confirm its stopped state")
                    status = "stopped-preserved"
            outcome = {**declaration,"status":status}
            if declaration["kind"] == "images" and resource is not None:
                outcome["image_id"] = resource.id
            outcomes.append(outcome)
        result = {"schema_version":1, "attempt":record["attempt"],
                  "scope":"observed-resource-quiescence-not-a-retry-receipt", "resources":outcomes}
        # A no-op inspection of a normally cleaned-up attempt must not change
        # its sealed evaluator evidence merely by adding a new report file.
        if any(resource is not None for _, resource in owned):
            publish(directory, "resource-quiescence.json", result)
        return result
    finally:
        if lock is not None:
            os.close(lock)
        os.close(directory)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--quiesce", required=True, type=Path)
    arguments = parser.parse_args()
    from asb_docker import DockerSession
    try:
        with DockerSession() as client:
            print(json.dumps(quiesce(client, arguments.quiesce)))
    except Exception as error:
        # SDK errors may contain daemon URLs or response bodies.
        result = {"ok":False, "error_class":type(error).__name__}
        if isinstance(error, RecoveryError):
            result["reason"] = str(error)
        print(json.dumps(result))
        raise SystemExit(1)
