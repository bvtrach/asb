use super::*;

#[test]
fn environment_preservation_keeps_previous_backups_and_dangling_links() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/python-preservation-fixtures")
        .join(crate::workloads::TaskIdentity::local("fixture", None).attempt_id);
    fs::create_dir_all(&root).unwrap();
    let environment = root.join("venv");
    assert!(preserve_environment(&environment).unwrap().is_none());
    fs::create_dir(root.join("venv.asb-backup-1")).unwrap();
    fs::write(root.join("venv.asb-backup-1/keep"), "previous manual work").unwrap();
    fs::create_dir(&environment).unwrap();
    fs::write(environment.join("keep"), "current manual work").unwrap();
    let backup = preserve_environment(&environment).unwrap().unwrap();
    assert_eq!(backup, root.join("venv.asb-backup-2/venv"));
    assert_eq!(
        fs::read_to_string(backup.join("keep")).unwrap(),
        "current manual work"
    );
    assert_eq!(
        fs::read_to_string(root.join("venv.asb-backup-1/keep")).unwrap(),
        "previous manual work"
    );
    assert!(!environment.exists());
    #[cfg(unix)]
    {
        let absent = root.join("never-created-target");
        std::os::unix::fs::symlink(&absent, &environment).unwrap();
        let backup = preserve_environment(&environment).unwrap().unwrap();
        assert_eq!(fs::read_link(backup).unwrap(), absent);
        assert!(!absent.exists());
    }
    fs::write(root.join("blocked"), "not a directory").unwrap();
    let error = preserve_environment(&root.join("blocked/venv"))
        .err()
        .unwrap()
        .to_string();
    assert!(
        error.contains("cannot inspect") && error.contains("blocked/venv"),
        "{error}"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn noninterpreters_and_missing_programs_are_not_accepted_as_python() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/python-probe-fixtures")
        .join(crate::workloads::TaskIdentity::local("fixture", None).attempt_id);
    fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let context = OperationContext::new(&root, &project::ProjectConfig::default()).unwrap();
    assert_eq!(context.root(), root);
    for executable in [
        "/bin/true",
        "/bin/false",
        "/bin/echo",
        "/asb-fixture-missing-python",
    ] {
        assert!(
            version(&context, Path::new(executable)).is_none(),
            "{executable}"
        );
        assert!(!matches_project(
            &context,
            Path::new(executable),
            Path::new("/bin/true")
        ));
    }
    #[cfg(target_os = "linux")]
    {
        let logs = fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().path().join("output.log"))
            .collect::<Vec<_>>();
        assert_eq!(logs.len(), 6, "all six diagnostics belong to this fixture");
        assert!(logs.iter().all(|path| path.is_file()));
        // Ancillary log settlement does not synchronously join its writer.
        // Observe the exact fixture bytes, not an assumed flush timing.
        let echo = b"-I -S -c import sys; print('%d.%d' % sys.version_info[:2])\n";
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            let contents = logs
                .iter()
                .map(|path| fs::read(path).unwrap())
                .collect::<Vec<_>>();
            assert!(
                contents.iter().all(|bytes| echo.starts_with(bytes)),
                "unexpected probe diagnostic; retained {}",
                root.display()
            );
            if contents
                .iter()
                .filter(|bytes| bytes.as_slice() == echo)
                .count()
                == 2
                && contents.iter().filter(|bytes| bytes.is_empty()).count() == 4
            {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "two exact echo diagnostics were not flushed within 3s; retained {}",
                root.display()
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    fs::remove_dir_all(root).unwrap();
}
