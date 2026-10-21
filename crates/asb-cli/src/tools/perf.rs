use super::{StaticTool, ToolKind};
use crate::Error;
use crate::install::{
    LoggedStatus, Recipe, command_output, install_verified_source, make_executable,
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

// Kernel.org's standalone perf source release. The digest is pinned from its
// published sha256sums.asc; no kernel build or host package installation is required.
const SOURCE: &str =
    "https://cdn.kernel.org/pub/linux/kernel/tools/perf/v6.12.0/perf-6.12.0.tar.gz";
const SOURCE_SHA256: &str = "e6c5bbaa1d7a80d8bb4ebd43aa15f2027b308a4cd2bdac9ba303e0e65f17081a";

pub(super) static ADAPTER: StaticTool = StaticTool::new(
    "perf",
    Recipe::Custom {
        executable: "perf",
        description: "install a distribution perf package or verified upstream source build",
        summary: "project-local Linux perf",
        install: install_perf,
    },
    ToolKind::Collector,
);

fn install_perf(root: &Path, progress: &mut dyn FnMut(u8, &str)) -> Result<PathBuf, Error> {
    if !cfg!(target_os = "linux") {
        return Err(Error::Input("perf requires Linux".into()));
    }
    fs::create_dir_all(root)
        .map_err(|e| Error::Io(format!("cannot create {}: {e}", root.display())))?;
    let stage = root.join(format!(
        ".install-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir(&stage)
        .map_err(|e| Error::Io(format!("cannot create {}: {e}", stage.display())))?;
    let result = (|| {
        progress(20, "obtaining distribution perf");
        let source = match distribution_perf(&stage) {
            Ok(path) => path,
            Err(package_error) => {
                progress(35, "building upstream perf");
                build_perf(&stage, progress).map_err(|build_error| Error::Io(format!("could not obtain a working distribution perf: {package_error}\nCould not build upstream perf: {build_error}\nInstall the compiler, make, flex, bison, libelf and libssl development packages, curl, tar and sha256sum, then retry `asb tool install perf`. See .asb/install.log for build details.")))?
            }
        };
        let bin = root.join("bin");
        fs::create_dir_all(&bin)
            .map_err(|e| Error::Io(format!("cannot create {}: {e}", bin.display())))?;
        let staged_binary = bin.join(format!(
            ".perf-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        if let Err(error) = fs::copy(&source, &staged_binary) {
            let _ = fs::remove_file(&staged_binary);
            return Err(Error::Io(format!(
                "cannot copy perf from {}: {error}",
                source.display()
            )));
        }
        let publish = (|| {
            make_executable(&staged_binary)?;
            if !runnable(&staged_binary) {
                return Err(Error::Io("the project-local perf executable could not run; check its runtime library dependencies".into()));
            }
            let destination = bin.join("perf");
            fs::rename(&staged_binary, &destination).map_err(|e| {
                Error::Io(format!(
                    "cannot install perf at {}: {e}",
                    destination.display()
                ))
            })?;
            Ok(destination)
        })();
        if publish.is_err() {
            let _ = fs::remove_file(staged_binary);
        }
        publish
    })();
    let _ = fs::remove_dir_all(&stage);
    result
}

fn runnable(path: &Path) -> bool {
    Command::new(path)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

// APT validates downloads against its configured, authenticated package indexes.
// Ubuntu's kernel-specific package is often a metapackage; its dependency holds
// the executable. Debian instead publishes linux-perf directly.
fn distribution_perf(stage: &Path) -> Result<PathBuf, String> {
    let kernel =
        command_output(Path::new("uname"), &["-r"], "kernel release").map_err(|e| e.to_string())?;
    if !kernel
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b".-_+".contains(&b))
    {
        return Err("invalid kernel release".into());
    }
    let meta = format!("linux-tools-{kernel}");
    let mut packages = vec![meta.clone()];
    if let Ok(output) = Command::new("apt-cache").args(["depends", &meta]).output()
        && output.status.success()
    {
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if let Some(name) = line.trim().strip_prefix("Depends: ")
                && name.starts_with("linux-")
                && name.contains("tools")
                && name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b".-+".contains(&b))
            {
                packages.insert(0, name.to_string());
            }
        }
    }
    packages.push("linux-perf".into());
    for (index, package) in packages.iter().enumerate() {
        let dir = stage.join(format!("package-{index}"));
        fs::create_dir(&dir).map_err(|e| e.to_string())?;
        let status = Command::new("apt-get")
            .args(["download", package])
            .current_dir(&dir)
            .logged_status();
        if !status.is_ok_and(|s| s.success()) {
            continue;
        }
        let archive = fs::read_dir(&dir)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|e| e == "deb"));
        let Some(archive) = archive else { continue };
        let extracted = dir.join("extracted");
        let status = Command::new("dpkg-deb")
            .arg("-x")
            .arg(archive)
            .arg(&extracted)
            .logged_status();
        if !status.is_ok_and(|s| s.success()) {
            continue;
        }
        if let Some(found) = find_perf(&extracted) {
            return Ok(found);
        }
    }
    Err("no runnable perf was found in the configured APT repositories".into())
}

fn find_perf(dir: &Path) -> Option<PathBuf> {
    for entry in fs::read_dir(dir).ok()?.filter_map(Result::ok) {
        let path = entry.path();
        let ty = entry.file_type().ok()?;
        if ty.is_file() && entry.file_name() == "perf" && runnable(&path) {
            return Some(path);
        }
        if ty.is_dir()
            && let Some(found) = find_perf(&path)
        {
            return Some(found);
        }
    }
    None
}

fn build_perf(stage: &Path, progress: &mut dyn FnMut(u8, &str)) -> Result<PathBuf, Error> {
    let archive = stage.join("perf.tar.gz");
    install_verified_source(
        SOURCE,
        &archive,
        "Linux perf source",
        SOURCE_SHA256,
        false,
        |_| Ok(()),
    )?;
    let source = stage.join("source");
    fs::create_dir(&source)
        .map_err(|e| Error::Io(format!("cannot create perf source directory: {e}")))?;
    let status = Command::new("tar")
        .arg("-xzf")
        .arg(&archive)
        .arg("-C")
        .arg(&source)
        .arg("--strip-components=1")
        .logged_status()
        .map_err(|e| Error::Io(format!("cannot extract perf source: {e}")))?;
    if !status.success() {
        return Err(Error::Io("could not extract perf source".into()));
    }
    progress(50, "compiling Linux perf");
    let mut make = Command::new("make");
    make.arg("-C")
        .arg(source.join("tools/perf"))
        .arg("JOBS=2")
        .arg("NO_GTK2=1")
        .arg("NO_PERL=1")
        .arg("NO_LIBPYTHON=1")
        .arg("PYTHON=python3")
        .arg("PYTHON_CONFIG=true")
        .arg("NO_LIBAUDIT=1")
        .arg("NO_LIBBABELTRACE=1")
        .arg("NO_LIBNUMA=1")
        .arg("NO_SLANG=1");
    if !Command::new("pkg-config")
        .args(["--exists", "libtraceevent"])
        .status()
        .is_ok_and(|s| s.success())
    {
        make.arg("NO_LIBTRACEEVENT=1");
    }
    let status = make
        .logged_status()
        .map_err(|e| Error::Io(format!("cannot start perf build: {e}")))?;
    if !status.success() {
        return Err(Error::Io("perf compilation failed".into()));
    }
    let binary = source.join("tools/perf/perf");
    if !runnable(&binary) {
        return Err(Error::Io("built perf is not runnable".into()));
    }
    Ok(binary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "downloads a distribution package from configured repositories"]
    fn live_distribution_package_is_runnable() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(format!("asb-perf-package-test-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let result = distribution_perf(&root);
        assert!(
            result.as_ref().is_ok_and(|path| runnable(path)),
            "{result:?}"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "downloads and compiles upstream perf"]
    fn live_upstream_source_build_is_runnable() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(format!("asb-perf-source-test-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let result = build_perf(&root, &mut |_, _| {});
        assert!(
            result.as_ref().is_ok_and(|path| runnable(path)),
            "{result:?}"
        );
        fs::remove_dir_all(root).unwrap();
    }
}
