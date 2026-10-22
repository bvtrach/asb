//! Timestamped map groups: missing keys are gaps, never synthetic zero samples.

use super::*;

const BEGIN: &str = "ASB_BPFTRACE_SNAPSHOT ";
const END: &str = "ASB_BPFTRACE_SNAPSHOT_END";

#[derive(Default)]
pub(super) struct Samples {
    pub values: BTreeMap<String, Vec<f64>>,
    pub timestamps: BTreeMap<String, Vec<f64>>,
    pub groups: usize,
    pub accepted_groups: usize,
}

pub(super) fn parse(
    profile: &str,
    output: &str,
    bounds: Option<(u64, u64)>,
) -> Result<Samples, String> {
    let mut result = Samples::default();
    let mut group: Option<(u64, String)> = None;
    let mut previous = None;
    for line in output.lines() {
        let line = line.trim();
        if let Some(clock) = line.strip_prefix(BEGIN) {
            let clock = clock
                .parse::<u64>()
                .map_err(|_| "invalid BPF snapshot monotonic timestamp")?;
            if group.is_some() || previous.is_some_and(|prev| clock <= prev) {
                return Err("nested or non-increasing BPF snapshot timestamps".into());
            }
            group = Some((clock, String::new()));
        } else if line == END {
            let (clock, maps) = group.take().ok_or("BPF snapshot end has no beginning")?;
            previous = Some(clock);
            result.groups += 1;
            let Some((start, end)) = bounds.filter(|(start, end)| end >= start) else {
                continue;
            };
            if clock < start || clock > end {
                continue;
            }
            result.accepted_groups += 1;
            for (metric, value) in parse_bpftrace_metrics(profile, &maps) {
                let values = result.values.entry(metric.clone()).or_default();
                if values.len() >= 100_000 {
                    return Err("BPF snapshot series exceeds 100000 observations".into());
                }
                values.push(value);
                result
                    .timestamps
                    .entry(metric)
                    .or_default()
                    .push((clock - start) as f64 / 1_000_000.0);
            }
        } else if let Some((_, maps)) = group.as_mut() {
            maps.push_str(line);
            maps.push('\n');
        }
    }
    if group.is_some() {
        return Err("incomplete BPF snapshot group; no partial time series published".into());
    }
    Ok(result)
}

#[allow(unsafe_code)] // Narrow POSIX clock read; no host mutation or child hooks.
pub(super) fn monotonic_ns() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let mut value = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // SAFETY: a valid writable timespec, and a clock with no pointer inputs.
        if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut value) } != 0 {
            return None;
        }
        let seconds = u64::try_from(value.tv_sec).ok()?;
        let nanos = u64::try_from(value.tv_nsec).ok()?;
        if nanos >= 1_000_000_000 {
            return None;
        }
        seconds.checked_mul(1_000_000_000)?.checked_add(nanos)
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sparse_maps_retain_actual_shared_clock_not_array_index_or_zero_padding() {
        let output = "ASB_BPFTRACE_SNAPSHOT 900000000\n@asb_events: 1\nASB_BPFTRACE_SNAPSHOT_END\nASB_BPFTRACE_SNAPSHOT 1100000000\n@asb_events: 2\n@asb_dest[4]: 3\nASB_BPFTRACE_SNAPSHOT_END\nASB_BPFTRACE_SNAPSHOT 2300000000\n@asb_events: 8\nASB_BPFTRACE_SNAPSHOT_END\nASB_BPFTRACE_SNAPSHOT 3800000000\n@asb_dest[4]: 9\nASB_BPFTRACE_SNAPSHOT_END\n@asb_events: 10\n";
        let samples = parse("fixture", output, Some((1_000_000_000, 4_000_000_000))).unwrap();
        assert_eq!(samples.groups, 4);
        assert_eq!(samples.values["bpftrace.fixture.events"], [2.0, 8.0]);
        assert_eq!(
            samples.timestamps["bpftrace.fixture.events"],
            [100.0, 1300.0]
        );
        assert_eq!(samples.values["bpftrace.fixture.dest[4]"], [3.0, 9.0]);
        assert_eq!(
            samples.timestamps["bpftrace.fixture.dest[4]"],
            [100.0, 2800.0]
        );
        assert_eq!(
            parse_bpftrace_metrics("fixture", output)["bpftrace.fixture.events"],
            10.0
        );
        assert!(parse("fixture", output, None).unwrap().values.is_empty());
        assert!(
            parse("fixture", "@asb_events: 8\n", Some((0, 5)))
                .unwrap()
                .values
                .is_empty()
        );
    }
    #[test]
    fn malformed_frames_are_errors_instead_of_plausible_partial_series() {
        for output in [
            "ASB_BPFTRACE_SNAPSHOT nope\n",
            "ASB_BPFTRACE_SNAPSHOT_END\n",
            "ASB_BPFTRACE_SNAPSHOT 1\n",
            "ASB_BPFTRACE_SNAPSHOT 2\nASB_BPFTRACE_SNAPSHOT 3\n",
            "ASB_BPFTRACE_SNAPSHOT 2\nASB_BPFTRACE_SNAPSHOT_END\nASB_BPFTRACE_SNAPSHOT 1\nASB_BPFTRACE_SNAPSHOT_END\n",
        ] {
            assert!(parse("fixture", output, Some((0, 10))).is_err(), "{output}");
        }
    }
    #[test]
    #[cfg(target_os = "linux")]
    fn native_monotonic_clock_is_available_without_privileges() {
        let first = monotonic_ns().unwrap();
        assert!(monotonic_ns().unwrap() >= first);
    }
}
