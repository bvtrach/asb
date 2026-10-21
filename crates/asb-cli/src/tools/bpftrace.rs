use super::{StaticTool, ToolKind};
use crate::Error;
use crate::install::{
    LoggedStatus, Recipe, command_output, host_executable, json_string, make_executable,
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const BPFTRACE_SOURCE: &str =
    "https://github.com/bpftrace/bpftrace/archive/refs/tags/v0.20.2.tar.gz";

pub(super) static ADAPTER: StaticTool = StaticTool::new(
    "bpftrace",
    Recipe::Custom {
        executable: "bpftrace",
        description: "copy the host-compatible bpftrace and record uname metadata",
        summary: "host-compatible bpftrace + kernel metadata",
        install: install_bpftrace,
    },
    ToolKind::Collector,
);

/// Copy the host bpftrace binary and persist the kernel pairing used to
/// install it.  bpftrace is kernel-sensitive; using the exact host binary and
/// recording `uname -r` prevents silently reusing a project tool from another
/// machine or kernel image.
fn install_bpftrace(
    root: &Path,
    progress: &mut dyn FnMut(u8, crate::install::ProgressStage),
) -> Result<PathBuf, Error> {
    progress(10, crate::install::ProgressStage::CheckingHost);
    let source = match host_executable("bpftrace") {
        Ok(source) => source,
        Err(host_error) => match build_bpftrace(root, progress) {
            Ok(source) => source,
            Err(build_error) => {
                return Err(Error::Io(format!(
                    "could not install bpftrace: {host_error}\n\n{build_error}\n\nTo install a matching host tool instead, run one of these commands:\n{}",
                    bpftrace_host_commands()
                )));
            }
        },
    };
    let version = command_output(&source, &["--version"], "bpftrace version")?;
    progress(95, crate::install::ProgressStage::Finalizing);
    let kernel = command_output(Path::new("uname"), &["-r"], "kernel release")?;
    let machine = command_output(Path::new("uname"), &["-m"], "kernel architecture")?;
    let bin = root.join("bin");
    fs::create_dir_all(&bin)
        .map_err(|error| Error::Io(format!("cannot create {}: {error}", bin.display())))?;
    let destination = bin.join("bpftrace");
    fs::copy(&source, &destination).map_err(|error| {
        Error::Io(format!(
            "cannot copy host bpftrace {} to {}: {error}",
            source.display(),
            destination.display()
        ))
    })?;
    make_executable(&destination)?;
    let metadata = format!(
        "{{\n  \"schema_version\": 1,\n  \"tool\": \"bpftrace\",\n  \"version\": {},\n  \"kernel_release\": {},\n  \"kernel_architecture\": {}\n}}\n",
        json_string(&version),
        json_string(&kernel),
        json_string(&machine)
    );
    fs::write(root.join("bpftrace-compatibility.json"), metadata).map_err(|error| {
        Error::Io(format!(
            "cannot write bpftrace compatibility metadata: {error}"
        ))
    })?;
    progress(100, crate::install::ProgressStage::Ready);
    Ok(destination)
}

fn build_bpftrace(
    root: &Path,
    progress: &mut dyn FnMut(u8, crate::install::ProgressStage),
) -> Result<PathBuf, Error> {
    let source_dir = root.join("source");
    let build_dir = root.join("build");
    let archive = root.join(".bpftrace-source.tar.gz");
    if !source_dir.is_dir() {
        fs::create_dir_all(root)
            .map_err(|error| Error::Io(format!("cannot create {}: {error}", root.display())))?;
        progress(10, crate::install::ProgressStage::Downloading);
        let status = Command::new("curl")
            .args([
                "--fail",
                "--location",
                "--silent",
                "--show-error",
                "--output",
            ])
            .arg(&archive)
            .arg(BPFTRACE_SOURCE)
            .logged_status()
            .map_err(|error| bpftrace_missing_dependency("curl", error))?;
        if !status.success() {
            return Err(Error::Io(format!(
                "could not download pinned bpftrace source from {BPFTRACE_SOURCE}; verify network access and retry\n\n{}",
                bpftrace_source_commands()
            )));
        }
        progress(25, crate::install::ProgressStage::Extracting);
        fs::create_dir_all(&source_dir).map_err(|error| {
            Error::Io(format!("cannot create bpftrace source directory: {error}"))
        })?;
        let status = Command::new("tar")
            .args(["-xzf"])
            .arg(&archive)
            .args(["-C"])
            .arg(&source_dir)
            .args(["--strip-components=1"])
            .logged_status()
            .map_err(|error| bpftrace_missing_dependency("tar", error))?;
        if !status.success() {
            return Err(Error::Io(format!(
                "could not extract the pinned bpftrace source; install tar and retry\n\n{}",
                bpftrace_dependency_commands("tar")
            )));
        }
    }
    progress(40, crate::install::ProgressStage::Configuring);
    let status = Command::new("cmake")
        .args(["-S"])
        .arg(&source_dir)
        .args(["-B"])
        .arg(&build_dir)
        .args(["-DBUILD_TESTING=OFF", "-DBPFTRACE_BUILD_TESTING=OFF"])
        .logged_status()
        .map_err(|error| bpftrace_missing_dependency("cmake", error))?;
    if !status.success() {
        return Err(Error::Io(format!(
            "cmake could not configure bpftrace; the BCC, libbpf, libclang, compiler, or CMake development packages are missing. Install them and rerun `asb tool install bpftrace`:\n\n{}",
            bpftrace_source_commands()
        )));
    }
    progress(60, crate::install::ProgressStage::Building);
    let status = Command::new("cmake")
        .args(["--build"])
        .arg(&build_dir)
        .args(["--target", "bpftrace", "--parallel"])
        .logged_status()
        .map_err(|error| bpftrace_missing_dependency("cmake", error))?;
    if !status.success() {
        return Err(Error::Io(format!(
            "bpftrace source compilation failed; install the compiler and BCC/libbpf/libclang development packages, then rerun `asb tool install bpftrace`:\n\n{}",
            bpftrace_source_commands()
        )));
    }
    progress(90, crate::install::ProgressStage::Validating);
    find_built_bpftrace(&build_dir).ok_or_else(|| {
        Error::Io(format!(
            "bpftrace built without a discoverable executable under {}",
            build_dir.display()
        ))
    })
}

fn find_built_bpftrace(directory: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(directory).ok()?.filter_map(Result::ok);
    for entry in entries {
        let path = entry.path();
        if path.file_name().and_then(|name| name.to_str()) == Some("bpftrace") && path.is_file() {
            return Some(path);
        }
        if path.is_dir()
            && path.file_name().and_then(|name| name.to_str()) != Some("CMakeFiles")
            && let Some(found) = find_built_bpftrace(&path)
        {
            return Some(found);
        }
    }
    None
}

pub(crate) fn bpftrace_host_commands() -> &'static str {
    "Red Hat/Fedora:\n  sudo dnf install -y bpftrace\nDebian/Ubuntu:\n  sudo apt-get update && sudo apt-get install -y bpftrace\nopenEuler:\n  sudo dnf install -y bpftrace"
}

pub(crate) fn bpftrace_source_commands() -> &'static str {
    "Red Hat/Fedora:\n  sudo dnf install -y bcc-devel libbpf-devel libclang-devel clang llvm cmake make gcc-c++ curl tar\nDebian/Ubuntu:\n  sudo apt-get update && sudo apt-get install -y libbpfcc-dev libbpf-dev libclang-dev clang llvm cmake build-essential curl tar\nopenEuler:\n  sudo dnf install -y bcc-devel libbpf-devel clang llvm cmake make gcc-c++ curl tar"
}

fn bpftrace_dependency_commands(dependency: &str) -> &'static str {
    match dependency {
        "curl" => {
            "Red Hat/Fedora:\n  sudo dnf install -y curl\nDebian/Ubuntu:\n  sudo apt-get update && sudo apt-get install -y curl\nopenEuler:\n  sudo dnf install -y curl"
        }
        "tar" => {
            "Red Hat/Fedora:\n  sudo dnf install -y tar\nDebian/Ubuntu:\n  sudo apt-get update && sudo apt-get install -y tar\nopenEuler:\n  sudo dnf install -y tar"
        }
        "cmake" => {
            "Red Hat/Fedora:\n  sudo dnf install -y cmake make gcc-c++\nDebian/Ubuntu:\n  sudo apt-get update && sudo apt-get install -y cmake build-essential\nopenEuler:\n  sudo dnf install -y cmake make gcc-c++"
        }
        _ => bpftrace_source_commands(),
    }
}

pub(crate) fn bpftrace_missing_dependency(dependency: &str, error: std::io::Error) -> Error {
    Error::Io(format!(
        "bpftrace source build needs `{dependency}`, but it could not be started: {error}. Install it, then rerun `asb tool install bpftrace`:\n\n{}",
        bpftrace_dependency_commands(dependency)
    ))
}

#[cfg(test)]
#[path = "../../tests/unit/bpftrace_install.rs"]
mod coverage_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prerequisite_diagnostics_name_the_failure_and_supported_distribution_repairs() {
        for dependency in ["curl", "tar", "cmake", "unknown-build-dependency"] {
            let commands = bpftrace_dependency_commands(dependency);
            for distribution in ["Red Hat/Fedora:", "Debian/Ubuntu:", "openEuler:"] {
                assert!(commands.contains(distribution));
            }
            if dependency != "unknown-build-dependency" {
                assert!(commands.contains(dependency));
            } else {
                assert_eq!(commands, bpftrace_source_commands());
            }
            let error = bpftrace_missing_dependency(
                dependency,
                std::io::Error::new(std::io::ErrorKind::NotFound, "fixture tool is missing"),
            );
            let message = error.to_string();
            assert!(message.contains(&format!("`{dependency}`")));
            assert!(message.contains("fixture tool is missing"));
            assert!(message.contains("asb tool install bpftrace"));
            assert!(message.contains(commands));
        }
        assert!(bpftrace_host_commands().contains("apt-get install -y bpftrace"));
        assert!(bpftrace_source_commands().contains("libbpf-dev"));
        assert!(bpftrace_source_commands().contains("libclang-dev"));
    }

    #[test]
    fn executable_discovery_skips_cmake_internal_outputs_and_searches_nested_targets() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/bpftrace-build-fixtures")
            .join(format!(
                "{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        assert_eq!(find_built_bpftrace(&root), None);
        fs::create_dir_all(root.join("CMakeFiles")).unwrap();
        fs::write(root.join("CMakeFiles/bpftrace"), "not a published target").unwrap();
        fs::write(root.join("README"), "not a binary").unwrap();
        assert_eq!(find_built_bpftrace(&root), None);
        // A directory with the target's name must not be returned as a binary.
        fs::create_dir_all(root.join("obj/bpftrace")).unwrap();
        assert_eq!(find_built_bpftrace(&root), None);
        fs::create_dir_all(root.join("src/release")).unwrap();
        let expected = root.join("src/release/bpftrace");
        fs::write(&expected, "fixture executable").unwrap();
        assert_eq!(find_built_bpftrace(&root), Some(expected));
        fs::remove_dir_all(root).unwrap();
    }
}
