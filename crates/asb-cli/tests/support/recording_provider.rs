use serde_json::json;

/// Bounded loopback responses, never a live provider or credential. The owner
/// joins the listener on every assertion/error path, including command failure.
pub(crate) struct RecordingProvider {
    pub(crate) endpoint: String,
    pub(crate) calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl RecordingProvider {
    pub(crate) fn new() -> Self {
        use std::io::{Read, Write};
        use std::sync::{
            Arc,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        };
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let calls = Arc::new(AtomicUsize::new(0));
        let (ending, observed) = (stop.clone(), calls.clone());
        let worker = std::thread::spawn(move || {
            while !ending.load(Ordering::Acquire) {
                let (mut stream, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => panic!("fixture listener: {error}"),
                };
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut request = Vec::new();
                let mut buffer = [0; 4096];
                let body_start = loop {
                    let count = stream.read(&mut buffer).unwrap();
                    assert!(count > 0, "fixture upload ended before headers");
                    request.extend_from_slice(&buffer[..count]);
                    assert!(request.len() <= 64 * 1024);
                    if let Some(index) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                        break index + 4;
                    }
                };
                let headers = String::from_utf8(request[..body_start].to_vec()).unwrap();
                let inference = headers.starts_with("POST /v1/chat/completions ");
                assert!(
                    inference
                        || headers.starts_with("GET /models ")
                        || headers.starts_with("GET /v1/models "),
                    "unexpected fixture route: {:?}",
                    headers.lines().next()
                );
                let length: usize = headers
                    .lines()
                    .find_map(|line| {
                        line.split_once(':')
                            .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                            .map(|(_, length)| length.trim().parse().unwrap())
                    })
                    .unwrap_or(0);
                assert!(length <= 64 * 1024);
                while request.len() < body_start + length {
                    let count = stream.read(&mut buffer).unwrap();
                    assert!(count > 0);
                    request.extend_from_slice(&buffer[..count]);
                }
                if inference {
                    observed.fetch_add(1, Ordering::SeqCst);
                }
                let body = if inference {
                    json!({"usage":{"prompt_tokens":2,"completion_tokens":3,"total_tokens":5},"recorded_track": observed.load(Ordering::SeqCst)}).to_string()
                } else {
                    r#"{"data":[{"id":"fixture","supported_parameters":["tools"]}]}"#.to_owned()
                };
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            }
        });
        Self {
            endpoint,
            calls,
            stop,
            worker: Some(worker),
        }
    }
}
impl Drop for RecordingProvider {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Release);
        let result = self.worker.take().unwrap().join();
        if !std::thread::panicking() {
            result.unwrap();
        }
    }
}
