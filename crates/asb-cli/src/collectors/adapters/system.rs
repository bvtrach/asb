use super::super::StaticCollector;
use crate::measurements::{self, Session};
use serde::Serialize;

pub(super) static ADAPTER: StaticCollector =
    StaticCollector::new("system", None, None, false).with_start(start);

#[derive(Serialize)]
struct SystemInfo {
    schema_version: u32,
    collected_unix_ms: u128,
    os: &'static str,
    architecture: &'static str,
    logical_cpus: usize,
    kernel_release: Option<String>,
    memory_total_kib: Option<u64>,
}

fn start(session: &mut Session) -> Result<(), String> {
    let info = SystemInfo {
        schema_version: 1,
        collected_unix_ms: session.started_unix_ms(),
        os: std::env::consts::OS,
        architecture: std::env::consts::ARCH,
        logical_cpus: std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get),
        kernel_release: read_trimmed("/proc/sys/kernel/osrelease"),
        memory_total_kib: read_trimmed("/proc/meminfo").and_then(|text| {
            text.lines().find_map(|line| {
                line.strip_prefix("MemTotal:")?
                    .split_whitespace()
                    .next()?
                    .parse()
                    .ok()
            })
        }),
    };
    measurements::write_json(&session.dir.join("system.json"), &info)
}

fn read_trimmed(path: &str) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}
