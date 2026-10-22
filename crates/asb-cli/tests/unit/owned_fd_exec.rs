//! Actual same-PID utility execution, exact matches and descriptor reuse.
use super::*;
use std::fs::File;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::MetadataExt;
use std::process::Stdio;

fn actual() -> std::path::PathBuf {
    std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("asb")
}
fn identity(fd: i32, kind: &str) -> serde_json::Value {
    let metadata = std::fs::metadata(format!("/proc/self/fd/{fd}")).unwrap();
    serde_json::json!({"fd":fd,"device":metadata.dev().to_string(),"inode":metadata.ino().to_string(),"kind":kind})
}
fn inherit(command: &mut Command, files: Vec<File>) {
    // SAFETY: captured File guards own every named descriptor until spawn.
    // Only child-table fcntl calls run after fork; no foreign FD is touched.
    unsafe {
        command.pre_exec(move || {
            for file in &files {
                let fd = file.as_raw_fd();
                let flags = libc::fcntl(fd, libc::F_GETFD);
                if flags < 0 || libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
                    return Err(io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
}
fn command(claims: &serde_json::Value) -> Command {
    let mut command = Command::new(actual());
    command.env_clear().args([
        OsString::from("__asb-owned-fd-exec"),
        claims.to_string().into(),
        "--".into(),
    ]);
    command
}

#[test]
fn actual_exec_sanitizes_only_matching_objects_and_preserves_raw_execution_context() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/owned-fd-exec")
        .join(crate::workloads::TaskIdentity::local("context", None).attempt_id);
    std::fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let file = File::create(root.join("owned")).unwrap();
    let fd = file.as_raw_fd();
    let claim = identity(fd, "file");
    let mut command = command(&serde_json::json!([claim.clone()]));
    let python = crate::tools::host_executable(std::path::Path::new("python3")).unwrap();
    command.arg(python).args(["-c",r#"import os,sys,json
fd=int(sys.argv[1]);expected=(int(sys.argv[2]),int(sys.argv[3]))
try:
 stat=os.fstat(fd);present=(stat.st_dev,stat.st_ino)==expected
except OSError: present=False
print(json.dumps({'pid':os.getpid(),'cwd':os.getcwd(),'env':os.environ['FIXTURE'],'stdin':sys.stdin.buffer.read().hex(),'raw':os.fsencode(sys.argv[4]).hex(),'owned_present':present}))
"#]).args([fd.to_string(),claim["device"].as_str().unwrap().into(),claim["inode"].as_str().unwrap().into()])
        .arg(OsString::from_vec(vec![b'x',0xff,b'y']))
        .current_dir(&root).env("FIXTURE","preserved")
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    inherit(&mut command, vec![file]);
    let mut child = command.spawn().unwrap();
    let pid = child.id();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&[0, 255, 42])
        .unwrap();
    drop(command);
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["pid"], pid);
    assert_eq!(result["cwd"], root.to_str().unwrap());
    assert_eq!(result["env"], "preserved");
    assert_eq!(result["stdin"], "00ff2a");
    assert_eq!(result["raw"], "78ff79");
    assert_eq!(result["owned_present"], false);
}

#[test]
fn exact_socket_matches_close_but_missing_and_reused_foreign_descriptors_survive() {
    use std::net::{TcpListener, TcpStream};
    let sentinel = TcpListener::bind("127.0.0.1:0").unwrap();
    let sentinel_address = sentinel.local_addr().unwrap();
    let python = crate::tools::host_executable(std::path::Path::new("python3")).unwrap();
    for mode in [
        "socket-match",
        "already-cloexec",
        "missing",
        "reused",
        "kind-mismatch",
    ] {
        let socket = TcpListener::bind("127.0.0.1:0").unwrap();
        let socket_fd = socket.as_raw_fd();
        let socket_claim = identity(socket_fd, "socket");
        let foreign = File::open("/dev/null").unwrap();
        let foreign_fd = foreign.as_raw_fd();
        let mut claim = if mode == "kind-mismatch" {
            identity(foreign_fd, "socket")
        } else {
            socket_claim.clone()
        };
        if mode == "missing" {
            claim["fd"] = serde_json::json!(999999);
        }
        if mode == "reused" {
            claim["fd"] = serde_json::json!(foreign_fd);
        }
        let probe_fd = if matches!(mode, "missing" | "reused" | "kind-mismatch") {
            foreign_fd
        } else {
            socket_fd
        };
        let mut command = command(&serde_json::json!([claim]));
        command.arg(&python).args(["-c","import os,sys;fd=int(sys.argv[1]);\ntry: os.fstat(fd);print('open')\nexcept OSError: print('closed')\n",&probe_fd.to_string()]);
        let expected = match mode {
            "socket-match" => {
                inherit(
                    &mut command,
                    vec![File::from(std::os::fd::OwnedFd::from(socket))],
                );
                "closed"
            }
            "already-cloexec" => "closed",
            _ => {
                inherit(&mut command, vec![foreign]);
                "open"
            }
        };
        let output = command.output().unwrap();
        drop(command);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            expected,
            "{mode}"
        );
        assert!(TcpStream::connect(sentinel_address).is_ok());
    }
}

#[test]
fn malformed_controls_are_bounded_redacted_and_full_width_values_do_not_round() {
    let large = serde_json::json!([{"fd":42,"device":"18446744073709551615","inode":"9007199254740993","kind":"socket"}]);
    let parsed = claims(std::ffi::OsStr::new(&large.to_string())).unwrap();
    assert_eq!(parsed[0].1, u64::MAX);
    assert_eq!(parsed[0].2, 9007199254740993);
    for input in [
        "inert-secret-malformed".to_owned(),
        "[]".into(),
        "x".repeat(8193),
        large.to_string().replace("42", "2"),
        large
            .to_string()
            .replace("9007199254740993", "18446744073709551616"),
        large.to_string().replace("9007199254740993", "+1"),
        large.to_string().replace("socket", "extension"),
        serde_json::json!([large[0], large[0]]).to_string(),
        serde_json::json!([large[0], large[0], large[0], large[0], large[0]]).to_string(),
    ] {
        let output = Command::new(actual())
            .env_clear()
            .args([
                "__asb-owned-fd-exec",
                &input,
                "--",
                "/bin/echo",
                "must-not-execute-secret",
            ])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert_eq!(
            output.stderr,
            b"asb: owned descriptor exec handoff failed\n"
        );
    }
}
