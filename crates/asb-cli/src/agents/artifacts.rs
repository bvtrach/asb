//! Attempt-scoped scratch ownership, frozen before interpretation and settled once.
use std::collections::BTreeMap;
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};

const LIMIT: u64 = 16 * 1024 * 1024;

#[derive(Clone, Copy)]
pub(crate) struct ArtifactSpec {
    pub namespace: &'static str,
    pub filename: &'static str,
}

struct OwnedFile {
    path: PathBuf,
    directory: Observation,
    observed: Option<Observation>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Identity(u64, u128);
struct Observation {
    metadata: Metadata,
    identity: Option<Identity>,
}
impl Observation {
    fn read(file: &File) -> std::io::Result<Self> {
        Ok(Self {
            metadata: file.metadata()?,
            identity: identity(file)?,
        })
    }
}

fn open_regular_or_directory(path: &Path) -> std::io::Result<File> {
    if fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(std::io::Error::other("artifact symlink refused"));
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Match std's handle-based metadata route; open a reparse point itself,
        // never its target, and permit directory handles for namespace identity.
        options.custom_flags(0x0200_0000 | 0x0020_0000);
    }
    options.open(path)
}

#[cfg(unix)]
fn identity(file: &File) -> std::io::Result<Option<Identity>> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata()?;
    Ok(Some(Identity(metadata.dev(), metadata.ino().into())))
}
#[cfg(windows)]
#[allow(unsafe_code)]
fn identity(file: &File) -> std::io::Result<Option<Identity>> {
    use std::os::windows::fs::MetadataExt;
    use std::os::windows::io::AsRawHandle;
    if file.metadata()?.file_attributes() & 0x400 != 0 {
        return Err(std::io::Error::other("artifact reparse point refused"));
    }
    // FILE_ID_INFO retains the complete 128-bit identifier, including ReFS.
    // FileIdInfo=18 matches the pinned std Windows ABI declarations.
    #[repr(C)]
    struct Information {
        volume: u64,
        id: [u8; 16],
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFileInformationByHandleEx(
            handle: *mut std::ffi::c_void,
            class: i32,
            info: *mut std::ffi::c_void,
            length: u32,
        ) -> i32;
    }
    let mut information = std::mem::MaybeUninit::<Information>::uninit();
    // SAFETY: std owns this live handle; the API initializes Information on success.
    if unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            18,
            information.as_mut_ptr().cast(),
            std::mem::size_of::<Information>() as u32,
        )
    } == 0
    {
        // A filesystem without stable IDs retains private artifacts rather
        // than disabling an otherwise supported agent or guessing ownership.
        return Ok(None);
    }
    let information = unsafe { information.assume_init() };
    Ok(Some(Identity(
        information.volume,
        u128::from_le_bytes(information.id),
    )))
}
#[cfg(not(any(unix, windows)))]
fn identity(_file: &File) -> std::io::Result<Option<Identity>> {
    Ok(None)
}

#[derive(Default)]
pub(crate) struct ArtifactOwner {
    files: Vec<OwnedFile>,
    settled: bool,
    frozen: bool,
}

#[derive(Default)]
pub(crate) struct FrozenArtifacts {
    documents: BTreeMap<String, serde_json::Value>,
    pub error: bool,
}
impl FrozenArtifacts {
    pub fn document(&self, filename: &str) -> Option<&serde_json::Value> {
        self.documents.get(filename)
    }
    #[cfg(test)]
    pub(crate) fn for_test(filename: &str, value: serde_json::Value) -> Self {
        Self {
            documents: [(filename.into(), value)].into_iter().collect(),
            error: false,
        }
    }
}

fn same_identity(a: &Observation, b: &Observation) -> bool {
    identities_match(a.identity, b.identity)
}
fn identities_match(a: Option<Identity>, b: Option<Identity>) -> bool {
    a.zip(b).is_some_and(|(a, b)| a == b)
}
fn unchanged(a: &Observation, b: &Observation) -> bool {
    same_identity(a, b)
        && a.metadata.len() == b.metadata.len()
        && a.metadata
            .modified()
            .ok()
            .zip(b.metadata.modified().ok())
            .is_some_and(|(a, b)| a == b)
}

impl ArtifactOwner {
    pub fn prepare(run_dir: &Path, specs: &[ArtifactSpec]) -> Result<Self, crate::Error> {
        let mut owner = Self::default();
        for spec in specs {
            assert!(!spec.namespace.contains(['/', '\\']) && !spec.filename.contains(['/', '\\']));
            fs::create_dir_all(run_dir).map_err(crate::io_error)?;
            let directory = run_dir.join(spec.namespace);
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(&directory).map_err(|_| {
                crate::Error::Run(
                    "cannot create fresh agent artifact namespace; existing paths were preserved"
                        .into(),
                )
            })?;
            let metadata =
                Observation::read(&open_regular_or_directory(&directory).map_err(crate::io_error)?)
                    .map_err(crate::io_error)?;
            owner.files.push(OwnedFile {
                path: directory.join(spec.filename),
                directory: metadata,
                observed: None,
            });
        }
        Ok(owner)
    }

    pub fn freeze(&mut self) -> FrozenArtifacts {
        if self.frozen {
            return FrozenArtifacts {
                error: true,
                ..Default::default()
            };
        }
        self.frozen = true;
        let mut frozen = FrozenArtifacts::default();
        for owned in &mut self.files {
            let result = (|| {
                let directory = Observation::read(&open_regular_or_directory(
                    owned.path.parent().expect("artifact directory"),
                )?)?;
                if !directory.metadata.is_dir()
                    || ((directory.identity.is_some() || owned.directory.identity.is_some())
                        && !same_identity(&directory, &owned.directory))
                {
                    return Err(std::io::Error::other("artifact namespace replaced"));
                }
                let file = match open_regular_or_directory(&owned.path) {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                    other => other?,
                };
                let before = Observation::read(&file)?;
                if !before.metadata.is_file() {
                    return Err(std::io::Error::other("artifact is not regular"));
                }
                owned.observed = Some(Observation::read(&file)?);
                if before.metadata.len() > LIMIT {
                    return Err(std::io::Error::other("artifact exceeds bound"));
                }
                let mut bytes = Vec::new();
                (&file).take(LIMIT + 1).read_to_end(&mut bytes)?;
                let after = Observation::read(&file)?;
                if bytes.len() as u64 > LIMIT
                    || before.metadata.len() != after.metadata.len()
                    || before.metadata.modified().ok() != after.metadata.modified().ok()
                    || ((before.identity.is_some() || after.identity.is_some())
                        && !same_identity(&before, &after))
                {
                    return Err(std::io::Error::other("artifact changed or exceeds bound"));
                }
                serde_json::from_slice(&bytes)
                    .map(Some)
                    .map_err(std::io::Error::other)
            })();
            match result {
                Ok(Some(document)) => {
                    frozen.documents.insert(
                        owned
                            .path
                            .file_name()
                            .expect("artifact filename")
                            .to_string_lossy()
                            .into_owned(),
                        document,
                    );
                }
                Ok(None) => (),
                Err(_) => frozen.error = true,
            }
        }
        frozen
    }

    /// Caller has frozen all evidence and completed independent grading/cleanup.
    pub fn settle(mut self) -> Result<(), String> {
        self.settle_inner()
    }

    fn settle_inner(&mut self) -> Result<(), String> {
        self.settled = true;
        let mut failed = false;
        for owned in &self.files {
            let result = (|| {
                let parent = owned.path.parent().expect("artifact directory");
                let directory = Observation::read(&open_regular_or_directory(parent)?)?;
                if !directory.metadata.is_dir() || !same_identity(&directory, &owned.directory) {
                    return Err(std::io::Error::other("artifact directory changed"));
                }
                match open_regular_or_directory(&owned.path)
                    .and_then(|file| Observation::read(&file))
                {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                    Ok(metadata)
                        if metadata.metadata.is_file()
                            && owned
                                .observed
                                .as_ref()
                                .map_or(!self.frozen, |before| unchanged(before, &metadata)) =>
                    {
                        fs::remove_file(&owned.path)?;
                    }
                    _ => return Err(std::io::Error::other("foreign artifact retained")),
                }
                fs::remove_dir(parent)
            })();
            failed |= result.is_err();
        }
        if failed {
            Err(
                if self
                    .files
                    .iter()
                    .any(|file| file.directory.identity.is_none())
                {
                    "agent scratch settlement unavailable without stable file identity; isolated artifacts retained".into()
                } else {
                    "agent scratch settlement failed; retained artifacts require inspection".into()
                },
            )
        } else {
            Ok(())
        }
    }
}
impl Drop for ArtifactOwner {
    fn drop(&mut self) {
        if !self.settled {
            // Fallback only; never print or retry a completed explicit settlement.
            let _ = self.settle_inner();
        }
    }
}

#[cfg(test)]
mod identity_tests {
    use super::*;
    #[test]
    fn only_two_matching_stable_identities_authorize_destructive_settlement() {
        assert!(identities_match(Some(Identity(1, 2)), Some(Identity(1, 2))));
        for (a, b) in [
            (None, None),
            (Some(Identity(1, 2)), None),
            (None, Some(Identity(1, 2))),
            (Some(Identity(1, 2)), Some(Identity(2, 2))),
            (Some(Identity(1, 2)), Some(Identity(1, 3))),
            (Some(Identity(1, 2)), Some(Identity(1, 2 + (1_u128 << 100)))),
        ] {
            assert!(!identities_match(a, b));
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    const SPEC: &[ArtifactSpec] = &[ArtifactSpec {
        namespace: "scratch",
        filename: "evidence.json",
    }];
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/agent-artifact-unit")
                .join(crate::workloads::TaskIdentity::local("artifact", None).attempt_id);
            fs::create_dir_all(&root).unwrap();
            Self(root)
        }
        fn file(&self) -> PathBuf {
            self.0.join("scratch/evidence.json")
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::set_permissions(self.0.join("scratch"), fs::Permissions::from_mode(0o700));
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn freeze_is_single_bounded_snapshot_and_settlement_preserves_replacements() {
        for replacement in ["file", "symlink", "same-inode", "new-after-absent"] {
            let fixture = Fixture::new();
            let mut owner = ArtifactOwner::prepare(&fixture.0, SPEC).unwrap();
            if replacement != "new-after-absent" {
                fs::write(fixture.file(), r#"{"answer":"A","usage":34}"#).unwrap();
            }
            let frozen = owner.freeze();
            assert!(!frozen.error);
            if replacement != "new-after-absent" {
                assert_eq!(frozen.document("evidence.json").unwrap()["answer"], "A");
            }
            match replacement {
                "file" => {
                    fs::rename(fixture.file(), fixture.0.join("original.saved")).unwrap();
                    fs::write(fixture.file(), "foreign replacement").unwrap();
                }
                "symlink" => {
                    fs::rename(fixture.file(), fixture.0.join("original.saved")).unwrap();
                    fs::write(fixture.0.join("foreign"), "foreign target").unwrap();
                    symlink(fixture.0.join("foreign"), fixture.file()).unwrap();
                }
                _ => fs::write(fixture.file(), "changed foreign bytes").unwrap(),
            }
            assert!(owner.freeze().error, "never reread current bytes");
            assert!(owner.settle().is_err());
            assert!(fs::symlink_metadata(fixture.file()).is_ok());
            if replacement != "new-after-absent" {
                assert_eq!(frozen.document("evidence.json").unwrap()["usage"], 34);
            }
        }
    }

    #[test]
    fn no_output_malformed_partial_oversized_and_symlink_have_no_stale_document() {
        for content in ["absent", "{", "[]", "large", "symlink"] {
            let fixture = Fixture::new();
            fs::write(fixture.0.join("legacy.json"), r#"{"answer":"old"}"#).unwrap();
            let mut owner = ArtifactOwner::prepare(&fixture.0, SPEC).unwrap();
            match content {
                "absent" => (),
                "large" => {
                    OpenOptions::new()
                        .create_new(true)
                        .write(true)
                        .open(fixture.file())
                        .unwrap()
                        .set_len(LIMIT + 1)
                        .unwrap();
                }
                "symlink" => symlink(fixture.0.join("legacy.json"), fixture.file()).unwrap(),
                _ => fs::write(fixture.file(), content).unwrap(),
            }
            let frozen = owner.freeze();
            assert_eq!(frozen.error, matches!(content, "{" | "large" | "symlink"));
            assert!(
                frozen
                    .document("evidence.json")
                    .is_none_or(|value| value.get("answer").is_none())
            );
            assert_eq!(owner.settle().is_err(), content == "symlink");
            assert_eq!(
                fs::read_to_string(fixture.0.join("legacy.json")).unwrap(),
                r#"{"answer":"old"}"#
            );
        }
    }

    #[test]
    fn namespace_collision_and_replacement_are_never_adopted_or_deleted() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.0.join("scratch")).unwrap();
        assert!(ArtifactOwner::prepare(&fixture.0, SPEC).is_err());
        fs::remove_dir(fixture.0.join("scratch")).unwrap();
        let mut owner = ArtifactOwner::prepare(&fixture.0, SPEC).unwrap();
        fs::rename(fixture.0.join("scratch"), fixture.0.join("owned.saved")).unwrap();
        fs::create_dir(fixture.0.join("scratch")).unwrap();
        fs::write(fixture.file(), "foreign bytes").unwrap();
        assert!(owner.freeze().error);
        assert!(owner.settle().is_err());
        assert_eq!(fs::read_to_string(fixture.file()).unwrap(), "foreign bytes");
    }

    #[test]
    fn explicit_failed_settlement_is_not_retried_and_unwind_cleans_owned_file() {
        let fixture = Fixture::new();
        let mut owner = ArtifactOwner::prepare(&fixture.0, SPEC).unwrap();
        fs::write(fixture.file(), "{}").unwrap();
        owner.freeze();
        fs::set_permissions(fixture.0.join("scratch"), fs::Permissions::from_mode(0o500)).unwrap();
        assert!(owner.settle().is_err());
        assert!(fixture.file().is_file());
        fs::set_permissions(fixture.0.join("scratch"), fs::Permissions::from_mode(0o700)).unwrap();
        fs::remove_file(fixture.file()).unwrap();
        fs::remove_dir(fixture.0.join("scratch")).unwrap();
        let result = std::panic::catch_unwind(|| {
            let _owner = ArtifactOwner::prepare(&fixture.0, SPEC).unwrap();
            fs::write(fixture.file(), "{}").unwrap();
            panic!("owned artifact unwind fixture");
        });
        assert!(result.is_err());
        assert!(!fixture.file().exists());
        assert!(!fixture.0.join("scratch").exists());
    }
}
