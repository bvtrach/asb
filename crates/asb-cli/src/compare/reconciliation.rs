//! Informational receipt provenance; comparison is not a historical attestor.
use super::*;

pub(super) fn recorded(
    results: &Path,
    directory: &Path,
    index: usize,
    replica: usize,
    total: usize,
    task: Option<&crate::workloads::TaskIdentity>,
) -> Result<bool, Error> {
    let path = results
        .join(".checkpoint/replicas")
        .join(format!("{index:06}"))
        .join(format!("{replica:03}.json"));
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(Error::Input(format!(
                "cannot inspect receipt provenance: {error}"
            )));
        }
        Ok(metadata) if !metadata.is_file() || metadata.len() > 1024 * 1024 => {
            return Err(Error::Input(
                "receipt provenance is not a bounded regular file".into(),
            ));
        }
        Ok(_) => (),
    }
    let receipt = read_json(&path)?;
    if receipt["reconciled"] != true {
        return Ok(false);
    }
    if receipt["schema_version"] != 1
        || receipt["scope"] != "completed-replica-declared-evidence"
        || receipt["configuration"].as_u64() != Some(index as u64)
        || receipt["replica"] != json!({"index":replica,"total":total})
    {
        return Err(Error::Input(
            "reconciled receipt provenance does not match its replica".into(),
        ));
    }
    let outcome = receipt["outcome"]
        .as_str()
        .map(Path::new)
        .ok_or_else(|| Error::Input("reconciled receipt has no outcome location".into()))?;
    let expected = std::path::absolute(directory)
        .map_err(|error| Error::Input(error.to_string()))?
        .join(".asb-completions");
    if outcome.parent() != Some(expected.as_path()) {
        return Err(Error::Input(
            "reconciled receipt points outside its result directory".into(),
        ));
    }
    let record = read_json(outcome)?;
    if task.is_none() || record["summary"]["task"] != json!(task) {
        return Err(Error::Input(
            "reconciled receipt refers to another task attempt".into(),
        ));
    }
    Ok(true)
}
