//! Best-effort interactive progress for one native benchmark attempt.

use super::capture::Progress;
use crate::{cassette, usage};
use std::io::{self, Write};
use std::time::{Duration, Instant};

const REFRESH: Duration = Duration::from_millis(250);

/// Emit a transition before an operation that may not produce agent output.
/// This is presentation-only: terminal failures never affect benchmark custody.
pub(super) fn status(enabled: bool, stage: &str) {
    if !enabled {
        return;
    }
    let mut stderr = io::stderr().lock();
    let _ = write!(stderr, "\r\x1b[2KBenchmark progress: {stage}");
    let _ = stderr.flush();
}

pub(super) fn clear(enabled: bool) {
    if enabled {
        let mut stderr = io::stderr().lock();
        let _ = clear_line(&mut stderr, true);
    }
}

#[derive(Clone, Default)]
pub(super) struct Context {
    enabled: bool,
    agent: String,
    provider: String,
    replay: Option<cassette::Statistics>,
}

impl Context {
    pub(super) fn new(
        enabled: bool,
        agent: &str,
        provider: &str,
        replay: Option<cassette::Statistics>,
    ) -> Self {
        Self {
            enabled,
            agent: agent.into(),
            provider: provider.into(),
            replay,
        }
    }

    pub(super) fn display(self) -> Option<Display> {
        self.enabled.then_some(Display {
            context: self,
            last: None,
            visible: false,
            frame: 0,
        })
    }

    pub(super) fn enabled(&self) -> bool {
        self.enabled
    }
}

pub(super) struct Display {
    context: Context,
    last: Option<Instant>,
    visible: bool,
    frame: usize,
}

impl Display {
    fn line(&self, stdout: &[u8], stderr: &[u8]) -> String {
        let stdout = String::from_utf8_lossy(stdout);
        let stderr = String::from_utf8_lossy(stderr);
        let steps = usage::steps_from_agent(&self.context.agent, &stdout, &stderr);
        let tokens = usage::from_sources(
            &self.context.provider,
            &self.context.agent,
            &stdout,
            &stderr,
            None,
        )
        .total_tokens;
        let step_text = quantity(
            steps,
            self.context.replay.as_ref().and_then(|value| value.steps),
        );
        let token_text = quantity(
            tokens,
            self.context.replay.as_ref().and_then(|value| value.tokens),
        );
        const SLIDER: [&str; 8] = ["▱▱▱", "▰▱▱", "▰▰▱", "▰▰▰", "▰▰▱", "▰▱▱", "▱▱▱", "▱▰▱"];
        format!(
            "Benchmark progress {} steps {step_text} · tokens {token_text}",
            SLIDER[self.frame % SLIDER.len()]
        )
    }

    fn write_line(&mut self, line: &str) {
        let mut stderr = io::stderr().lock();
        if write!(stderr, "\r\x1b[2K{line}")
            .and_then(|()| stderr.flush())
            .is_ok()
        {
            self.visible = true;
        }
    }
}

impl Progress for Display {
    fn update(&mut self, stdout: &[u8], stderr: &[u8]) {
        let now = Instant::now();
        if self
            .last
            .is_some_and(|last| now.duration_since(last) < REFRESH)
        {
            return;
        }
        self.last = Some(now);
        self.frame = self.frame.wrapping_add(1);
        self.write_line(&self.line(stdout, stderr));
    }

    fn finish(&mut self) {
        if self.visible {
            let mut stderr = io::stderr().lock();
            // Keep the cursor on this line: the caller immediately emits the
            // teardown transition, which should replace the active progress.
            let _ = clear_line(&mut stderr, false);
            self.visible = false;
        }
    }
}

fn clear_line(output: &mut impl Write, newline: bool) -> io::Result<()> {
    write!(output, "\r\x1b[2K")?;
    if newline {
        writeln!(output)?;
    }
    output.flush()
}

fn quantity(current: Option<u64>, total: Option<u64>) -> String {
    let current = current.map_or_else(|| "?".into(), |value| value.to_string());
    match total.filter(|total| *total > 0) {
        Some(total) => {
            let observed = current.parse::<u64>().unwrap_or(0).min(total);
            format!(
                "{current}/{total} ({}%)",
                observed.saturating_mul(100) / total
            )
        }
        None => current,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_quantities_include_known_totals_and_percentage() {
        assert_eq!(quantity(Some(3), Some(12)), "3/12 (25%)");
        assert_eq!(quantity(None, Some(12)), "?/12 (0%)");
        assert_eq!(quantity(Some(13), Some(12)), "13/12 (100%)");
    }

    #[test]
    fn live_quantities_do_not_invent_a_total() {
        assert_eq!(quantity(Some(3), None), "3");
        assert_eq!(quantity(None, None), "?");
    }

    #[test]
    fn progress_handoff_clears_in_place_but_final_cleanup_ends_the_line() {
        let mut handoff = Vec::new();
        clear_line(&mut handoff, false).unwrap();
        assert_eq!(handoff, b"\r\x1b[2K");

        let mut final_cleanup = Vec::new();
        clear_line(&mut final_cleanup, true).unwrap();
        assert_eq!(final_cleanup, b"\r\x1b[2K\n");
    }
}
