//! Reuse explicitly validated outcomes, never infer completion from exit zero.
use crate::{Error, io_error};
use serde_json::Value;
use std::io::Write;

pub(crate) fn render_reused(
    summaries: &[Value],
    json: bool,
    output: &mut dyn Write,
) -> Result<(), Error> {
    let mut failed = 0;
    let mut unknown = 0;
    let mut unresolved_grades = 0;
    for summary in summaries {
        if json {
            let mut summary = summary.clone();
            summary["reused"] = Value::Bool(true);
            serde_json::to_writer(&mut *output, &summary)
                .map_err(|error| Error::Io(error.to_string()))?;
            writeln!(output).map_err(io_error)?;
        } else {
            let task = summary["task"]["task_id"].as_str().unwrap_or("task");
            let safe: String = task.chars().filter(|value| !value.is_control()).collect();
            writeln!(
                output,
                "Reused {}task {safe}: {}.",
                if summary["recovered_grade_only"] == true {
                    "official grade only (execution unavailable) for "
                } else if summary["reconciled"] == true {
                    "reconciled retained outcome for "
                } else {
                    "completed "
                },
                if summary["workload_success"] == true {
                    "resolved"
                } else {
                    "unresolved"
                }
            )
            .map_err(io_error)?;
        }
        if summary["recovered_grade_only"] == true && summary["process_success"].is_null() {
            unknown += 1;
            unresolved_grades += usize::from(summary["workload_success"] == false);
        } else if summary["process_success"] != true
            || summary["workload_success"] != true
            || !summary["failure_reason"].is_null()
        {
            failed += 1;
        }
    }
    if unknown > 0 {
        return Err(Error::Run(format!(
            "{unknown} reused official grade(s) have unknown execution status ({unresolved_grades} unresolved); grades were preserved without rerunning clients; {failed} other reused outcome(s) remain failed"
        )));
    }
    if failed > 0 {
        Err(Error::Run(format!(
            "{failed} reused benchmark outcome(s) remain failed; inspect their preserved measurement and evaluator records"
        )))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn grade_only_reuse_reports_false_grades_and_never_invents_process_success() {
        let summaries = [false, true].map(|resolved| {
            json!({
                "task":{"task_id":"retained-task"}, "workload_success":resolved,
                "process_success":null, "recovered_grade_only":true, "reconciled":true
            })
        });
        for json in [false, true] {
            let mut output = Vec::new();
            let error = render_reused(&summaries, json, &mut output)
                .unwrap_err()
                .to_string();
            assert!(error.contains(
                "2 reused official grade(s) have unknown execution status (1 unresolved)"
            ));
            let text = String::from_utf8(output).unwrap();
            assert_eq!(text.lines().count(), 2);
            if json {
                for line in text.lines() {
                    let value: Value = serde_json::from_str(line).unwrap();
                    assert!(value["process_success"].is_null());
                    assert_eq!(value["reused"], true);
                }
            } else {
                assert!(text.contains("execution unavailable"));
                assert!(text.contains("unresolved"));
            }
        }
    }

    #[test]
    fn reused_false_grades_and_failed_clients_remain_failures_in_jsonl() {
        let summaries = vec![
            json!({"task":{"task_id":"unresolved"},"workload_success":false,"process_success":true,"token_usage":{"total_tokens":42}}),
            json!({"task":{"task_id":"resolved-partial-work"},"workload_success":true,"process_success":false}),
            json!({"task":{"task_id":"resolved"},"workload_success":true,"process_success":true}),
            json!({"task":{"task_id":"reconciled-unresolved"},"workload_success":false,"process_success":true,"reconciled":true}),
        ];
        let before = summaries.clone();
        let mut output = Vec::new();
        let error = render_reused(&summaries, true, &mut output).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("3 reused benchmark outcome(s) remain failed")
        );
        let records: Vec<Value> = String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(records.len(), 4);
        assert_eq!(records[3]["workload_success"], false);
        assert_eq!(records[3]["reconciled"], true);
        assert!(records.iter().all(|record| record["reused"] == true));
        assert_eq!(records[0]["workload_success"], false);
        assert_eq!(records[0]["token_usage"]["total_tokens"], 42);
        assert_eq!(
            summaries, before,
            "presentation cannot rewrite historical grades"
        );
    }

    #[test]
    fn reused_human_output_is_one_line_per_attempt_and_output_failure_propagates() {
        let summary = json!({"task":{"task_id":"task\n\u{1b}[31m"},"workload_success":true,"process_success":true});
        let mut output = Vec::new();
        render_reused(std::slice::from_ref(&summary), false, &mut output).unwrap();
        let text = String::from_utf8(output).unwrap();
        assert_eq!(text.lines().count(), 1);
        assert!(!text.contains('\u{1b}'));
        struct FailedOutput;
        impl Write for FailedOutput {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("fixture output failure"))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        assert!(render_reused(&[summary], true, &mut FailedOutput).is_err());
    }
}
