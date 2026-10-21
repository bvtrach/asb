//! Explicit model-session teardown against only loopback catalog fixtures.
use super::*;
use std::cell::Cell;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::rc::Rc;
use std::time::{Duration, Instant};

struct Session {
    endpoint: String,
    calls: Rc<Cell<usize>>,
    fail: bool,
    finished: bool,
}
impl providers::ProviderSession for Session {
    fn api_key(&self) -> &str {
        "fixture-only-key"
    }
    fn base_url(&self) -> &str {
        &self.endpoint
    }
    fn finish(&mut self) -> Result<(), Error> {
        assert!(!self.finished);
        self.finished = true;
        self.calls.set(self.calls.get() + 1);
        if self.fail {
            Err(Error::Run("fixture catalog cleanup uncertain".into()))
        } else {
            Ok(())
        }
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        assert!(self.finished, "model session skipped explicit finish");
    }
}

#[test]
fn fetch_parse_and_cleanup_failures_preserve_the_primary_catalog_outcome() {
    for (body, status, cleanup_fail) in [
        (r#"{"data":[{"id":"fixture-model"}]}"#, 200, false),
        (r#"{"data":[{"id":"fixture-model"}]}"#, 200, true),
        ("invalid fixture JSON", 200, true),
        ("fixture catalog refused", 503, true),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => panic!("fixture catalog accept: {error}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            let mut chunk = [0; 1024];
            while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                let count = stream.read(&mut chunk).unwrap();
                assert!(count > 0 && request.len() + count < 8192);
                request.extend_from_slice(&chunk[..count]);
            }
            assert!(
                String::from_utf8(request)
                    .unwrap()
                    .contains("Bearer fixture-only-key")
            );
            write!(stream, "HTTP/1.1 {status} fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        });
        let calls = Rc::new(Cell::new(0));
        let session = Box::new(Session {
            endpoint: format!("http://{address}/v1"),
            calls: calls.clone(),
            fail: cleanup_fail,
            finished: false,
        });
        let result = discover_session_models(
            providers::model_spec("cli2key").unwrap(),
            "cli2key",
            false,
            session,
        );
        server.join().unwrap();
        assert_eq!(calls.get(), 1);
        if !cleanup_fail {
            assert_eq!(result.unwrap()[0].id, "fixture-model");
            continue;
        }
        let error = result.unwrap_err().to_string();
        assert!(
            error.contains("fixture catalog cleanup uncertain"),
            "{error}"
        );
        assert!(!error.contains("fixture-only-key"));
        if body.starts_with("invalid") || status != 200 {
            assert!(
                !error.starts_with("fixture catalog cleanup uncertain"),
                "{error}"
            );
            assert!(error.contains("; provider cleanup failed:"), "{error}");
        }
    }
}
