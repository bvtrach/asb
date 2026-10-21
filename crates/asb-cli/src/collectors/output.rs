//! Complete measurement input is separate from bounded diagnostic capture.

use super::ProcessData;
use std::io::{Read, Write};
use std::path::Path;

const MAX_INPUT_BYTES: u64 = 16 * 1024 * 1024;
// Leave 4 MiB beneath comparison's 16 MiB record bound: two 256 KiB raw
// diagnostic prefixes can each expand SIX-fold during JSON escaping, plus
// ample space for outer fields. Count without another serialized buffer.
const MAX_PROCESSED_BYTES: usize = 12 * 1024 * 1024;

pub(crate) fn read_complete_output(path: &Path) -> Result<String, String> {
    let file = std::fs::File::open(path)
        .map_err(|error| format!("cannot read complete collector output: {error}"))?;
    let mut bytes = Vec::new();
    file.take(MAX_INPUT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read complete collector output: {error}"))?;
    if bytes.len() as u64 > MAX_INPUT_BYTES {
        return Err(
            "complete collector output exceeds 16 MiB; no partial measurements published".into(),
        );
    }
    if !bytes.is_empty() && !bytes.ends_with(b"\n") {
        return Err(
            "collector output has an incomplete final line; no partial measurements published"
                .into(),
        );
    }
    String::from_utf8(bytes)
        .map_err(|_| "collector output is not UTF-8; no partial measurements published".into())
}

struct Budget(usize);

impl Write for Budget {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self.0.checked_sub(bytes.len()).ok_or_else(|| {
            std::io::Error::other("processed collector data exceeds its output budget")
        })?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub(crate) fn validate_processed_output(data: &ProcessData) -> Result<(), String> {
    if data
        .metrics
        .values()
        .chain(data.series.values().flatten())
        .chain(data.timestamps.values().flatten())
        .any(|value| !value.is_finite())
    {
        return Err("non-finite collector measurement; no partial measurements published".into());
    }
    validate_with_budget(data, MAX_PROCESSED_BYTES)
}

fn validate_with_budget(data: &ProcessData, bytes: usize) -> Result<(), String> {
    serde_json::to_writer_pretty(Budget(bytes), data).map_err(|_| {
        "processed collector data exceeds its output budget; no partial measurements published"
            .into()
    })
}

#[cfg(test)]
pub(crate) fn process_output_fixture(
    adapter: &dyn super::CollectorAdapter,
    name: &str,
    full: &str,
) -> Result<ProcessData, String> {
    let root = std::env::temp_dir()
        .join(crate::workloads::TaskIdentity::local("collector-full-output", None).attempt_id);
    std::fs::create_dir(&root).unwrap();
    let stdout_path = root.join("stdout");
    let stderr_path = root.join("stderr");
    std::fs::write(&stdout_path, full).unwrap();
    std::fs::write(&stderr_path, "").unwrap();
    let status = std::process::Command::new("true").status().unwrap();
    let mut session =
        crate::measurements::Session::start_with_collectors(&root, &[], None).unwrap();
    let result = adapter.process(
        &mut session,
        &super::ProcessContext {
            name,
            status: &status,
            stdout: "diagnostic prefix is not measurement input",
            stderr: "",
            stdout_path: &stdout_path,
            stderr_path: &stderr_path,
        },
    );
    drop(session);
    std::fs::remove_dir_all(&root).unwrap();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_output_bounds_fail_instead_of_silently_returning_a_prefix() {
        let root = std::env::temp_dir()
            .join(crate::workloads::TaskIdentity::local("complete-output", None).attempt_id);
        std::fs::create_dir(&root).unwrap();
        let path = root.join("output");
        assert!(read_complete_output(&path).is_err());
        assert!(read_complete_output(&root).is_err());
        std::fs::write(&path, b"final row without newline").unwrap();
        assert!(
            read_complete_output(&path)
                .unwrap_err()
                .contains("incomplete")
        );
        std::fs::write(&path, [0xff, b'\n']).unwrap();
        assert!(read_complete_output(&path).unwrap_err().contains("UTF-8"));
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(MAX_INPUT_BYTES + 1).unwrap();
        assert!(read_complete_output(&path).unwrap_err().contains("16 MiB"));
        std::fs::write(&path, b"").unwrap();
        assert_eq!(read_complete_output(&path).unwrap(), "");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn processed_data_size_is_checked_without_materializing_another_json_buffer() {
        let mut data = ProcessData {
            status: "completed".into(),
            metrics: Default::default(),
            series: Default::default(),
            timestamps: Default::default(),
            artifact: None,
        };
        assert!(validate_processed_output(&data).is_ok());
        assert!(validate_with_budget(&data, 1).is_err());
        data.metrics.insert("invalid".into(), f64::INFINITY);
        assert!(validate_processed_output(&data).is_err());
        data.metrics.clear();
        data.series.insert("invalid".into(), vec![f64::NAN]);
        assert!(validate_processed_output(&data).is_err());
        data.series.clear();
        data.timestamps
            .insert("invalid".into(), vec![f64::NEG_INFINITY]);
        assert!(validate_processed_output(&data).is_err());
        let mut budget = Budget(2);
        assert_eq!(budget.write(b"ok").unwrap(), 2);
        assert!(budget.write(b"x").is_err());
        budget.flush().unwrap();
    }

    #[test]
    fn worst_case_escaped_diagnostics_fit_with_room_for_outer_record_fields() {
        let data = ProcessData {
            status: "x".repeat(MAX_PROCESSED_BYTES - 1024),
            metrics: Default::default(),
            series: Default::default(),
            timestamps: Default::default(),
            artifact: None,
        };
        validate_processed_output(&data).unwrap();
        let diagnostic = "\0".repeat(256 * 1024);
        let mut final_budget = Budget(16 * 1024 * 1024);
        serde_json::to_writer_pretty(&mut final_budget, &(&data, &diagnostic, &diagnostic))
            .unwrap();
        // Even maximally escaped prefixes leave over 1 MiB for the bounded
        // collector name and other outer fields, not merely an assumed 512 KiB.
        assert!(final_budget.0 >= 1024 * 1024);
    }
}
