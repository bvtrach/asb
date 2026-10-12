// Copyright (C) Huawei Technologies Co., Ltd. 2026. All rights reserved.
// SPDX-License-Identifier: MIT
//! The single human step/progress renderer.
//!
//! This module deliberately has no background thread or timer.  Callers feed
//! observations through [`Step::update`], which keeps refreshes bounded and
//! makes the renderer deterministic in tests.  JSON and quiet callers should
//! never construct a reporter; the output router can use [`StepReporter::sink`]
//! when a generic writer is required.

#![allow(dead_code)]

use std::fmt::Write as _;
use std::io::{self, Write};
use std::time::{Duration, Instant};

const DELAY: Duration = Duration::from_secs(1);
const REFRESH: Duration = Duration::from_millis(100);

/// Clock used by the reporter; production uses [`InstantClock`] and tests can
/// provide a deterministic implementation.
pub trait MonotonicClock {
    /// Return elapsed monotonic time.
    fn now(&self) -> Duration;
}

/// Production monotonic clock.
#[derive(Debug)]
pub struct InstantClock(Instant);

impl Default for InstantClock {
    fn default() -> Self {
        Self(Instant::now())
    }
}

impl MonotonicClock for InstantClock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }
}

/// Terminal outcome for a step.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StepStatus {
    /// Work completed successfully.
    Success,
    /// Work failed.
    Failure,
    /// Work was cancelled or interrupted.
    Cancelled,
}

/// A bounded human status/progress reporter.
pub struct StepReporter<W, C = InstantClock> {
    writer: W,
    clock: C,
    terminal: bool,
    width: usize,
    active: Option<ActiveStep>,
    last_error: Option<io::Error>,
}

impl<W: Write, C> Write for StepReporter<W, C> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.writer.write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

#[derive(Debug)]
struct ActiveStep {
    id: String,
    label: String,
    total: Option<u64>,
    completed: u64,
    started: Duration,
    rendered: bool,
    last_refresh: Duration,
}

/// A live step owned by a reporter.  Dropping an unfinished step finalizes it
/// as cancelled, ensuring interrupted operations never leave a progress bar.
pub struct Step<'a, W: Write, C: MonotonicClock = InstantClock> {
    reporter: &'a mut StepReporter<W, C>,
    finished: bool,
}

impl<W: Write> StepReporter<W, InstantClock> {
    /// Construct a reporter for a human output stream.
    pub fn new(writer: W, terminal: bool, width: usize) -> Self {
        Self::with_clock(writer, terminal, width, InstantClock::default())
    }
}

impl<W, C> StepReporter<W, C>
where
    W: Write,
    C: MonotonicClock,
{
    /// Construct a reporter with an injected clock.
    pub fn with_clock(writer: W, terminal: bool, width: usize, clock: C) -> Self {
        Self {
            writer,
            clock,
            terminal,
            width: width.max(20),
            active: None,
            last_error: None,
        }
    }

    /// Return a reporter that cannot emit bytes.  Useful for router sinks.
    pub fn sink(writer: W, clock: C) -> Self {
        Self::with_clock(writer, false, 20, clock)
    }

    /// Begin one structured step.  A step completing before one second emits
    /// only its terminal fixed-width line.
    pub fn begin<'a>(
        &'a mut self,
        id: impl Into<String>,
        label: impl Into<String>,
        total: Option<u64>,
    ) -> Step<'a, W, C> {
        self.active = Some(ActiveStep {
            id: id.into(),
            label: label.into(),
            total,
            completed: 0,
            started: self.clock.now(),
            rendered: false,
            last_refresh: self.clock.now(),
        });
        Step {
            reporter: self,
            finished: false,
        }
    }

    /// Return the first writer failure, if any.
    pub fn error(&self) -> Option<&io::Error> {
        self.last_error.as_ref()
    }

    /// Access the underlying writer after all steps have completed.
    pub fn into_inner(self) -> W {
        self.writer
    }

    fn update(&mut self, completed: u64, total: Option<u64>, message: Option<&str>) {
        let now = self.clock.now();
        let Some(step) = self.active.as_mut() else {
            return;
        };
        step.completed = completed;
        if total.is_some() {
            step.total = total;
        }
        if now.saturating_sub(step.started) >= DELAY
            && now.saturating_sub(step.last_refresh) >= REFRESH
        {
            step.rendered = true;
            step.last_refresh = now;
            let line = render_line(step, now.saturating_sub(step.started), self.width, message);
            self.write_line(&line, self.terminal);
        }
    }

    fn finish(&mut self, status: StepStatus, message: Option<&str>) {
        let Some(step) = self.active.take() else {
            return;
        };
        let elapsed = self.clock.now().saturating_sub(step.started);
        let line = render_terminal(&step, status, elapsed, self.width, message);
        self.write_line(&line, false);
    }

    fn write_line(&mut self, line: &str, redraw: bool) {
        let result = if redraw {
            write!(self.writer, "\r{}\x1b[K", line)
        } else {
            writeln!(self.writer, "{}", line)
        };
        if let Err(error) = result {
            self.last_error = Some(error);
        } else if let Err(error) = self.writer.flush() {
            self.last_error = Some(error);
        }
    }
}

impl<'a, W, C> Step<'a, W, C>
where
    W: Write,
    C: MonotonicClock,
{
    /// Record completed work and optionally revise the total.
    pub fn update(&mut self, completed: u64, total: Option<u64>) {
        self.reporter.update(completed, total, None);
    }

    /// Record progress and a bounded human detail message.
    pub fn detail(&mut self, completed: u64, total: Option<u64>, message: &str) {
        self.reporter.update(completed, total, Some(message));
    }

    /// Complete successfully.
    pub fn success(mut self) {
        self.reporter.finish(StepStatus::Success, None);
        self.finished = true;
    }

    /// Finalize with a failure message.
    pub fn failure(mut self, message: &str) {
        self.reporter.finish(StepStatus::Failure, Some(message));
        self.finished = true;
    }

    /// Finalize as cancelled/interrupted.
    pub fn cancel(mut self) {
        self.reporter.finish(StepStatus::Cancelled, None);
        self.finished = true;
    }
}

impl<W, C> Drop for Step<'_, W, C>
where
    W: Write,
    C: MonotonicClock,
{
    fn drop(&mut self) {
        if !self.finished {
            self.reporter.finish(StepStatus::Cancelled, None);
        }
    }
}

fn clip(value: &str, width: usize) -> String {
    value.chars().take(width).collect()
}

fn render_line(
    step: &ActiveStep,
    elapsed: Duration,
    width: usize,
    message: Option<&str>,
) -> String {
    let mut line = String::new();
    let _ = write!(line, "[WAIT] {} ({})", step.label, step.id);
    if let Some(total) = step.total {
        let done = step.completed.min(total);
        let _ = write!(line, " {done}/{total}");
        if done > 0 && elapsed.as_secs_f64() > 0.0 && done < total {
            let eta = elapsed
                .mul_f64((total - done) as f64 / done as f64)
                .as_secs();
            let _ = write!(line, " eta~{eta}s");
        }
    } else {
        let _ = write!(line, " -- elapsed={}s", elapsed.as_secs());
    }
    if let Some(message) = message {
        let _ = write!(line, " {message}");
    }
    clip(&line, width)
}

fn render_terminal(
    step: &ActiveStep,
    status: StepStatus,
    elapsed: Duration,
    width: usize,
    message: Option<&str>,
) -> String {
    let tag = match status {
        StepStatus::Success => "DONE",
        StepStatus::Failure => "FAIL",
        StepStatus::Cancelled => "CANCEL",
    };
    let mut line = String::new();
    let _ = write!(
        line,
        "[{tag}] {} ({}, {}s)",
        step.label,
        step.id,
        elapsed.as_secs()
    );
    if let Some(total) = step.total {
        let _ = write!(line, " {}/{}", step.completed.min(total), total);
    }
    if let Some(message) = message {
        let _ = write!(line, " {message}");
    }
    clip(&line, width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    #[derive(Clone, Default)]
    struct FakeClock(Rc<Cell<Duration>>);
    impl MonotonicClock for FakeClock {
        fn now(&self) -> Duration {
            self.0.get()
        }
    }

    #[test]
    fn short_steps_emit_only_terminal_line() {
        let clock = FakeClock::default();
        let mut out = Vec::new();
        let mut reporter = StepReporter::with_clock(&mut out, false, 80, clock.clone());
        let step = reporter.begin("one", "quick", Some(2));
        step.success();
        let text = String::from_utf8(out).unwrap();
        assert_eq!(text.lines().count(), 1);
        assert!(text.starts_with("[DONE] quick"));
    }

    #[test]
    fn known_and_unknown_work_are_honest() {
        let clock = FakeClock::default();
        let mut out = Vec::new();
        let mut reporter = StepReporter::with_clock(&mut out, false, 80, clock.clone());
        let mut step = reporter.begin("known", "known", Some(4));
        clock.0.set(Duration::from_secs(1));
        step.update(1, None);
        clock.0.set(Duration::from_secs(6));
        step.detail(2, None, "working");
        step.success();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("2/4"));
        assert!(text.contains("eta~"));

        let mut out = Vec::new();
        let mut reporter = StepReporter::with_clock(&mut out, false, 80, clock.clone());
        let mut step = reporter.begin("unknown", "unknown", None);
        clock.0.set(Duration::from_secs(8));
        step.update(3, None);
        step.success();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("-- elapsed="));
        assert!(!text.contains("eta~"));
    }

    #[test]
    fn dropped_steps_finalize_and_terminal_is_plain() {
        let clock = FakeClock::default();
        let mut out = Vec::new();
        let mut reporter = StepReporter::with_clock(&mut out, true, 24, clock);
        let _step = reporter.begin("cancel", "a very long label", None);
        drop(_step);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("[CANCEL]"));
        assert!(!text.contains('\u{1b}'));
        assert!(text.lines().all(|line| line.chars().count() <= 24));
    }
}
