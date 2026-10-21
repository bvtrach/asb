"""Bounded canonical material identities, never archive extraction or credentials.

The tar walker validates extension lengths before reading them. It deliberately
does not use tarfile.next(), which can allocate an advertised PAX/longname body
before the caller sees its entry. Callers own Docker coverage and transport
timeouts; a tree digest is not a claim about undeclared mounts or services.
"""
import hashlib
import json
import time


class MaterialError(RuntimeError):
    pass


RUNTIME_ROOTS = ("/proc", "/sys", "/dev")
RUNTIME_FILES = ("/etc/hosts", "/etc/hostname", "/etc/resolv.conf")


def change_inventory(changes):
    """Stable writable-layer declarations, including explicit deletion facts."""
    if not isinstance(changes, list) or len(changes) > 100000:
        raise MaterialError("prepared material changes exceed coverage bounds")
    result, remaining = {}, 16 * 1024**2
    for change in changes:
        path = change.get("Path") if isinstance(change, dict) else None
        kind = change.get("Kind") if isinstance(change, dict) else None
        if (not isinstance(path, str) or not path.startswith("/")
                or type(kind) is not int or kind not in [0, 1, 2]):
            raise MaterialError("prepared material returned invalid change coverage")
        normalized = "/" + path_name(path[1:])
        if normalized != path or path in result:
            raise MaterialError("prepared material changes have ambiguous paths")
        remaining -= len(path.encode())
        if remaining < 0:
            raise MaterialError("prepared material change names exceed bounds")
        result[path] = kind
    return [(path, kind) for path, kind in sorted(result.items())
        if path not in RUNTIME_FILES and not any(path == root or path.startswith(root + "/")
                                               for root in RUNTIME_ROOTS)]


def canonical_digest(value, domain):
    remaining = [1024 * 1024, 10000]

    def check(item, depth=0):
        remaining[1] -= 1
        if depth > 16 or remaining[1] < 0:
            raise MaterialError("material metadata exceeds structural bounds")
        if isinstance(item, str):
            remaining[0] -= len(item.encode("utf-8"))
        elif isinstance(item, list):
            for value in item:
                check(value, depth + 1)
        elif isinstance(item, dict):
            for key, value in item.items():
                if not isinstance(key, str):
                    raise MaterialError("material metadata has invalid keys")
                check(key, depth + 1)
                check(value, depth + 1)
        elif item is not None and not isinstance(item, (bool, int)):
            raise MaterialError("material metadata has unsupported values")
        elif isinstance(item, int) and item.bit_length() > 64:
            raise MaterialError("material metadata integer exceeds bounds")
        if remaining[0] < 0:
            raise MaterialError("material metadata exceeds byte bound")

    check(value)
    return hashlib.sha256(domain.encode() + b"\0" + json.dumps(
        value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()).hexdigest()


def image_digest(attributes, configuration):
    """Image filesystem and declared runtime; not the writable prepared tree."""
    layers = attributes.get("RootFS", {}).get("Layers")
    if (not isinstance(layers, list) or not 1 <= len(layers) <= 4096
            or any(not isinstance(layer, str) or len(layer) != 71
                   or not layer.startswith("sha256:")
                   or any(char not in "0123456789abcdef" for char in layer[7:]) for layer in layers)
            or attributes.get("Os") != "linux"
            or attributes.get("Architecture") not in ["amd64", "arm64"]):
        raise MaterialError("image does not establish bounded filesystem material")
    runtime = {key: configuration.get(key) for key in [
        "Env", "User", "WorkingDir", "Entrypoint", "Cmd", "Shell", "Volumes",
        "ExposedPorts", "StopSignal", "Healthcheck", "OnBuild"]}
    return canonical_digest({"layers": layers, "os": attributes["Os"],
        "architecture": attributes["Architecture"], "variant": attributes.get("Variant"),
        "runtime": runtime}, "asb-image-material-v1")


def capture_task(task, definition, exclusions=(), deadline=None, binary=None):
    """Verify the complete declared service set before the single-root path."""
    from asb_resources import RecoveryError, daemon_identity, owned_resource
    deadline = time.monotonic() + 420 if deadline is None else deadline
    journal = getattr(task, "journal", None)
    if journal is None or journal.lock is None:
        raise MaterialError("prepared material requires an active resource declaration")
    declarations = journal.record.get("resources")
    if not isinstance(declarations, list) or len(declarations) > 64:
        raise MaterialError("prepared material resource declarations exceed bounds")
    containers = [item for item in declarations if item.get("kind") == "containers"]
    if len(containers) != 1:
        raise MaterialError("multiple declared services require explicit prepared material coverage")
    if time.monotonic() >= deadline or (task.cancelled is not None and task.cancelled.is_set()):
        raise MaterialError("material capture was cancelled")
    try:
        if daemon_identity(task.client) != journal.record.get("daemon"):
            raise MaterialError("prepared material selected a different resource daemon")
        resource = owned_resource(task.client, containers[0], journal.labels)
    except RecoveryError:
        raise MaterialError("prepared material resource ownership could not be confirmed") from None
    if resource is None or resource.id != task.workspace.container.id:
        raise MaterialError("prepared material resource differs from the task container")
    if binary is not None:
        from asb_material_root import capture
        return capture(task, definition, exclusions, binary, deadline)
    return capture_workdir(task.workspace, definition, exclusions, task.cancelled, deadline)


def capture_workdir(workspace, definition, exclusions=(), cancelled=None, deadline=None):
    """One adapter-declared non-root workspace, with no undeclared mounts.

    The owning SDK transport has a bounded 120-second read timeout. This capture
    additionally observes a 420-second deadline/cancellation between reads. Root
    filesystems, extra services and mounted inputs need explicit coverage rather
    than silently being treated as equivalent to this narrower declaration.
    """
    deadline = time.monotonic() + 420 if deadline is None else deadline
    def check():
        if time.monotonic() >= deadline or (cancelled is not None and cancelled.is_set()):
            raise MaterialError("material capture was cancelled or exceeded its deadline")
    check()
    workdir = workspace.workdir.rstrip("/")
    if not workdir or not workdir.startswith("/") or ".." in workdir.split("/"):
        raise MaterialError("root or invalid prepared material requires an explicit coverage adapter")
    container = workspace.container
    if container is None:
        raise MaterialError("prepared material container is not available")
    container.reload()
    check()
    if not isinstance(container.attrs.get("Mounts"), list) or container.attrs["Mounts"]:
        raise MaterialError("mounted prepared material requires explicit coverage")
    changes = change_inventory(container.diff())
    check()
    for path, _kind in changes:
        covered = path == workdir or path.startswith(workdir + "/")
        if not covered:
            raise MaterialError("prepared material changed outside declared roots; explicit coverage is required")
    image = container.image
    image.reload()
    check()
    image_sha256 = image_digest(image.attrs, container.attrs.get("Config", {}))
    check()
    chunks, metadata = container.get_archive(workdir, chunk_size=65536, encode_stream=False)
    try:
        prefix = metadata.get("name")
        if not isinstance(prefix, str) or prefix != workdir.rsplit("/", 1)[-1]:
            raise MaterialError("prepared material archive root differs from declaration")
        files = tar_digest(chunks, prefix, exclusions, cancelled=cancelled, deadline=deadline)
    finally:
        close = getattr(chunks, "close", None)
        if close is not None:
            close()
    sha256 = canonical_digest({"scope":"single-container-workdir-and-image-v1",
        "roots":[{"logical":"workspace", "exclusions":sorted(set(exclusions))}],
        "external_runtime_exclusions":list(RUNTIME_ROOTS + RUNTIME_FILES),
        "definition":definition,"image_sha256":image_sha256,"files_sha256":files},
        "asb-prepared-workload-material-v1")
    check()
    return {"schema_version":1,"scope":"prepared-workload-material-v1","sha256":sha256}


class Reader:
    def __init__(self, chunks, maximum, deadline, cancelled):
        self.chunks, self.maximum = iter(chunks), maximum
        self.deadline, self.cancelled = deadline, cancelled
        self.pending, self.offset = b"", 0

    def check(self):
        if time.monotonic() >= self.deadline or (self.cancelled is not None and self.cancelled.is_set()):
            raise MaterialError("material capture was cancelled or exceeded its deadline")

    def read(self, size):
        if not 0 <= size <= 65536:
            raise MaterialError("material read exceeds bounded chunk size")
        output = bytearray()
        while len(output) < size:
            self.check()
            if self.offset == len(self.pending):
                try:
                    chunk = next(self.chunks)
                except StopIteration:
                    raise MaterialError("material archive is truncated") from None
                self.check()
                if not isinstance(chunk, bytes) or not 0 < len(chunk) <= 1024 * 1024:
                    raise MaterialError("material transport returned an invalid chunk")
                self.maximum -= len(chunk)
                if self.maximum < 0:
                    raise MaterialError("material archive exceeds wire bound")
                self.pending, self.offset = chunk, 0
            count = min(size - len(output), len(self.pending) - self.offset)
            output.extend(self.pending[self.offset:self.offset + count])
            self.offset += count
        return bytes(output)

    def finish(self):
        self.check()
        if any(self.pending[self.offset:]):
            raise MaterialError("material archive has unexpected trailing bytes")
        while True:
            self.check()
            try:
                chunk = next(self.chunks)
            except StopIteration:
                self.check()
                break
            self.check()
            if not isinstance(chunk, bytes) or len(chunk) > 1024 * 1024:
                raise MaterialError("material transport returned an invalid chunk")
            self.maximum -= len(chunk)
            if self.maximum < 0 or any(chunk):
                raise MaterialError("material archive has excessive or invalid trailing bytes")


def number(field):
    if field and field[0] & 128:
        if field[0] & 64:
            raise MaterialError("negative archive field is unsupported")
        return int.from_bytes(bytes([field[0] & 127]) + field[1:], "big")
    field = field.strip(b"\0 ") or b"0"
    if any(char not in b"01234567" for char in field):
        raise MaterialError("material archive has an invalid numeric field")
    return int(field, 8)


def text(field):
    try:
        return field.split(b"\0", 1)[0].decode("utf-8")
    except UnicodeDecodeError:
        raise MaterialError("material archive names must be UTF-8") from None


def path_name(name):
    if (not isinstance(name, str) or len(name.encode()) > 4096 or name.startswith("/")
            or any(ord(char) < 32 or ord(char) == 127 for char in name)):
        raise MaterialError("material archive has an invalid path")
    parts = [part for part in name.split("/") if part not in ["", "."]]
    if len(parts) > 128 or ".." in parts:
        raise MaterialError("material archive path exceeds depth or escapes its root")
    return "/".join(parts)


def pax_fields(payload):
    result, position = {}, 0
    while position < len(payload):
        space = payload.find(b" ", position, position + 16)
        if space < 0 or not payload[position:space].isdigit():
            raise MaterialError("invalid material PAX record length")
        size = int(payload[position:space])
        end = position + size
        if end > len(payload) or size <= space - position + 2 or payload[end - 1:end] != b"\n":
            raise MaterialError("invalid material PAX record bounds")
        try:
            key, value = payload[space + 1:end - 1].decode("utf-8").split("=", 1)
        except (UnicodeDecodeError, ValueError):
            raise MaterialError("invalid material PAX record") from None
        if key in result or key.startswith("GNU.sparse"):
            raise MaterialError("duplicate or sparse material PAX record")
        result[key] = value
        position = end
    return result


def validate_expected_entries(expected_entries, exclusions, root_metadata_only, max_file_bytes):
    if expected_entries is not None:
        if (not isinstance(expected_entries, dict) or not 1 <= len(expected_entries) <= 128
                or exclusions or root_metadata_only):
            raise MaterialError("invalid expected material tree declaration")
        for name, item in expected_entries.items():
            if (not isinstance(name,str) or (name and path_name(name) != name)
                    or not isinstance(item,dict) or set(item) not in [{"kind","mode"},{"kind","mode","sha256","bytes"}]
                    or item.get("kind") not in ["file","5"]
                    or type(item.get("mode")) is not int or not 0 <= item["mode"] <= 0o7777
                    or (item["kind"] == "file" and (set(item) != {"kind","mode","sha256","bytes"}
                        or type(item["bytes"]) is not int or not 0 <= item["bytes"] <= max_file_bytes
                        or not isinstance(item["sha256"],str) or len(item["sha256"]) != 64
                        or any(char not in "0123456789abcdef" for char in item["sha256"])))
                    or (item["kind"] == "5" and set(item) != {"kind","mode"})):
                raise MaterialError("invalid expected material tree declaration")


def tar_digest(chunks, prefix, exclusions=(), *, cancelled=None, deadline=None,
               max_entries=100000, max_bytes=2 * 1024**3, max_file_bytes=256 * 1024**2,
               max_names=16 * 1024**2, max_wire=3 * 1024**3, root_metadata_only=False,
               expected_root_kind=None, expected_entries=None):
    """Hash one declared tree; no file is extracted or symlink followed."""
    source = Reader(chunks, max_wire, time.monotonic() + 600 if deadline is None else deadline, cancelled)
    entries, seen, pending, headers, required_directories = {}, set(), {}, 0, set()
    try:
        validate_expected_entries(expected_entries, exclusions, root_metadata_only, max_file_bytes)
        filesystem_root = root_metadata_only and prefix == "/"
        prefix = "" if filesystem_root else path_name(prefix)
        if not isinstance(exclusions, (list, tuple)) or len(exclusions) > 128:
            raise MaterialError("material exclusions exceed bounds")
        exclusions = sorted(set(path_name(name) for name in exclusions))
        if (not prefix and not filesystem_root) or any(not name for name in exclusions):
            raise MaterialError("material root and exclusions must be explicit")
        if root_metadata_only and exclusions:
            raise MaterialError("directory metadata capture cannot exclude its root")
        while True:
            header = source.read(512)
            if not any(header):
                if any(source.read(512)) or pending:
                    raise MaterialError("material archive has an invalid terminator")
                source.finish()
                break
            headers += 1
            if headers > max_entries * 4 + 16:
                raise MaterialError("material archive exceeds header bound")
            checksum = number(header[148:156])
            if checksum != sum(header[:148]) + 8 * 32 + sum(header[156:]):
                raise MaterialError("material archive header checksum mismatch")
            size, kind = number(header[124:136]), header[156:157]
            if kind in [b"x", b"L", b"K", b"g"]:
                # Crucially validate this BEFORE allocating/reading extension data.
                if kind == b"g" or size > 65536:
                    raise MaterialError("unsupported or oversized material archive metadata")
                max_names -= size
                if max_names < 0:
                    raise MaterialError("material archive metadata exceeds aggregate bound")
                payload = source.read(size)
                source.read((-size) % 512)
                values = pax_fields(payload) if kind == b"x" else {
                    "path" if kind == b"L" else "linkpath": text(payload)}
                if set(values) & set(pending):
                    raise MaterialError("duplicate material archive extension")
                pending.update(values)
                continue
            raw_name = text(header[:100])
            parent = text(header[345:500])
            raw_name = pending.get("path", (parent + "/" if parent else "") + raw_name)
            full_name = "" if filesystem_root and raw_name in ["/", ".", "./"] else path_name(raw_name)
            if full_name != prefix and not full_name.startswith(prefix + "/"):
                raise MaterialError("material archive entry escapes declared root")
            name = full_name[len(prefix):].lstrip("/")
            actual_kind = b"0" if kind == b"\0" else kind
            if not name and expected_root_kind is not None and actual_kind != expected_root_kind:
                raise MaterialError("material archive changed the requested root kind")
            link = pending.get("linkpath", text(header[157:257]))
            max_names -= len(full_name.encode()) + len(link.encode())
            if max_names < 0 or name in seen or len(seen) >= max_entries:
                raise MaterialError("duplicate or excessive material archive names")
            seen.add(name)
            if "size" in pending:
                if not pending["size"].isdigit() or len(pending["size"]) > 20:
                    raise MaterialError("invalid material archive PAX size")
                size = int(pending["size"])
            max_bytes -= size
            if size > max_file_bytes or max_bytes < 0:
                raise MaterialError("material archive exceeds payload bounds")
            ignored = any(name == excluded or name.startswith(excluded + "/") for excluded in exclusions)
            record = {"kind": "file" if kind in [b"\0", b"0"] else kind.decode("ascii", errors="replace"), "mode": number(header[100:108]),
                "uid": number(header[108:116]), "gid": number(header[116:124]), "bytes": size}
            for key in ["uid", "gid"]:
                if key in pending:
                    if not pending[key].isdigit() or len(pending[key]) > 20:
                        raise MaterialError("invalid material PAX ownership")
                    record[key] = int(pending[key])
            if not ignored:
                if kind not in [b"\0", b"0", b"1", b"2", b"5"] or (kind not in [b"\0", b"0"] and size):
                    raise MaterialError("unsupported material archive entry kind")
                if name:
                    if "" in entries and entries[""]["kind"] != "5":
                        raise MaterialError("material archive has a file/directory collision")
                    required_directories.add("")
                parents = name.split("/")[:-1]
                for index in range(1, len(parents) + 1):
                    parent = "/".join(parents[:index])
                    if parent in entries and entries[parent]["kind"] != "5":
                        raise MaterialError("material archive has a file/directory collision")
                    required_directories.add(parent)
                if name in required_directories and kind != b"5":
                    raise MaterialError("material archive has a file/directory collision")
                if kind in [b"1", b"2"]:
                    if len(link.encode()) > 4096 or any(ord(char) < 32 for char in link):
                        raise MaterialError("invalid material link target")
                    record["link"] = link
                    if kind == b"1":
                        target = path_name(link)
                        if (not target.startswith(prefix + "/") or target[len(prefix) + 1:] not in entries
                                or entries[target[len(prefix) + 1:]]["kind"] not in ["file", "1"]):
                            raise MaterialError("material hardlink has an unknown or forward target")
                        record["link"] = target[len(prefix) + 1:]
                        record["inode_target"] = entries[record["link"]].get("inode_target", record["link"])
                record["pax"] = {key: value for key, value in pending.items()
                    if key not in ["path", "linkpath", "size", "uid", "gid", "uname", "gname", "mtime", "atime", "ctime"]}
            if root_metadata_only:
                # Docker emits the requested directory first. Never advance to
                # its descendants: a changed '/' is not a request to scan '/'.
                if name or kind != b"5" or size or ignored:
                    raise MaterialError("material directory metadata root is missing or has wrong kind")
                source.check()
                return canonical_digest(record, "asb-prepared-directory-metadata-v1")
            content, remaining = hashlib.sha256(), size
            while remaining:
                block = source.read(min(remaining, 65536))
                content.update(block)
                remaining -= len(block)
            source.read((-size) % 512)
            if not ignored:
                record["sha256"] = content.hexdigest()
                entries[name] = record
            pending = {}
    finally:
        close = getattr(chunks, "close", None)
        if close is not None:
            close()
    # The aggregate tree can exceed the metadata helper's smaller object cap;
    # stream each bounded entry into a canonical sorted domain instead.
    digest = hashlib.sha256(b"asb-prepared-file-tree-v1\0")
    if "" not in entries:
        raise MaterialError("material archive omitted its declared root")
    if any(name not in entries or entries[name]["kind"] != "5" for name in required_directories):
        raise MaterialError("material archive omitted directory metadata")
    if expected_entries is not None:
        if set(entries) != set(expected_entries) or any(
                any(entries[name].get(key) != value for key,value in expected.items())
                for name,expected in expected_entries.items()):
            raise MaterialError("staged material differs from its exact owned declaration")
    # TAR writers may choose different first names for the same hardlinked
    # inode. Canonicalize each group by its lowest path, not archive order.
    groups = {}
    for name, record in entries.items():
        source.check()
        if record["kind"] in ["file", "1"]:
            groups.setdefault(record.get("inode_target", name), []).append(name)
    for target, names in groups.items():
        source.check()
        original = entries[target]
        canonical = min(names)
        for name in names:
            record = entries[name]
            if any(record[key] != original[key] for key in ["mode", "uid", "gid"]):
                raise MaterialError("material hardlinks have contradictory inode metadata")
            if any(original["pax"].get(key) != value for key, value in record["pax"].items()):
                raise MaterialError("material hardlinks have contradictory inode attributes")
            entries[name] = dict(original, inode=canonical)
    digest.update(json.dumps(exclusions, separators=(",", ":")).encode())
    for name, record in sorted(entries.items()):
        source.check()
        digest.update(json.dumps([name, record], sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode())
        digest.update(b"\0")
    return digest.hexdigest()
