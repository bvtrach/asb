//! A first writer failure stops admission, never evaluation or owned cleanup.
use crate::Error;
use std::io::{self, Write};

pub(super) struct Presentation<'a> {
    output: &'a mut dyn Write,
    failure: Option<(io::ErrorKind, String)>,
}

impl<'a> Presentation<'a> {
    pub fn new(output: &'a mut dyn Write) -> Self {
        Self {
            output,
            failure: None,
        }
    }
    pub fn failed(&self) -> bool {
        self.failure.is_some()
    }
    pub fn error(&self) -> Option<Error> {
        self.failure
            .as_ref()
            .map(|(_, reason)| Error::Io(format!("sweep output failed: {reason}")))
    }
    pub fn stop(&mut self, error: &Error) {
        if self.failure.is_none() {
            self.failure = Some((io::ErrorKind::Other, error.to_string()));
        }
    }
    fn failed_io(&self) -> io::Error {
        let (kind, reason) = self.failure.as_ref().expect("latched writer failure");
        io::Error::new(*kind, reason.clone())
    }
    fn observe<T>(&mut self, result: io::Result<T>) -> io::Result<T> {
        if let Err(error) = &result
            && error.kind() != io::ErrorKind::Interrupted
        {
            self.failure
                .get_or_insert((error.kind(), error.to_string()));
        }
        result
    }
}
impl Write for Presentation<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.failed() {
            return Err(self.failed_io());
        }
        let result = self.output.write(bytes).and_then(|written| {
            if written == 0 && !bytes.is_empty() {
                Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "sweep output accepted no bytes",
                ))
            } else {
                Ok(written)
            }
        });
        self.observe(result)
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.failed() {
            return Err(self.failed_io());
        }
        let result = self.output.flush();
        self.observe(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interrupted_writes_retry_and_interrupted_flush_is_not_a_permanent_latch() {
        struct Output {
            writes: usize,
            flushes: usize,
            permanent: bool,
        }
        impl Write for Output {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.writes += 1;
                if self.writes == 1 {
                    Err(io::Error::from(io::ErrorKind::Interrupted))
                } else if self.permanent {
                    Err(io::Error::other("real writer failure"))
                } else {
                    Ok(bytes.len())
                }
            }
            fn flush(&mut self) -> io::Result<()> {
                self.flushes += 1;
                if self.flushes == 1 {
                    Err(io::Error::from(io::ErrorKind::Interrupted))
                } else {
                    Ok(())
                }
            }
        }
        for permanent in [false, true] {
            let mut output = Output {
                writes: 0,
                flushes: 0,
                permanent,
            };
            let mut writer = Presentation::new(&mut output);
            assert_eq!(writer.write_all(b"bytes").is_err(), permanent);
            assert_eq!(writer.failed(), permanent);
            if permanent {
                assert!(writer.write_all(b"no retry").is_err());
            } else {
                assert_eq!(
                    writer.flush().unwrap_err().kind(),
                    io::ErrorKind::Interrupted
                );
                assert!(!writer.failed());
                writer.flush().unwrap();
            }
            drop(writer);
            assert_eq!(output.writes, 2);
            assert_eq!(output.flushes, if permanent { 0 } else { 2 });
        }
    }
    #[test]
    fn first_write_flush_and_zero_write_failures_are_sticky_without_retry() {
        struct Output {
            zero: bool,
            flush_failure: bool,
            writes: usize,
            flushes: usize,
        }
        impl Write for Output {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.writes += 1;
                if self.zero {
                    Ok(0)
                } else if !self.flush_failure {
                    Err(io::Error::other("fixture write"))
                } else {
                    Ok(bytes.len())
                }
            }
            fn flush(&mut self) -> io::Result<()> {
                self.flushes += 1;
                Err(io::Error::other("fixture flush"))
            }
        }
        for (zero, flush_failure) in [(false, false), (true, false), (false, true)] {
            let mut output = Output {
                zero,
                flush_failure,
                writes: 0,
                flushes: 0,
            };
            let mut presentation = Presentation::new(&mut output);
            let result = presentation.write_all(b"first");
            if flush_failure {
                result.unwrap();
                assert!(presentation.flush().is_err());
            } else {
                assert!(result.is_err());
            }
            assert!(presentation.failed());
            assert!(presentation.write_all(b"again").is_err());
            assert!(presentation.flush().is_err());
            drop(presentation);
            assert_eq!(output.writes, 1);
            assert_eq!(output.flushes, usize::from(flush_failure));
        }
    }
}
