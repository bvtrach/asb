//! Private content binding primitives for sweep continuation.
use crate::Error;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const HELPER: &str = include_str!("../../assets/benchmarks/asb_fingerprint.py");
mod completed;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Root {
    pub name: String,
    pub path: PathBuf,
    #[serde(default = "location_bound")]
    pub bind_location: bool,
    #[serde(default)]
    pub direct_file: bool,
}

fn location_bound() -> bool {
    true
}

impl Root {
    pub(super) fn path(name: impl Into<String>, path: PathBuf) -> Self {
        Self {
            name: name.into(),
            path,
            bind_location: true,
            direct_file: false,
        }
    }
    pub(super) fn executable() -> Result<Self, Error> {
        #[cfg(target_os = "linux")]
        let path = PathBuf::from(format!("/proc/{}/exe", std::process::id()));
        #[cfg(not(target_os = "linux"))]
        let path = std::env::current_exe().map_err(crate::io_error)?;
        Ok(Self {
            name: "asb-executable".into(),
            path,
            bind_location: false,
            direct_file: true,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Fingerprint {
    pub schema_version: u32,
    pub digest: String,
    pub bytes: u64,
    pub nodes: u64,
}

impl Fingerprint {
    fn valid(&self) -> bool {
        self.schema_version == 1
            && self.digest.len() == 64
            && self.digest.bytes().all(|byte| byte.is_ascii_hexdigit())
            && self.bytes <= 64 * 1024u64.pow(3)
            && self.nodes <= 300000
    }
}

// Deliberately not Debug/Serialize: the private key must not enter diagnostics.
pub(super) struct Snapshotter {
    key: [u8; 32],
    python: PathBuf,
}

impl Snapshotter {
    pub(super) fn open(directory: &Path, python: PathBuf) -> Result<Self, Error> {
        if let Ok(metadata) = std::fs::symlink_metadata(directory)
            && !metadata.is_dir()
        {
            return Err(Error::Input(
                "checkpoint directory must be a real directory, not a symlink or file".into(),
            ));
        }
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(directory).map_err(crate::io_error)?;
        let key_path = directory.join("key");
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&key_path) {
            Ok(mut file) => {
                let mut key = [0; 32];
                fill_random(&mut key)?;
                file.write_all(&key)
                    .and_then(|()| file.sync_all())
                    .map_err(crate::io_error)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(error) => return Err(crate::io_error(error)),
        }
        let metadata = std::fs::symlink_metadata(&key_path).map_err(crate::io_error)?;
        if !metadata.is_file() || metadata.len() != 32 {
            return Err(Error::Input("checkpoint key is missing or invalid; preserve the old results and start a fresh sweep".into()));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err(Error::Input("checkpoint key permissions are not private; protect its existing key with mode 0600 before continuing".into()));
            }
        }
        let mut key = [0; 32];
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        options
            .open(&key_path)
            .and_then(|mut file| file.read_exact(&mut key))
            .map_err(crate::io_error)?;
        Ok(Self { key, python })
    }

    pub(super) fn fingerprint(
        &self,
        roots: &[Root],
        context: &Value,
        environment: bool,
        exclude: &[&str],
    ) -> Result<Fingerprint, Error> {
        let key: String = self.key.iter().map(|byte| format!("{byte:02x}")).collect();
        let request = serde_json::to_vec(&json!({"key":key,"roots":roots,"context":context,"environment":environment,"exclude":exclude}))
            .map_err(|_| Error::Input("cannot encode private checkpoint inputs".into()))?;
        if request.len() > 16 * 1024 * 1024 {
            return Err(Error::Input(
                "checkpoint input description exceeds 16 MiB; select a smaller batch".into(),
            ));
        }
        let mut child = Command::new(&self.python)
            .args(["-I", "-S", "-B", "-c", HELPER])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| {
                Error::Input("checkpoint hashing needs a usable local Python 3 interpreter".into())
            })?;
        let send = child
            .stdin
            .take()
            .expect("private helper stdin")
            .write_all(&request);
        if send.is_err() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::Io(
                "cannot send private checkpoint input description".into(),
            ));
        }
        let deadline = Instant::now() + Duration::from_secs(130);
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(Error::Run(
                        "checkpoint hashing exceeded its deadline or helper wait failed".into(),
                    ));
                }
            }
        }
        let output = child
            .wait_with_output()
            .map_err(|_| Error::Io("cannot collect checkpoint digest".into()))?;
        if output.stdout.len() > 2048 || !output.status.success() {
            return Err(Error::Run("checkpoint inputs could not be sealed (missing, changing, unsupported or excessive input); no input values were logged".into()));
        }
        let fingerprint: Fingerprint = serde_json::from_slice(&output.stdout)
            .map_err(|_| Error::Run("checkpoint helper returned an invalid digest".into()))?;
        if !fingerprint.valid() {
            return Err(Error::Run(
                "checkpoint helper returned an invalid or unbounded digest".into(),
            ));
        }
        Ok(fingerprint)
    }
}

pub(super) struct Inputs {
    directory: PathBuf,
    snapshotter: Snapshotter,
    roots: Vec<Root>,
    context: Value,
    initial: Fingerprint,
}

impl Inputs {
    pub(super) fn begin(
        parent: &Path,
        results: &Path,
        plan: &super::plan::SweepPlan,
    ) -> Result<Self, Error> {
        let directory = results.join(".checkpoint");
        let mut roots = vec![
            Root::executable()?,
            Root::path("selection-plan", results.join("plan.json")),
        ];
        let mut projects = BTreeSet::new();
        let mut python = PathBuf::from("python3");
        for entry in &plan.entries {
            let (root, _) = crate::project::load(&parent.join(&entry.reference))?;
            projects.insert(root.canonicalize().map_err(crate::io_error)?);
        }
        for (index, root) in projects.iter().enumerate() {
            let (_, config) = crate::project::load(root)?;
            let candidate = crate::python_runtime::project_python(root);
            if candidate.is_file() {
                python = candidate;
            }
            for (name, path) in [
                ("configuration", root.join(".asb/project.toml")),
                ("tools", crate::project::resolve(root, &config.tools_dir)),
                (
                    "workloads",
                    std::env::var_os("ASB_WORKLOAD_DIR")
                        .map(PathBuf::from)
                        .unwrap_or_else(|| crate::project::resolve(root, &config.workloads_dir)),
                ),
                ("python", root.join(".asb/python")),
            ] {
                roots.push(Root::path(format!("project-{index}-{name}"), path));
            }
        }
        let mut environment_files = BTreeSet::new();
        for (_, value) in std::env::vars_os() {
            let path = PathBuf::from(value);
            if path.is_file() {
                // Keep the declared symlink in the input graph. Canonicalizing
                // it here would miss retargeting to different content while
                // the previous target stayed unchanged.
                environment_files.insert(std::path::absolute(path).map_err(crate::io_error)?);
            }
        }
        for (index, path) in environment_files.into_iter().enumerate() {
            roots.push(Root::path(
                format!("explicit-environment-file-{index}"),
                path,
            ));
        }
        let snapshotter = Snapshotter::open(&directory, python)?;
        let context = json!({"protocol":"asb-input-seal-v1","scope":"declared-project-inputs-and-process-environment"});
        let initial = snapshotter.fingerprint(&roots, &context, true, &[])?;
        let state = Self {
            directory,
            snapshotter,
            roots,
            context,
            initial,
        };
        state.publish("pending", None)?;
        Ok(state)
    }

    pub(super) fn finish(self) -> Result<(), Error> {
        match self
            .snapshotter
            .fingerprint(&self.roots, &self.context, true, &[])
        {
            Ok(after) => self.publish(
                if after.digest == self.initial.digest {
                    "stable"
                } else {
                    "changed"
                },
                Some(&after),
            ),
            Err(error) => {
                let _ = self.publish("unavailable", None);
                Err(error)
            }
        }
    }

    fn publish(&self, status: &str, after: Option<&Fingerprint>) -> Result<(), Error> {
        let content = serde_json::to_vec_pretty(&json!({"schema_version":1,"status":status,
            "scope":"input-stability-only-not-a-resume-receipt", "roots":self.roots,
            "initial":self.initial,"final":after}))
        .map_err(|_| Error::Io("cannot encode private input seal".into()))?;
        let temporary = self.directory.join(format!(
            ".state-{}",
            crate::workloads::TaskIdentity::local("input-seal", None).attempt_id
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary).map_err(crate::io_error)?;
        file.write_all(&content)
            .and_then(|()| file.sync_all())
            .and_then(|()| std::fs::rename(&temporary, self.directory.join("inputs.json")))
            .map_err(crate::io_error)
    }
}

#[cfg(unix)]
fn fill_random(bytes: &mut [u8]) -> Result<(), Error> {
    File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(bytes))
        .map_err(|_| Error::Io("cannot generate a private checkpoint key".into()))
}

#[cfg(not(unix))]
fn fill_random(_: &mut [u8]) -> Result<(), Error> {
    Err(Error::Input(
        "secure checkpoint key generation is unavailable on this host".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let directory = std::env::temp_dir()
                .join(crate::workloads::TaskIdentity::local("checkpoint", None).attempt_id);
            std::fs::create_dir_all(&directory).unwrap();
            Self(directory)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn keyed_content_binding_is_private_stable_and_sensitive_to_input_changes() {
        let directory = Directory::new();
        let file = directory.0.join("input");
        std::fs::write(&file, "one").unwrap();
        let state = directory.0.join("state");
        let snapshotter = Snapshotter::open(&state, "python3".into()).unwrap();
        let roots = [Root::path("runtime", file.clone())];
        let context = json!({"model":"logical-model","private-test-value":"never expose this"});
        let original = snapshotter
            .fingerprint(&roots, &context, false, &[])
            .unwrap();
        assert!(original.valid());
        assert!(
            !serde_json::to_string(&original)
                .unwrap()
                .contains("never expose")
        );
        let reopened = Snapshotter::open(&state, "python3".into()).unwrap();
        assert_eq!(
            original.digest,
            reopened
                .fingerprint(&roots, &context, false, &[])
                .unwrap()
                .digest
        );
        std::fs::write(file, "two").unwrap();
        assert_ne!(
            original.digest,
            reopened
                .fingerprint(&roots, &context, false, &[])
                .unwrap()
                .digest
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(state.join("key"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
            assert_eq!(
                std::fs::metadata(&state).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
    }

    #[test]
    fn malformed_existing_keys_are_preserved_instead_of_regenerated() {
        let directory = Directory::new();
        std::fs::write(directory.0.join("key"), "existing malformed key").unwrap();
        assert!(Snapshotter::open(&directory.0, "python3".into()).is_err());
        assert_eq!(
            std::fs::read_to_string(directory.0.join("key")).unwrap(),
            "existing malformed key"
        );
    }

    #[test]
    fn input_stability_records_distinguish_stable_changed_and_unavailable() {
        for (action, status) in [
            ("none", "stable"),
            ("change", "changed"),
            ("fifo", "unavailable"),
        ] {
            let directory = Directory::new();
            let input = directory.0.join("input");
            std::fs::write(&input, "before").unwrap();
            let state = directory.0.join("state");
            let snapshotter = Snapshotter::open(&state, "python3".into()).unwrap();
            let roots = vec![Root::path("declared-input", input.clone())];
            let context = json!({"scope":"unit-fixture"});
            let initial = snapshotter
                .fingerprint(&roots, &context, true, &[])
                .unwrap();
            let inputs = Inputs {
                directory: state.clone(),
                snapshotter,
                roots,
                context,
                initial,
            };
            inputs.publish("pending", None).unwrap();
            if action == "change" {
                std::fs::write(&input, "after").unwrap();
            }
            #[cfg(unix)]
            if action == "fifo" {
                std::fs::remove_file(&input).unwrap();
                // The FIFO is owned by this fixture; hashing must reject it
                // without opening/blocking on a writer.
                assert!(
                    Command::new("python3")
                        .args([
                            "-I",
                            "-S",
                            "-B",
                            "-c",
                            "import os,sys; os.mkfifo(sys.argv[1], 0o600)"
                        ])
                        .arg(&input)
                        .status()
                        .unwrap()
                        .success()
                );
            }
            #[cfg(not(unix))]
            if action == "fifo" {
                continue;
            }
            assert_eq!(inputs.finish().is_ok(), action != "fifo");
            let record: Value =
                serde_json::from_slice(&std::fs::read(state.join("inputs.json")).unwrap()).unwrap();
            assert_eq!(record["status"], status);
            assert_eq!(record["scope"], "input-stability-only-not-a-resume-receipt");
            if action == "none" {
                assert_eq!(record["initial"]["digest"], record["final"]["digest"]);
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_checkpoint_directory_and_nonprivate_key_are_not_accepted() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let directory = Directory::new();
        let target = directory.0.join("target");
        std::fs::create_dir(&target).unwrap();
        let link = directory.0.join("redirect");
        symlink(&target, &link).unwrap();
        assert!(Snapshotter::open(&link, "python3".into()).is_err());
        assert!(!target.join("key").exists());
        Snapshotter::open(&target, "python3".into()).unwrap();
        let key = std::fs::read(target.join("key")).unwrap();
        std::fs::set_permissions(target.join("key"), std::fs::Permissions::from_mode(0o644))
            .unwrap();
        assert!(Snapshotter::open(&target, "python3".into()).is_err());
        assert_eq!(std::fs::read(target.join("key")).unwrap(), key);
    }
}
