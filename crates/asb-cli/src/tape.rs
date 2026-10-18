//! Record/replay entry point, backed by the configured proxy chain.

use crate::{Error, proxy};
use std::any::Any;
use std::collections::HashMap;
use std::path::Path;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Record,
    Replay,
}

/// A running record/replay chain.
pub struct Session(proxy::Chain);

impl Session {
    pub fn start(
        mode: Mode,
        cassette: &Path,
        upstream: Option<&str>,
        respect_record_timings: bool,
        workspace: &Path,
    ) -> Result<Self, Error> {
        if mode == Mode::Replay && !cassette.is_file() {
            return Err(Error::Input(format!(
                "replay cassette {} does not exist; run `asb run CONFIG --record --cassette {}` first",
                cassette.display(),
                cassette.display()
            )));
        }
        let settings = proxy::adapters::llmtape::Settings {
            mode,
            cassette: cassette.to_path_buf(),
            respect_record_timings,
            workspace: workspace.to_path_buf(),
        };
        let mut configurations: HashMap<String, Box<dyn Any>> = HashMap::new();
        configurations.insert(proxy::adapters::llmtape::NAME.into(), Box::new(settings));
        let names = proxy::configured_chain(proxy::adapters::llmtape::NAME)?;
        proxy::Chain::start(&names, upstream, &configurations).map(Self)
    }

    pub fn url(&self) -> &str {
        self.0.url()
    }

    pub fn stop(self) -> Result<(), Error> {
        self.0.stop()
    }
}
