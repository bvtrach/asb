use super::super::{CollectorAdapter, ProcessContext, ProcessData};
use crate::measurements;
use crate::measurements::{parse_float, parse_pidstat_columns};
use std::collections::BTreeMap;
pub(super) struct MpstatCollector;
pub(super) static ADAPTER: MpstatCollector = MpstatCollector;

impl CollectorAdapter for MpstatCollector {
    fn id(&self) -> &'static str {
        "mpstat"
    }
    fn environment_flag(&self) -> Option<&'static str> {
        Some("ASB_MEASURE_MPSTAT")
    }
    fn tool_id(&self) -> Option<&'static str> {
        Some("mpstat")
    }
    fn requires_pid(&self) -> bool {
        true
    }
    fn preflight(&self, context: &mut super::super::PreflightContext<'_>) -> Result<(), String> {
        if super::super::selected_or_environment("mpstat", context.selected) {
            super::pidstat::preflight(context.tool_dir, "-u")
        } else {
            Ok(())
        }
    }
    fn configure(
        &self,
        session: &mut measurements::Session,
        selected: &[String],
    ) -> Result<(), String> {
        session.state::<CpuState>().borrow_mut().enabled =
            super::super::selected_or_environment("mpstat", selected);
        Ok(())
    }
    fn process(
        &self,
        session: &mut measurements::Session,
        context: &ProcessContext<'_>,
    ) -> Result<ProcessData, String> {
        process(session, context)
    }
    fn attach_pid(&self, session: &mut measurements::Session, pid: u32) -> Result<(), String> {
        session.start_cpu_snapshot(pid)
    }
}

fn process(
    _: &mut measurements::Session,
    context: &ProcessContext<'_>,
) -> Result<ProcessData, String> {
    let mut result = ProcessData::from_output(context);
    result.metrics = parse_pidstat_cpu_metrics(context.stdout);
    if result.metrics.is_empty() {
        result.metrics = parse_mpstat_metrics(context.stdout);
    }
    result.series = parse_series(context.stdout);
    Ok(result)
}

pub(crate) fn parse_series(output: &str) -> std::collections::BTreeMap<String, Vec<f64>> {
    measurements::parse_pidstat_series(
        output,
        &["%usr", "%system", "%guest", "%wait", "%CPU"],
        |name| {
            format!(
                "process.cpu.{}",
                match name {
                    "%usr" => "user_pct",
                    "%system" => "system_pct",
                    "%guest" => "guest_pct",
                    "%wait" => "wait_pct",
                    "%CPU" => "total_pct",
                    _ => "unknown",
                }
            )
        },
    )
}

impl measurements::Session {
    pub(crate) fn start_cpu_snapshot(&mut self, pid: u32) -> Result<(), String> {
        let interval = measurements::pidstat_interval_seconds();
        let enabled = self.state::<CpuState>().borrow().enabled;
        self.start_snapshot(
            "mpstat",
            "ASB_MEASURE_MPSTAT",
            &["-u", "-T", "ALL", "-p", &pid.to_string(), &interval],
            enabled,
            &super::pidstat::executable(self.tool_dir.as_deref()),
        )
    }
}

#[derive(Default)]
struct CpuState {
    enabled: bool,
}

pub(crate) fn parse_mpstat_metrics(output: &str) -> BTreeMap<String, f64> {
    let mut header = None;
    let mut totals: BTreeMap<String, (f64, usize)> = BTreeMap::new();
    for line in output.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if let Some(cpu) = fields.iter().position(|field| *field == "CPU")
            && fields
                .get(cpu + 1)
                .is_some_and(|field| field.starts_with('%'))
        {
            header = Some((cpu, fields[cpu + 1..].to_vec()));
            continue;
        }
        let Some((cpu_index, names)) = &header else {
            continue;
        };
        if fields.get(*cpu_index).copied() != Some("all") {
            continue;
        }
        for (index, name) in names.iter().enumerate() {
            let Some(value) = fields
                .get(cpu_index + 1 + index)
                .and_then(|field| parse_float(field))
            else {
                continue;
            };
            let key = mpstat_name(name);
            let entry = totals.entry(key).or_default();
            entry.0 += value;
            entry.1 += 1;
        }
    }
    totals
        .into_iter()
        .filter_map(|(name, (sum, count))| {
            (count > 0).then_some((format!("cpu.{name}"), sum / count as f64))
        })
        .collect()
}

/// Parse process-scoped `pidstat -u` output. The collector is retained under
/// the stable `mpstat` result name for compatibility, but it no longer uses
/// host-wide mpstat CPU totals.
pub(crate) fn parse_pidstat_cpu_metrics(output: &str) -> BTreeMap<String, f64> {
    parse_pidstat_columns(
        output,
        &["%usr", "%system", "%guest", "%wait", "%CPU"],
        |name| {
            format!(
                "process.cpu.{}",
                match name {
                    "%usr" => "user_pct",
                    "%system" => "system_pct",
                    "%guest" => "guest_pct",
                    "%wait" => "wait_pct",
                    "%CPU" => "total_pct",
                    _ => "unknown",
                }
            )
        },
    )
}

fn mpstat_name(name: &str) -> String {
    let mapped = match name.trim_start_matches('%') {
        "usr" => Some("user_pct"),
        "nice" => Some("nice_pct"),
        "sys" => Some("system_pct"),
        "iowait" => Some("iowait_pct"),
        "irq" => Some("irq_pct"),
        "soft" => Some("soft_pct"),
        "steal" => Some("steal_pct"),
        "guest" => Some("guest_pct"),
        "gnice" => Some("guest_nice_pct"),
        _ => None,
    };
    mapped
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{}_pct", name.trim_start_matches('%')))
}
