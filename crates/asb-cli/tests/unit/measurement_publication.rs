use super::*;
use serde::ser::{Error as _, SerializeMap};

#[test]
fn partial_measurement_serialization_does_not_clobber_previous_file() {
    struct Broken;
    impl Serialize for Broken {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let mut map = serializer.serialize_map(Some(2))?;
            map.serialize_entry("partial", &true)?;
            Err(S::Error::custom("deliberate encoding failure"))
        }
    }
    let root = std::env::temp_dir().join(format!(
        "asb-measurement-publish-{}",
        crate::tools::runtime::unique_id()
    ));
    fs::create_dir(&root).unwrap();
    let path = root.join("run.json");
    let old = b"{\"official_grade\":false}";
    fs::write(&path, old).unwrap();
    let result = write_json(&path, &Broken);
    let bytes = fs::read(&path).unwrap();
    fs::remove_dir_all(root).unwrap();
    assert!(result.is_err());
    assert_eq!(
        bytes, old,
        "failed serialization clobbered prior measurement"
    );
}

#[test]
fn malformed_summary_history_is_preserved_not_reset() {
    let root = std::env::temp_dir().join(format!(
        "asb-summary-publish-{}",
        crate::tools::runtime::unique_id()
    ));
    fs::create_dir(&root).unwrap();
    let path = root.join("summary.json");
    let old = b"{\"runs\":[{\"official_grade\":true}], broken history";
    fs::write(&path, old).unwrap();
    let result = update_summary_value(&root, serde_json::json!({"workload_success":false}));
    let bytes = fs::read(&path).unwrap();
    fs::remove_dir_all(root).unwrap();
    assert!(
        result.is_err(),
        "malformed history was silently accepted and replaced"
    );
    assert_eq!(bytes, old);
}

#[test]
fn summary_transaction_retains_unknown_fields_and_rejects_invalid_runs() {
    let root = std::env::temp_dir().join(format!(
        "asb-summary-history-{}",
        crate::tools::runtime::unique_id()
    ));
    fs::create_dir(&root).unwrap();
    let path = root.join("summary.json");
    let old = serde_json::json!({"schema_version":2,"future":{"retained":17},"runs":[{"future_kind":"extension","native_reward":0.75,"resolved":null}]});
    fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
    update_summary_value(&root, serde_json::json!({"workload_success":false,"benchmark_outcome":{"evaluation":{"outcome":"pending"}}})).unwrap();
    let updated: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(updated["future"], old["future"]);
    assert_eq!(updated["runs"][0], old["runs"][0]);
    assert_eq!(updated["runs"].as_array().unwrap().len(), 2);
    for old in [
        b"{}".as_slice(),
        b"{\"runs\":null}".as_slice(),
        b"{\"runs\":[1]}".as_slice(),
        b"[]".as_slice(),
    ] {
        fs::write(&path, old).unwrap();
        assert!(update_summary_value(&root, serde_json::json!({})).is_err());
        assert_eq!(fs::read(&path).unwrap(), old);
    }
    fs::remove_dir_all(root).unwrap();
}
