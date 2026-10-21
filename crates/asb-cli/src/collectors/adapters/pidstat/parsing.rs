//! Sum process rows within each pidstat observation, not across time points.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(in crate::collectors::adapters) struct Samples {
    pub series: BTreeMap<String, Vec<f64>>,
    pub timestamps: BTreeMap<String, Vec<f64>>,
}

struct Observation {
    key: (usize, u64),
    elapsed: f64,
    totals: BTreeMap<String, f64>,
    pids: BTreeSet<String>,
}

pub(in crate::collectors::adapters) fn parse_samples(
    output: &str,
    wanted: &[&str],
    name: impl Fn(&str) -> String,
) -> Samples {
    let mut header: Option<Vec<&str>> = None;
    let mut origin = None;
    let mut marker = None;
    let mut block = 0usize;
    let mut groups: Vec<Observation> = Vec::new();
    for line in output.lines() {
        if let Some(value) = line.strip_prefix("# asb-snapshot-elapsed-seconds ") {
            marker = value
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite() && *v >= 0.0);
            block += 1;
            header = None;
            continue;
        }
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.first() == Some(&"Linux") {
            block += 1;
            header = None;
            continue;
        }
        if fields.first() == Some(&"Average:") {
            continue;
        }
        if fields.contains(&"PID") {
            header = fields
                .iter()
                .any(|field| wanted.contains(field))
                .then(|| fields.clone());
            if header.is_some() && origin.is_none() {
                origin = clock_seconds(&fields);
            }
            continue;
        }
        let Some(columns) = header.as_ref() else {
            continue;
        };
        if fields.len() != columns.len() {
            continue;
        }
        let Some(pid_index) = columns.iter().position(|field| *field == "PID") else {
            continue;
        };
        let Some(pid) = fields
            .get(pid_index)
            .filter(|value| value.parse::<u32>().is_ok_and(|v| v > 0))
        else {
            continue;
        };
        let Some(clock) = clock_seconds(&fields) else {
            continue;
        };
        let elapsed =
            marker.unwrap_or_else(|| (clock - origin.unwrap_or(clock)).rem_euclid(86400.0));
        let key = (block, clock.to_bits());
        if groups.last().is_none_or(|group| group.key != key) {
            groups.push(Observation {
                key,
                elapsed,
                totals: BTreeMap::new(),
                pids: BTreeSet::new(),
            });
        }
        let observation = groups.last_mut().expect("observation group");
        if !observation.pids.insert((*pid).to_owned()) {
            continue;
        }
        for column in wanted {
            let Some(index) = columns.iter().position(|field| field == column) else {
                continue;
            };
            let Some(value) = fields[index]
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite() && *v >= 0.0)
            else {
                continue;
            };
            *observation.totals.entry(name(column)).or_default() += value;
        }
    }
    let mut result = Samples::default();
    for observation in groups {
        for (name, value) in observation.totals {
            if value.is_finite() {
                result
                    .timestamps
                    .entry(name.clone())
                    .or_default()
                    .push(observation.elapsed * 1_000.0);
                result.series.entry(name).or_default().push(value);
            }
        }
    }
    result
}

pub(in crate::collectors::adapters) fn parse_columns(
    output: &str,
    wanted: &[&str],
    name: impl Fn(&str) -> String,
) -> BTreeMap<String, f64> {
    parse_samples(output, wanted, name)
        .series
        .into_iter()
        .map(|(name, values)| {
            let mean = values.iter().sum::<f64>() / values.len() as f64;
            (name, mean)
        })
        .filter(|(_, mean)| mean.is_finite())
        .collect()
}

fn clock_seconds(fields: &[&str]) -> Option<f64> {
    let components = fields.first()?.split(':').collect::<Vec<_>>();
    if components.len() != 3 {
        return None;
    }
    let mut hour = components[0].parse::<u32>().ok()?;
    let minute = components[1].parse::<u32>().ok()?;
    let second = components[2].parse::<f64>().ok()?;
    if hour > 23 || minute > 59 || !second.is_finite() || !(0.0..60.0).contains(&second) {
        return None;
    }
    if matches!(fields.get(1), Some(&"AM" | &"PM")) {
        if hour == 0 || hour > 12 {
            return None;
        }
        hour %= 12;
        if fields.get(1) == Some(&"PM") {
            hour += 12;
        }
    }
    Some(f64::from(hour * 3600 + minute * 60) + second)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sums_tree_rows_per_interval_and_averages_intervals_not_processes() {
        let input = "Linux test\n12:00:00 UID PID %usr %CPU Command\n12:00:01 1000 42 10 20 agent\n12:00:01 1000 43 30 40 child\n12:00:01 1000 43 30 40 child\n12:00:03 1000 42 20 30 agent\nAverage: UID PID %usr %CPU Command\nAverage: 1000 42 999 999 agent\n12:00:03 UID PID usr-ms Command\n12:00:04 1000 42 500 agent\n";
        let samples = parse_samples(input, &["%usr", "%CPU"], str::to_owned);
        assert_eq!(samples.series["%usr"], vec![40.0, 20.0]);
        assert_eq!(samples.series["%CPU"], vec![60.0, 30.0]);
        assert_eq!(samples.timestamps["%CPU"], vec![1_000.0, 3_000.0]);
        assert_eq!(parse_columns(input, &["%CPU"], str::to_owned)["%CPU"], 45.0);
    }

    #[test]
    fn monotonic_snapshot_markers_preserve_gaps_and_ignore_wall_clock_changes() {
        let input = "# asb-snapshot-elapsed-seconds 1.25\nLinux test\n23:59:59 UID PID kB_rd/s Command\n00:00:00 1000 42 5 agent\n00:00:00 1000 43 7 child\n# asb-snapshot-elapsed-seconds 4.50\nLinux test\n10:00:00 UID PID kB_rd/s Command\n10:00:01 1000 42 3 agent\n";
        let samples = parse_samples(input, &["kB_rd/s"], str::to_owned);
        assert_eq!(samples.series["kB_rd/s"], vec![12.0, 3.0]);
        assert_eq!(samples.timestamps["kB_rd/s"], vec![1_250.0, 4_500.0]);
        assert!(
            parse_samples("unavailable", &["%CPU"], str::to_owned)
                .series
                .is_empty()
        );
    }

    #[test]
    fn legacy_timestamps_handle_midnight_and_twelve_hour_clocks() {
        let input = "11:59:59 PM UID PID %CPU Command\n12:00:01 AM 1000 42 3 agent\n";
        assert_eq!(
            parse_samples(input, &["%CPU"], str::to_owned).timestamps["%CPU"],
            vec![2_000.0]
        );
        let invalid = "12:00:00 UID PID %CPU Command\n12:00:01 1000 42 NaN agent\n12:00:02 1000 43 -1 child\n12:00:03 1000 44 inf child\n";
        assert!(
            parse_samples(invalid, &["%CPU"], str::to_owned)
                .series
                .is_empty()
        );
    }
}
