//! Exact private proxy protocol, separate from upstream key parsing.
use super::*;

fn address() -> SocketAddrV4 {
    SocketAddrV4::new(Ipv4Addr::LOCALHOST, 12345)
}
fn line(port: u16) -> String {
    format!(
        "ASB cli2key proxy ready {{\"schema_version\":1,\"host\":\"127.0.0.1\",\"port\":{port}}}\n"
    )
}

#[test]
fn proxy_ready_is_exact_bounded_and_matches_the_owned_address() {
    for bad in [
        "ASB cli2key proxy ready\n".into(),
        "ASB cli2key proxy ready {}\n".into(),
        "ASB cli2key proxy ready {broken}\n".into(),
        line(0),
        line(12346),
        line(12345).replace("12345", "65536"),
        line(12345).replace("12345", "-1"),
        line(12345).replace("127.0.0.1", "0.0.0.0"),
        line(12345).replace("127.0.0.1", "localhost"),
        line(12345).replace("schema_version\":1", "schema_version\":2"),
        line(12345).replace("12345", "\"12345\""),
        line(12345).replace('}', ",\"extra\":true}"),
        line(12345).replace("}\n", "} trailing\n"),
        format!("ASB cli2key proxy ready {}\n", "x".repeat(MAX_LINE)),
        "x".repeat(MAX_BYTES + 1),
        "ordinary noise\n".repeat(MAX_LINES),
    ] {
        let watch = Watch::new(address());
        let mut parser = Parser::new(watch.clone());
        for chunk in bad.as_bytes().chunks(7) {
            parser.accept(chunk);
        }
        assert!(watch.check().is_err(), "accepted invalid ready data");
        assert!(parser.line.len() <= MAX_LINE);
    }
    let watch = Watch::new(address());
    let mut parser = Parser::new(watch.clone());
    parser.accept(b"unrelated banner\n");
    for chunk in line(12345).as_bytes().chunks(1) {
        parser.accept(chunk);
    }
    assert_eq!(watch.check().unwrap(), Some(address()));
    parser.end();
    assert!(watch.ended());
}

#[test]
fn proxy_duplicate_and_late_malformed_facts_are_sticky_not_hidden_by_first_ready() {
    for later in [
        line(12345),
        line(12346),
        "ASB cli2key proxy ready garbage\n".into(),
    ] {
        for same_turn in [true, false] {
            let watch = Watch::new(address());
            let mut parser = Parser::new(watch.clone());
            if same_turn {
                parser.accept(format!("{}{later}", line(12345)).as_bytes());
            } else {
                parser.accept(line(12345).as_bytes());
                assert_eq!(watch.check().unwrap(), Some(address()));
                parser.accept(later.as_bytes());
            }
            assert!(watch.check().is_err());
            parser.accept(line(12345).as_bytes());
            assert!(watch.check().is_err());
        }
    }
    let watch = Watch::new(address());
    let mut parser = Parser::new(watch.clone());
    parser.accept(line(12345).as_bytes());
    parser.accept(b"ASB cli2key proxy ready unfinished");
    parser.end();
    assert!(watch.check().is_err());
}

#[test]
fn arbitrary_later_output_remains_bounded_discard_not_a_new_failure_policy() {
    let watch = Watch::new(address());
    let mut parser = Parser::new(watch.clone());
    parser.accept(line(12345).as_bytes());
    for _ in 0..100 {
        parser.accept(&[b'x'; 8192]);
    }
    parser.accept(b"\nordinary output\n");
    assert_eq!(watch.check().unwrap(), Some(address()));
    assert!(parser.line.len() <= MAX_LINE);
    parser.end();
    assert!(watch.check().is_ok());
}

#[test]
fn listener_cloexec_stays_parent_local_and_unrelated_children_cannot_keep_it() {
    use std::os::unix::fs::MetadataExt;
    use std::time::{Duration, Instant};

    let sentinel = TcpListener::bind("127.0.0.1:0").unwrap();
    let sentinel_address = sentinel.local_addr().unwrap();
    let reservation = Reservation::new().unwrap();
    let identity = reservation.socket_identity();
    let fd = reservation.listener.as_raw_fd();
    // SAFETY: read-only observation of our live owned descriptor.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    assert!(flags >= 0 && flags & libc::FD_CLOEXEC != 0);
    let target = std::fs::read_link(format!("/proc/self/fd/{fd}")).unwrap();
    let mut admitted = Command::new("/bin/true");
    reservation.configure(&mut admitted);
    let python = crate::tools::host_executable(std::path::Path::new("python3")).unwrap();
    std::thread::scope(|scope| {
        let probe=scope.spawn(|| {
            for _ in 0..8 {
                let output=Command::new(&python).env_clear().args(["-c", "import os\nfor entry in os.listdir('/proc/self/fd'):\n try: print(os.readlink('/proc/self/fd/'+entry))\n except FileNotFoundError: pass\n"]).output().unwrap();
                assert!(output.status.success());
                assert!(!String::from_utf8_lossy(&output.stdout).lines().any(|line|line==target.to_string_lossy()));
            }
        });
        assert!(admitted.status().unwrap().success());
        probe.join().unwrap();
    });
    assert_eq!(unsafe { libc::fcntl(fd, libc::F_GETFD) }, flags);
    drop(admitted); // Its pre_exec closure owns a real parent duplicate.
    drop(reservation);
    for entry in std::fs::read_dir("/proc/self/fd").unwrap() {
        if let Ok(metadata) = std::fs::metadata(entry.unwrap().path()) {
            assert_ne!(
                (metadata.dev(), metadata.ino()),
                identity,
                "original socket retained in parent FD custody"
            );
        }
    }
    // Other parallel actors may briefly inherit a CLOEXEC copy before exec.
    // Numeric port reuse cannot establish which socket is still owned.
    let end = Instant::now() + Duration::from_secs(3);
    let inode = identity.1.to_string();
    loop {
        let present = std::fs::read_to_string("/proc/net/tcp")
            .unwrap()
            .lines()
            .any(|line| line.split_whitespace().nth(9) == Some(inode.as_str()));
        if !present {
            break;
        }
        assert!(
            Instant::now() < end,
            "original listening socket inode {} remained after cleanup",
            identity.1
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        std::net::TcpStream::connect(sentinel_address).is_ok(),
        "unrelated sentinel listener was closed"
    );
}
