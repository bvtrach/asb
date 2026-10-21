//! Optional principal facts, never guardian status or continuation permission.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "Wire", into = "Wire")]
pub(crate) enum ProcessTermination {
    Code(i32),
    Signal(i32),
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Wire {
    Code { code: i32 },
    Signal { signal: i32 },
}

impl TryFrom<Wire> for ProcessTermination {
    type Error = &'static str;
    fn try_from(value: Wire) -> Result<Self, Self::Error> {
        match value {
            Wire::Code { code } => Ok(Self::Code(code)),
            Wire::Signal { signal } if (1..=127).contains(&signal) => Ok(Self::Signal(signal)),
            _ => Err("invalid principal signal"),
        }
    }
}
impl From<ProcessTermination> for Wire {
    fn from(value: ProcessTermination) -> Self {
        match value {
            ProcessTermination::Code(code) => Self::Code { code },
            ProcessTermination::Signal(signal) => Self::Signal { signal },
        }
    }
}
impl ProcessTermination {
    pub(crate) fn observed(status: std::process::ExitStatus) -> Option<Self> {
        if let Some(code) = status.code() {
            return Some(Self::Code(code));
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            status
                .signal()
                .filter(|signal| (1..=127).contains(signal))
                .map(Self::Signal)
        }
        #[cfg(not(unix))]
        None
    }

    /// Only the existing Unix control wire needs an OS status bridge.
    #[cfg(target_os = "linux")]
    pub(crate) fn status(self) -> std::process::ExitStatus {
        use std::os::unix::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(match self {
            Self::Code(code) => code << 8,
            Self::Signal(signal) => signal,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Diagnostic {
    Malformed,
    Unavailable,
}

/// This diagnostic must never enter status-changing AttemptIssues.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
pub(crate) struct Observation {
    pub process_termination: Option<ProcessTermination>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_termination_diagnostic: Option<Diagnostic>,
}

impl Observation {
    pub(crate) fn from_record(value: &serde_json::Value) -> Self {
        serde_json::from_value(serde_json::json!({
            "process_termination":value.get("process_termination"),
            "process_termination_diagnostic":value.get("process_termination_diagnostic"),
        }))
        .unwrap_or_else(|_| Self::malformed())
    }
    pub(crate) fn description(self) -> String {
        match self.process_termination {
            Some(ProcessTermination::Code(code)) => format!("principal exit code {code}"),
            Some(ProcessTermination::Signal(signal)) => format!("principal signal {signal}"),
            None => match self.process_termination_diagnostic {
                Some(Diagnostic::Malformed) => {
                    "principal termination unknown: optional observation malformed or incomplete"
                        .into()
                }
                Some(Diagnostic::Unavailable) => {
                    "principal termination unknown: optional observation unavailable".into()
                }
                None => "principal termination unknown (legacy or unobserved)".into(),
            },
        }
    }
    pub(crate) fn observed(status: std::process::ExitStatus) -> Self {
        Self {
            process_termination: ProcessTermination::observed(status),
            ..Self::default()
        }
    }
    pub(crate) fn malformed() -> Self {
        Self {
            process_termination: None,
            process_termination_diagnostic: Some(Diagnostic::Malformed),
        }
    }
}

impl<'de> Deserialize<'de> for Observation {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Fields {
            #[serde(default)]
            process_termination: Option<serde_json::Value>,
            #[serde(default)]
            process_termination_diagnostic: Option<serde_json::Value>,
        }
        let fields = Fields::deserialize(deserializer)?;
        let exact = fields
            .process_termination
            .map(serde_json::from_value)
            .transpose();
        let diagnostic = fields
            .process_termination_diagnostic
            .map(serde_json::from_value)
            .transpose();
        Ok(match (exact, diagnostic) {
            (Ok(process_termination), Ok(process_termination_diagnostic))
                if process_termination.is_none() || process_termination_diagnostic.is_none() =>
            {
                Self {
                    process_termination,
                    process_termination_diagnostic,
                }
            }
            _ => Self::malformed(),
        })
    }
}

/// Persisted malformed facts reject report updates; absence is never a conflict.
pub(crate) fn recorded(
    value: &serde_json::Value,
) -> Result<Option<ProcessTermination>, &'static str> {
    let exact = value
        .get("process_termination")
        .filter(|value| !value.is_null())
        .map(|value| {
            serde_json::from_value(value.clone())
                .map_err(|_| "malformed process termination evidence")
        })
        .transpose()?;
    let diagnostic: Option<Diagnostic> = value
        .get("process_termination_diagnostic")
        .filter(|value| !value.is_null())
        .map(|value| serde_json::from_value(value.clone()))
        .transpose()
        .map_err(|_| "malformed process termination diagnostic")?;
    if exact.is_some() && diagnostic.is_some() {
        return Err("known process termination has an unknown-observation diagnostic");
    }
    Ok(exact)
}

pub(crate) fn agree(
    left: &serde_json::Value,
    right: &serde_json::Value,
) -> Result<(), &'static str> {
    match (recorded(left)?, recorded(right)?) {
        (Some(left), Some(right)) if left != right => {
            Err("conflicting exact process termination evidence")
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_width_codes_signals_and_legacy_are_distinct() {
        for code in [i32::MIN, -1, 0, 7, 137, 143, i32::MAX] {
            let value = ProcessTermination::Code(code);
            assert_eq!(
                serde_json::from_str::<ProcessTermination>(&serde_json::to_string(&value).unwrap())
                    .unwrap(),
                value
            );
        }
        for signal in [1, 9, 15, 127] {
            let value = ProcessTermination::Signal(signal);
            assert_eq!(
                serde_json::from_str::<ProcessTermination>(&serde_json::to_string(&value).unwrap())
                    .unwrap(),
                value
            );
        }
        for text in [
            r#"{"kind":"signal","signal":0}"#,
            r#"{"kind":"signal","signal":128}"#,
            r#"{"kind":"signal","signal":-1}"#,
            r#"{"kind":"code","code":2147483648}"#,
            r#"{"kind":"code","code":1,"signal":15}"#,
            r#"{"kind":"code","code":1,"code":2}"#,
            r#"{"kind":"unknown"}"#,
        ] {
            assert!(
                serde_json::from_str::<ProcessTermination>(text).is_err(),
                "{text}"
            );
        }
        for text in ["{}", r#"{"process_termination":null}"#] {
            assert_eq!(
                serde_json::from_str::<Observation>(text).unwrap(),
                Observation::default()
            );
        }
        let code =
            serde_json::json!({"exit_code":143,"process_termination":{"kind":"code","code":143}});
        let signal =
            serde_json::json!({"exit_code":1,"process_termination":{"kind":"signal","signal":15}});
        assert!(agree(&code, &signal).is_err());
        assert!(agree(&signal, &serde_json::json!({"exit_code":143})).is_ok());
        assert_eq!(
            serde_json::from_str::<Observation>(
                r#"{"process_termination":{"kind":"signal","signal":0}}"#
            )
            .unwrap(),
            Observation::malformed()
        );
    }
}
