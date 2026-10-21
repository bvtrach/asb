//! Bounded visible Linux xattrs, including visible ACL/capability bytes.
//!
//! Enumeration is caller-visible, not proof that privileged trusted.* attributes
//! do not exist. No privilege escalation or empty-on-permission-error fallback.
#![allow(unsafe_code)]

use sha2::{Digest, Sha256};
use std::ffi::{CStr, CString};
use std::fs::{self, File, Metadata};
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::time::Instant;

const MAX_BLOB: usize = 65536;

pub(super) enum Target<'a> {
    Descriptor(&'a File),
    Link {
        parent: &'a File,
        name: &'a str,
        pinned: &'a File,
    },
}

fn invalid() -> &'static str {
    "cannot establish bounded visible attribute observation"
}

pub(super) fn stable(a: &Metadata, b: &Metadata) -> bool {
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

impl Target<'_> {
    fn metadata(&self) -> Result<Metadata, &'static str> {
        match self {
            Self::Descriptor(file) => file,
            Self::Link { pinned, .. } => pinned,
        }
        .metadata()
        .map_err(|_| invalid())
    }

    fn link_path(&self) -> Result<CString, &'static str> {
        let Self::Link { parent, name, .. } = self else {
            return Err(invalid());
        };
        if name.is_empty() || name.contains('/') || *name == "." || *name == ".." {
            return Err(invalid());
        }
        CString::new(format!("/proc/self/fd/{}/{name}", parent.as_raw_fd())).map_err(|_| invalid())
    }

    fn verify(&self) -> Result<(), &'static str> {
        if matches!(self, Self::Link { .. }) {
            let path = self.link_path()?;
            let path = std::path::Path::new(std::ffi::OsStr::from_bytes(path.as_bytes()));
            if !stable(
                &self.metadata()?,
                &fs::symlink_metadata(path).map_err(|_| invalid())?,
            ) {
                return Err("visible attribute symlink entry changed during observation");
            }
        }
        Ok(())
    }

    fn query(
        &self,
        name: Option<&CStr>,
        pointer: *mut u8,
        size: usize,
    ) -> Result<usize, &'static str> {
        self.verify()?;
        let before = self.metadata()?;
        // SAFETY: caller supplies a live buffer of size bytes (or null for a
        // size probe). Files are held open and names/paths are terminated C
        // strings. l* calls do not follow the final symlink; its parent is pinned.
        let result = unsafe {
            match self {
                Self::Descriptor(file) => match name {
                    Some(name) => {
                        libc::fgetxattr(file.as_raw_fd(), name.as_ptr(), pointer.cast(), size)
                    }
                    None => libc::flistxattr(file.as_raw_fd(), pointer.cast(), size),
                },
                Self::Link { .. } => {
                    let path = self.link_path()?;
                    match name {
                        Some(name) => {
                            libc::lgetxattr(path.as_ptr(), name.as_ptr(), pointer.cast(), size)
                        }
                        None => libc::llistxattr(path.as_ptr(), pointer.cast(), size),
                    }
                }
            }
        };
        let error = (result < 0).then(std::io::Error::last_os_error);
        self.verify()?;
        if let Some(error) = error {
            return Err(match error.raw_os_error() {
                Some(libc::EACCES) | Some(libc::EPERM) => "visible attribute access was denied",
                Some(libc::ENOTSUP) => "filesystem does not support visible attribute reads",
                Some(libc::ERANGE) => "visible attribute size changed during observation",
                Some(libc::ENODATA) => "visible attribute disappeared during observation",
                _ => "visible attribute syscall failed",
            });
        }
        if !stable(&before, &self.metadata()?) {
            return Err("visible attribute inode changed during observation");
        }
        Ok(result as usize)
    }
}

use std::os::unix::ffi::OsStrExt;

fn charge(budget: &mut usize, size: usize) -> Result<(), &'static str> {
    *budget = budget
        .checked_sub(size)
        .ok_or("visible attribute aggregate byte budget exceeded")?;
    Ok(())
}

fn bounded_blob(
    mut query: impl FnMut(*mut u8, usize) -> Result<usize, &'static str>,
    budget: &mut usize,
) -> Result<Vec<u8>, &'static str> {
    let size = query(std::ptr::null_mut(), 0)?;
    if size > MAX_BLOB {
        return Err("visible attribute list or value exceeds 64 KiB");
    }
    charge(budget, size)?;
    let mut bytes = vec![0; size];
    let actual = query(bytes.as_mut_ptr(), size)?;
    if actual > size {
        return Err("visible attribute read exceeded its probed bound");
    }
    // OverlayFS may return the underlying list's size for a null-buffer probe
    // and filter private attributes only on the actual read. Charge the whole
    // allocation, but observe only returned bytes. Re-list/re-read below still
    // require stable visible names/values; an upper-bound probe is not content.
    bytes.truncate(actual);
    Ok(bytes)
}

fn names(target: &Target<'_>, budget: &mut usize) -> Result<Vec<Vec<u8>>, &'static str> {
    let bytes = bounded_blob(|pointer, size| target.query(None, pointer, size), budget)?;
    if !bytes.is_empty() && bytes.last() != Some(&0) {
        return Err("visible attribute list has an invalid terminator");
    }
    let mut result = Vec::new();
    for name in bytes.split(|b| *b == 0).take_while(|name| !name.is_empty()) {
        if result.len() >= 1024 || name.len() > 255 {
            return Err("visible attribute names exceed bounds");
        }
        charge(budget, name.len())?;
        result.push(name.to_vec());
    }
    // Empty interior names must not silently truncate the reported list.
    if result.iter().map(|name| name.len() + 1).sum::<usize>() != bytes.len() {
        return Err("visible attribute list has ambiguous names");
    }
    result.sort_unstable();
    if result.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(invalid());
    }
    Ok(result)
}

fn values(
    target: &Target<'_>,
    names: &[Vec<u8>],
    budget: &mut usize,
    deadline: Instant,
) -> Result<Vec<(u64, [u8; 32])>, &'static str> {
    let mut result = Vec::new();
    for name in names {
        if Instant::now() >= deadline {
            return Err(invalid());
        }
        let name = CString::new(name.as_slice()).map_err(|_| invalid())?;
        charge(budget, name.as_bytes().len())?;
        let bytes = bounded_blob(
            |pointer, size| target.query(Some(&name), pointer, size),
            budget,
        )?;
        result.push((bytes.len() as u64, Sha256::digest(&bytes).into()));
    }
    Ok(result)
}

#[derive(Eq, PartialEq)]
struct AttributeSet {
    names: Vec<Vec<u8>>,
    values: Vec<(u64, [u8; 32])>,
}

fn read_pass(
    target: &Target<'_>,
    budget: &mut usize,
    deadline: Instant,
) -> Result<AttributeSet, &'static str> {
    let names = names(target, budget)?;
    let values = values(target, &names, budget, deadline)?;
    Ok(AttributeSet { names, values })
}

fn require_match(first: &AttributeSet, second: &AttributeSet) -> Result<(), &'static str> {
    if first != second {
        return Err("visible attribute names or values changed between observations");
    }
    Ok(())
}

pub(super) fn observe(
    target: Target<'_>,
    budget: &mut usize,
    deadline: Instant,
) -> Result<String, &'static str> {
    if Instant::now() >= deadline {
        return Err(invalid());
    }
    let before = target.metadata()?;
    let first = read_pass(&target, budget, deadline)?;
    require_match(&first, &read_pass(&target, budget, deadline)?)?;
    if !stable(&before, &target.metadata()?) || Instant::now() >= deadline {
        return Err(invalid());
    }
    let mut digest = Sha256::new();
    digest.update(b"asb-visible-linux-attributes-v1\0");
    for (name, (length, value)) in first.names.iter().zip(first.values) {
        digest.update((name.len() as u64).to_le_bytes());
        digest.update(name);
        digest.update(length.to_le_bytes());
        digest.update(value);
    }
    Ok(format!("{:x}", digest.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::FromRawFd;
    use std::os::unix::fs::symlink;
    use std::time::Duration;

    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir()
                .join(crate::workloads::TaskIdentity::local("attributes", None).attempt_id);
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn file(&self) -> File {
            let path = self.0.join("file");
            if !path.exists() {
                fs::write(&path, "fixture bytes").unwrap();
            }
            File::open(path).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    fn set(file: &File, name: &[u8], value: &[u8]) {
        let name = CString::new(name).unwrap();
        // SAFETY: live file and terminated name; value covers the supplied size.
        assert_eq!(
            unsafe {
                libc::fsetxattr(
                    file.as_raw_fd(),
                    name.as_ptr(),
                    value.as_ptr().cast(),
                    value.len(),
                    0,
                )
            },
            0
        );
    }
    fn digest(file: &File) -> String {
        observe(
            Target::Descriptor(file),
            &mut (16 * 1024 * 1024),
            Instant::now() + Duration::from_secs(5),
        )
        .unwrap()
    }

    #[test]
    fn binary_names_values_and_empty_attributes_are_bound_without_raw_output() {
        let fixture = Fixture::new();
        let file = fixture.file();
        let empty = digest(&file);
        set(&file, b"user.asb", b"secret\0binary\xff");
        let first = digest(&file);
        assert_ne!(empty, first);
        assert_eq!(first, digest(&file));
        assert_eq!(first.len(), 64);
        assert!(!first.contains("secret"));
        set(&file, b"user.asb", b"changed\0binary\xff");
        assert_ne!(first, digest(&file));
        set(&file, b"user.\xff", b"");
        assert_ne!(empty, digest(&file));
        let second = Fixture::new();
        let other = second.file();
        set(&other, b"user.\xff", b"");
        set(&other, b"user.asb", b"changed\0binary\xff");
        assert_eq!(digest(&file), digest(&other));
    }

    #[test]
    fn oversized_probe_is_refused_before_allocation_and_changed_size_is_not_empty() {
        let mut calls = 0;
        let mut budget = 16 * 1024 * 1024;
        assert!(
            bounded_blob(
                |_, size| {
                    calls += 1;
                    assert_eq!(size, 0);
                    Ok(MAX_BLOB + 1)
                },
                &mut budget
            )
            .is_err()
        );
        assert_eq!(calls, 1);
        assert_eq!(budget, 16 * 1024 * 1024);
        let mut calls = 0;
        assert!(
            bounded_blob(
                |_, _| {
                    calls += 1;
                    Ok(if calls == 1 { 0 } else { 1 })
                },
                &mut budget
            )
            .is_err()
        );
        assert_eq!(calls, 2);
        assert!(bounded_blob(|_, _| Err("fixture permission failure"), &mut budget).is_err());
    }

    #[test]
    fn overestimated_overlay_probe_uses_only_actual_returned_bytes_and_charges_allocation() {
        let mut budget = 128;
        let mut calls = 0;
        let bytes = bounded_blob(
            |pointer, size| {
                calls += 1;
                if calls == 1 {
                    return Ok(64);
                }
                assert_eq!(size, 64);
                let visible = b"user.asb\0";
                // SAFETY: bounded_blob allocated the supplied64-byte output buffer.
                unsafe { std::ptr::copy_nonoverlapping(visible.as_ptr(), pointer, visible.len()) };
                Ok(visible.len())
            },
            &mut budget,
        )
        .unwrap();
        assert_eq!(bytes, b"user.asb\0");
        assert_eq!(budget, 64);
        let mut calls = 0;
        let bytes = bounded_blob(
            |_, _| {
                calls += 1;
                Ok(if calls == 1 { 32 } else { 0 })
            },
            &mut budget,
        )
        .unwrap();
        assert!(bytes.is_empty());
        assert_eq!(budget, 32);
    }

    #[test]
    fn byte_budget_and_deadline_apply_across_read_passes_and_nodes() {
        let fixture = Fixture::new();
        let file = fixture.file();
        set(&file, b"user.asb", b"nonempty");
        assert!(
            observe(
                Target::Descriptor(&file),
                &mut 1,
                Instant::now() + Duration::from_secs(1)
            )
            .is_err()
        );
        assert!(
            observe(
                Target::Descriptor(&file),
                &mut 65536,
                Instant::now() - Duration::from_secs(1)
            )
            .is_err()
        );
        let mut budget = 65536;
        observe(
            Target::Descriptor(&file),
            &mut budget,
            Instant::now() + Duration::from_secs(1),
        )
        .unwrap();
        assert!(budget < 65536 - 2 * (b"user.asb".len() + b"nonempty".len()));
        let consumed = 65536 - budget;
        let mut budget = consumed;
        observe(
            Target::Descriptor(&file),
            &mut budget,
            Instant::now() + Duration::from_secs(1),
        )
        .unwrap();
        assert_eq!(budget, 0);
        assert!(
            observe(
                Target::Descriptor(&file),
                &mut budget,
                Instant::now() + Duration::from_secs(1)
            )
            .is_err()
        );
    }

    #[test]
    fn symlink_attributes_do_not_follow_target_and_replaced_link_is_rejected() {
        let fixture = Fixture::new();
        let file = fixture.file();
        set(&file, b"user.asb", b"target-secret-not-link-metadata");
        let path = fixture.0.join("link");
        symlink("file", &path).unwrap();
        let parent = File::open(&fixture.0).unwrap();
        let name = c"link";
        // SAFETY: the held parent and static name are valid; resulting fresh
        // O_PATH descriptor pins the symlink without following its target.
        let descriptor = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                name.as_ptr(),
                libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        assert!(descriptor >= 0);
        let pinned = unsafe { File::from_raw_fd(descriptor) };
        let target = || Target::Link {
            parent: &parent,
            name: "link",
            pinned: &pinned,
        };
        let link = observe(
            target(),
            &mut 65536,
            Instant::now() + Duration::from_secs(1),
        )
        .unwrap();
        assert_ne!(link, digest(&file));
        fs::remove_file(&path).unwrap();
        symlink("different", &path).unwrap();
        assert!(
            observe(
                target(),
                &mut 65536,
                Instant::now() + Duration::from_secs(1)
            )
            .is_err()
        );
    }

    #[test]
    fn second_pass_rejects_same_sized_value_and_name_mutations() {
        let fixture = Fixture::new();
        let file = fixture.file();
        set(&file, b"user.asb", b"aaaa");
        let target = Target::Descriptor(&file);
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut budget = 65536;
        let first = read_pass(&target, &mut budget, deadline).unwrap();
        require_match(&first, &read_pass(&target, &mut budget, deadline).unwrap()).unwrap();
        set(&file, b"user.asb", b"bbbb");
        let changed = read_pass(&target, &mut budget, deadline).unwrap();
        assert!(require_match(&first, &changed).is_err());
        // SAFETY: descriptor is live and static attribute name is terminated.
        assert_eq!(
            unsafe { libc::fremovexattr(file.as_raw_fd(), c"user.asb".as_ptr()) },
            0
        );
        set(&file, b"user.asc", b"bbbb");
        assert!(
            require_match(
                &changed,
                &read_pass(&target, &mut budget, deadline).unwrap()
            )
            .is_err()
        );
    }
}
