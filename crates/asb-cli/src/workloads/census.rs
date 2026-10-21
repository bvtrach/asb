//! Read-only Linux census of explicitly declared roots, not a machine snapshot.
//!
//! User paths are traversed through pinned O_PATH descriptors without following
//! symlinks. Proc descriptor references reopen only already-verified regular
//! inodes or directories. Numeric inode identities never leave this process.
#![allow(unsafe_code)]

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{CString, OsString};
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Component, Path};
use std::process::ExitCode;
use std::time::{Duration, Instant};

const MAX_REQUEST: usize = 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    schema_version: u32,
    roots: Vec<Root>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Root {
    name: String,
    path: String,
    #[serde(default)]
    exclusions: Vec<String>,
}

#[derive(Serialize)]
struct Observation {
    schema_version: u32,
    scope: &'static str,
    sha256: String,
    entries: usize,
    bytes: u64,
    coverage: Coverage,
}

#[derive(Serialize)]
struct Coverage {
    file_content: bool,
    posix_metadata: bool,
    visible_inode_equivalence: bool,
    extended_attributes: bool,
    complete_namespace: bool,
    atomic_snapshot: bool,
}

#[derive(Clone, Serialize)]
struct Record {
    name: String,
    kind: &'static str,
    mode: u32,
    uid: u32,
    gid: u32,
    bytes: Option<u64>,
    sha256: Option<String>,
    link: Option<String>,
    inode_group: Option<String>,
    #[serde(skip)]
    inode: Option<(u64, u64)>,
    #[serde(skip)]
    version: Option<(i64, i64, i64, i64, u64)>,
}

struct Census {
    records: Vec<Record>,
    entries_left: usize,
    names_left: usize,
    bytes_left: u64,
    deadline: Instant,
}

fn invalid() -> &'static str {
    "cannot establish bounded declared-root material observation"
}

fn relative_name(value: &str, empty: bool) -> bool {
    (empty || !value.is_empty())
        && value.len() <= 4096
        && !value.starts_with('/')
        && !value.chars().any(char::is_control)
        && (value.is_empty()
            || value
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != ".."))
}

fn absolute_name(value: &str) -> bool {
    value == "/"
        || value
            .strip_prefix('/')
            .is_some_and(|v| relative_name(v, false))
}

fn open_at(parent: &File, name: &str) -> Result<File, &'static str> {
    let name = CString::new(name).map_err(|_| invalid())?;
    // SAFETY: parent is held open; the C string is terminated and contains no
    // NUL. O_PATH opens symlink inodes without following or reading devices.
    let descriptor = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if descriptor < 0 {
        return Err(invalid());
    }
    // SAFETY: successful openat returned a fresh owned descriptor.
    Ok(unsafe { File::from_raw_fd(descriptor) })
}

fn pin_root(path: &str) -> Result<File, &'static str> {
    let mut current = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_PATH | libc::O_DIRECTORY | libc::O_CLOEXEC)
        .open("/")
        .map_err(|_| invalid())?;
    for component in Path::new(path).components() {
        if let Component::Normal(name) = component {
            if !current.metadata().map_err(|_| invalid())?.is_dir() {
                return Err(invalid());
            }
            current = open_at(&current, name.to_str().ok_or_else(invalid)?)?;
        }
    }
    Ok(current)
}

fn stable(a: &Metadata, b: &Metadata) -> bool {
    a.dev() == b.dev()
        && a.ino() == b.ino()
        && a.mode() == b.mode()
        && a.uid() == b.uid()
        && a.gid() == b.gid()
        && a.len() == b.len()
        && a.mtime() == b.mtime()
        && a.mtime_nsec() == b.mtime_nsec()
        && a.ctime() == b.ctime()
        && a.ctime_nsec() == b.ctime_nsec()
        && a.nlink() == b.nlink()
}

fn descriptor_path(file: &File) -> String {
    format!("/proc/self/fd/{}", file.as_raw_fd())
}

impl Census {
    fn check(&self) -> Result<(), &'static str> {
        if Instant::now() >= self.deadline {
            return Err(invalid());
        }
        Ok(())
    }

    fn charge_name(&mut self, name: &str) -> Result<(), &'static str> {
        self.names_left = self
            .names_left
            .checked_sub(name.len())
            .ok_or_else(invalid)?;
        Ok(())
    }

    fn visit(
        &mut self,
        root: &Root,
        relative: &str,
        file: File,
        depth: usize,
    ) -> Result<(), &'static str> {
        self.check()?;
        if depth > 128 || self.entries_left == 0 {
            return Err(invalid());
        }
        self.entries_left -= 1;
        let before = file.metadata().map_err(|_| invalid())?;
        let mut record = Record {
            name: format!("{}/{}", root.name, relative),
            kind: "directory",
            mode: before.mode(),
            uid: before.uid(),
            gid: before.gid(),
            bytes: None,
            sha256: None,
            link: None,
            inode_group: None,
            inode: None,
            version: None,
        };
        self.charge_name(&record.name)?;
        if before.is_dir() {
            let mut children = Vec::new();
            for child in fs::read_dir(descriptor_path(&file)).map_err(|_| invalid())? {
                self.check()?;
                if children.len() >= self.entries_left {
                    return Err(invalid());
                }
                let name = child.map_err(|_| invalid())?.file_name();
                let name = name
                    .to_str()
                    .filter(|n| relative_name(n, false))
                    .ok_or_else(invalid)?;
                let relative = if relative.is_empty() {
                    name.to_owned()
                } else {
                    format!("{relative}/{name}")
                };
                if root.exclusions.iter().any(|excluded| {
                    relative == *excluded || relative.starts_with(&format!("{excluded}/"))
                }) {
                    continue;
                }
                self.charge_name(&relative)?;
                self.charge_name(name)?;
                children.push((name.to_owned(), relative));
            }
            children.sort_unstable();
            for (name, relative) in children {
                self.visit(root, &relative, open_at(&file, &name)?, depth + 1)?;
            }
        } else if before.is_file() {
            record.kind = "file";
            record.bytes = Some(before.len());
            record.inode = Some((before.dev(), before.ino()));
            record.version = Some((
                before.mtime(),
                before.mtime_nsec(),
                before.ctime(),
                before.ctime_nsec(),
                before.nlink(),
            ));
            self.bytes_left = self
                .bytes_left
                .checked_sub(before.len())
                .ok_or_else(invalid)?;
            // The proc reference targets this pinned, verified regular inode,
            // not a user path that could be replaced by a symlink between calls.
            let mut reader = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
                .open(descriptor_path(&file))
                .map_err(|_| invalid())?;
            if !stable(&before, &reader.metadata().map_err(|_| invalid())?) {
                return Err(invalid());
            }
            let mut hash = Sha256::new();
            let mut left = before.len();
            let mut buffer = [0u8; 65536];
            while left > 0 {
                self.check()?;
                let bound = left.min(buffer.len() as u64) as usize;
                let count = reader.read(&mut buffer[..bound]).map_err(|_| invalid())?;
                if count == 0 {
                    return Err(invalid());
                }
                hash.update(&buffer[..count]);
                left -= count as u64;
            }
            if reader.read(&mut buffer[..1]).map_err(|_| invalid())? != 0 {
                return Err(invalid());
            }
            record.sha256 = Some(format!("{:x}", hash.finalize()));
        } else if before.file_type().is_symlink() {
            record.kind = "symlink";
            let mut target = [0u8; 4097];
            // SAFETY: empty readlinkat name reads this pinned O_PATH symlink;
            // target is a valid output buffer of the supplied size.
            let size = unsafe {
                libc::readlinkat(
                    file.as_raw_fd(),
                    c"".as_ptr(),
                    target.as_mut_ptr().cast(),
                    target.len(),
                )
            };
            if size < 0 || size as usize > 4096 {
                return Err(invalid());
            }
            let target = std::str::from_utf8(&target[..size as usize]).map_err(|_| invalid())?;
            self.charge_name(target)?;
            record.link = Some(target.to_owned());
        } else {
            return Err(invalid());
        }
        self.check()?;
        if !stable(&before, &file.metadata().map_err(|_| invalid())?) {
            return Err(invalid());
        }
        self.records.push(record);
        Ok(())
    }
}

fn inode_groups(records: &[Record]) -> Result<BTreeMap<(u64, u64), usize>, &'static str> {
    let mut observed: BTreeMap<(u64, u64), usize> = BTreeMap::new();
    for (index, record) in records.iter().enumerate() {
        if let Some(inode) = record.inode {
            if let Some(previous) = observed.get(&inode) {
                let previous = &records[*previous];
                if record.mode != previous.mode
                    || record.uid != previous.uid
                    || record.gid != previous.gid
                    || record.bytes != previous.bytes
                    || record.sha256 != previous.sha256
                    || record.version != previous.version
                {
                    return Err(invalid());
                }
                if record.name < previous.name {
                    observed.insert(inode, index);
                }
            } else {
                observed.insert(inode, index);
            }
        }
    }
    Ok(observed)
}

fn observe(
    request: Request,
    entries: usize,
    names: usize,
    bytes: u64,
) -> Result<Observation, &'static str> {
    if request.schema_version != 1 || request.roots.is_empty() || request.roots.len() > 64 {
        return Err(invalid());
    }
    let mut roles = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for root in &request.roots {
        if root.name.is_empty()
            || root.name.len() > 128
            || !root
                .name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            || !roles.insert(&root.name)
            || !absolute_name(&root.path)
            || !paths.insert(&root.path)
            || root.exclusions.len() > 128
            || root
                .exclusions
                .iter()
                .any(|name| !relative_name(name, false))
        {
            return Err(invalid());
        }
    }
    let mut census = Census {
        records: Vec::new(),
        entries_left: entries,
        names_left: names,
        bytes_left: bytes,
        deadline: Instant::now() + Duration::from_secs(60),
    };
    let mut exclusions = BTreeMap::new();
    for root in &request.roots {
        census.charge_name(&root.name)?;
        let mut declared = root.exclusions.clone();
        declared.sort_unstable();
        declared.dedup();
        for name in &declared {
            census.charge_name(name)?;
        }
        exclusions.insert(&root.name, declared);
        let file = pin_root(&root.path)?;
        let before = file.metadata().map_err(|_| invalid())?;
        census.visit(root, "", file, 0)?;
        if !stable(
            &before,
            &pin_root(&root.path)?.metadata().map_err(|_| invalid())?,
        ) {
            return Err(invalid());
        }
    }
    census.records.sort_unstable_by(|a, b| a.name.cmp(&b.name));
    let inodes = inode_groups(&census.records)?;
    let mut digest = Sha256::new();
    digest.update(b"asb-declared-root-material-v1\0");
    digest.update(serde_json::to_vec(&exclusions).map_err(|_| invalid())?);
    for index in 0..census.records.len() {
        if Instant::now() >= census.deadline {
            return Err(invalid());
        }
        let group = census.records[index].inode.and_then(|key| inodes.get(&key));
        let canonical = if let Some(group) = group {
            let name = &census.records[*group].name;
            census.names_left = census
                .names_left
                .checked_sub(name.len())
                .ok_or_else(invalid)?;
            Some(name.clone())
        } else {
            None
        };
        let record = &mut census.records[index];
        record.inode_group = canonical;
        digest.update(serde_json::to_vec(record).map_err(|_| invalid())?);
        digest.update(b"\0");
    }
    Ok(Observation {
        schema_version: 1,
        scope: "declared-root-material-observation-v1",
        sha256: format!("{:x}", digest.finalize()),
        entries: census.records.len(),
        bytes: bytes - census.bytes_left,
        coverage: Coverage {
            file_content: true,
            posix_metadata: true,
            visible_inode_equivalence: true,
            extended_attributes: false,
            complete_namespace: false,
            atomic_snapshot: false,
        },
    })
}

pub(crate) fn entry(args: &[OsString]) -> ExitCode {
    let result = (|| {
        if !args.is_empty() {
            return Err(invalid());
        }
        let mut input = Vec::new();
        std::io::stdin()
            .take((MAX_REQUEST + 1) as u64)
            .read_to_end(&mut input)
            .map_err(|_| invalid())?;
        if input.len() > MAX_REQUEST {
            return Err(invalid());
        }
        let request = serde_json::from_slice(&input).map_err(|_| invalid())?;
        let value = observe(request, 100_000, 16 * 1024 * 1024, 2 * 1024 * 1024 * 1024)?;
        let output = serde_json::to_vec(&value).map_err(|_| invalid())?;
        std::io::stdout()
            .lock()
            .write_all(&output)
            .map_err(|_| invalid())?;
        Ok(())
    })();
    if result.is_ok() {
        ExitCode::SUCCESS
    } else {
        eprintln!("asb: {}", invalid());
        ExitCode::from(2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir()
                .join(crate::workloads::TaskIdentity::local("census", None).attempt_id);
            fs::create_dir_all(&root).unwrap();
            Self(root)
        }
        fn request(&self, names: &[&str]) -> Request {
            Request {
                schema_version: 1,
                roots: names
                    .iter()
                    .map(|name| Root {
                        name: name.to_string(),
                        path: self.0.join(name).to_str().unwrap().into(),
                        exclusions: vec![],
                    })
                    .collect(),
            }
        }
        fn digest(&self, names: &[&str]) -> String {
            observe(self.request(names), 1000, 65536, 1024 * 1024)
                .unwrap()
                .sha256
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn declared_roots_bind_cross_root_inode_aliases_not_numeric_inodes_or_input_order() {
        let fixture = Fixture::new();
        for name in ["left", "right"] {
            fs::create_dir(fixture.0.join(name)).unwrap();
        }
        let a = fixture.0.join("left/a");
        let b = fixture.0.join("right/b");
        fs::write(&a, "same bytes").unwrap();
        fs::hard_link(&a, &b).unwrap();
        let linked = fixture.digest(&["left", "right"]);
        assert_eq!(linked, fixture.digest(&["right", "left"]));
        fs::remove_file(&b).unwrap();
        fs::write(&b, "same bytes").unwrap();
        assert_ne!(linked, fixture.digest(&["left", "right"]));
        fs::remove_file(&a).unwrap();
        fs::hard_link(&b, &a).unwrap();
        assert_eq!(linked, fixture.digest(&["left", "right"]));
    }

    #[test]
    fn contradictory_inode_content_metadata_and_versions_are_never_grouped() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.0.join("workspace")).unwrap();
        fs::write(fixture.0.join("workspace/a"), "same bytes").unwrap();
        fs::hard_link(fixture.0.join("workspace/a"), fixture.0.join("workspace/b")).unwrap();
        let request = fixture.request(&["workspace"]);
        let mut census = Census {
            records: vec![],
            entries_left: 1000,
            names_left: 65536,
            bytes_left: 1024,
            deadline: Instant::now() + Duration::from_secs(60),
        };
        census
            .visit(
                &request.roots[0],
                "",
                pin_root(&request.roots[0].path).unwrap(),
                0,
            )
            .unwrap();
        let files: Vec<_> = census
            .records
            .iter()
            .filter(|record| record.inode.is_some())
            .cloned()
            .collect();
        assert_eq!(files.len(), 2);
        assert!(inode_groups(&files).is_ok());
        for field in ["content", "mode", "version"] {
            let mut changed = files.clone();
            match field {
                "content" => changed[1].sha256 = Some("0".repeat(64)),
                "mode" => changed[1].mode ^= 0o100,
                _ => changed[1].version.as_mut().unwrap().0 += 1,
            }
            assert!(inode_groups(&changed).is_err());
        }
    }

    #[test]
    fn physical_roots_timestamps_and_creation_order_do_not_replace_logical_material() {
        let a = Fixture::new();
        let b = Fixture::new();
        for (fixture, names) in [(&a, ["z", "a"]), (&b, ["a", "z"])] {
            fs::create_dir(fixture.0.join("workspace")).unwrap();
            fs::create_dir(fixture.0.join("workspace/empty")).unwrap();
            for name in names {
                fs::write(fixture.0.join("workspace").join(name), name).unwrap();
            }
        }
        let original = a.digest(&["workspace"]);
        assert_eq!(original, b.digest(&["workspace"]));
        let path = b.0.join("workspace/a");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        assert_ne!(original, b.digest(&["workspace"]));
    }

    #[test]
    fn symlink_content_is_observed_without_following_links_or_linked_root_ancestors() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.0.join("workspace")).unwrap();
        symlink("/does-not-exist", fixture.0.join("workspace/link")).unwrap();
        let original = fixture.digest(&["workspace"]);
        fs::remove_file(fixture.0.join("workspace/link")).unwrap();
        symlink(
            "/different-missing-target",
            fixture.0.join("workspace/link"),
        )
        .unwrap();
        assert_ne!(original, fixture.digest(&["workspace"]));
        symlink(fixture.0.join("workspace"), fixture.0.join("alias")).unwrap();
        let mut request = fixture.request(&["workspace"]);
        request.roots[0].path = fixture.0.join("alias/link").to_str().unwrap().into();
        assert!(observe(request, 1000, 65536, 1024 * 1024).is_err());
    }

    #[test]
    fn declared_exclusions_are_bound_and_cannot_escape_or_hide_the_root() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.0.join("workspace")).unwrap();
        fs::write(fixture.0.join("workspace/file"), "bytes").unwrap();
        let original = fixture.digest(&["workspace"]);
        let mut request = fixture.request(&["workspace"]);
        request.roots[0].exclusions = vec!["file".into()];
        let excluded = observe(request, 1000, 65536, 1024 * 1024).unwrap();
        assert_ne!(original, excluded.sha256);
        assert_eq!(excluded.entries, 1);
        for exclusion in ["", "../file", "/file", "a//file", "a/./file"] {
            let mut request = fixture.request(&["workspace"]);
            request.roots[0].exclusions = vec![exclusion.into()];
            assert!(observe(request, 1000, 65536, 1024 * 1024).is_err());
        }
    }

    #[test]
    fn global_resource_bounds_and_unsupported_fifo_never_open_special_payloads() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.0.join("workspace")).unwrap();
        fs::write(fixture.0.join("workspace/file"), "bytes").unwrap();
        for (entries, names, bytes) in [(1, 65536, 1024), (1000, 1, 1024), (1000, 65536, 1)] {
            assert!(observe(fixture.request(&["workspace"]), entries, names, bytes).is_err());
        }
        let mut request = fixture.request(&["workspace"]);
        request.roots[0].name = "x".repeat(60);
        assert!(
            observe(request, 1000, 128, 1024).is_err(),
            "retained logical prefixes must be charged"
        );
        let path = CString::new(fixture.0.join("workspace/fifo").to_str().unwrap()).unwrap();
        // SAFETY: path is a terminated string for a new fixture-owned FIFO.
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        assert!(observe(fixture.request(&["workspace"]), 1000, 65536, 1024).is_err());
    }

    #[test]
    fn invalid_schema_roles_roots_and_expired_deadlines_are_rejected() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.0.join("workspace")).unwrap();
        let mut request = fixture.request(&["workspace"]);
        request.schema_version = 2;
        assert!(observe(request, 1000, 65536, 1024).is_err());
        assert!(
            observe(
                fixture.request(&["workspace", "workspace"]),
                1000,
                65536,
                1024
            )
            .is_err()
        );
        let mut request = fixture.request(&["workspace"]);
        request.roots[0].path.push('/');
        assert!(observe(request, 1000, 65536, 1024).is_err());
        let census = Census {
            records: vec![],
            entries_left: 100,
            names_left: 100,
            bytes_left: 100,
            deadline: Instant::now() - Duration::from_secs(1),
        };
        assert!(census.check().is_err());
    }
}
