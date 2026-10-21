//! Explicit client intent, kept separate from the command carrying its gate.
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Adapter-owned executable and initial arguments, before shared preparation.
pub(crate) struct ProcessLaunch {
    pub(crate) program: OsString,
    args: Vec<OsString>,
    arg0: Option<OsString>,
}

/// Frozen interpretation of a configured command, never inferred from gate argv.
pub(crate) struct TargetLaunch {
    pub(crate) program: OsString,
    pub(crate) args: Vec<OsString>,
    pub(crate) cwd: Option<PathBuf>,
    pub(crate) path: Option<OsString>,
}

/// Private framing, including explicit client argv[0], not the gate's argv[0].
pub(crate) const FORMAT: &str = "--asb-launch-v1";

pub(crate) struct CommandRoute {
    program: OsString,
    prefix: usize,
    pub(crate) gated: bool,
}

impl ProcessLaunch {
    pub(crate) fn new(program: impl AsRef<OsStr>) -> Self {
        Self {
            program: program.as_ref().to_owned(),
            args: Vec::new(),
            arg0: None,
        }
    }

    pub(crate) fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut Self {
        self.args.push(arg.as_ref().to_owned());
        self
    }

    pub(crate) fn arg0(&mut self, arg: impl AsRef<OsStr>) -> &mut Self {
        self.arg0 = Some(arg.as_ref().to_owned());
        self
    }

    pub(crate) fn direct(&self) -> Command {
        let mut command = Command::new(&self.program);
        command.args(&self.args);
        #[cfg(unix)]
        if let Some(arg0) = &self.arg0 {
            use std::os::unix::process::CommandExt;
            command.arg0(arg0);
        }
        command
    }

    /// Materialize once, before configuration. Never reconstruct this command
    /// from getters: std owns env_clear, I/O, credentials and pre-exec state.
    pub(crate) fn native(&self, gate: Option<&Path>) -> (Command, CommandRoute) {
        let (command, prefix) = if let Some(gate) = gate {
            let mut command = Command::new(gate);
            command.arg("__asb-gate").arg(FORMAT);
            match &self.arg0 {
                Some(arg0) => {
                    command.arg("1").arg(arg0);
                }
                None => {
                    command.arg("0");
                }
            }
            command.arg(&self.program).args(&self.args);
            let prefix = command.get_args().count() - self.args.len();
            (command, prefix)
        } else {
            (self.direct(), 0)
        };
        (
            command,
            CommandRoute {
                program: self.program.clone(),
                prefix,
                gated: gate.is_some(),
            },
        )
    }
}

impl CommandRoute {
    pub(crate) fn target(&self, command: &Command) -> TargetLaunch {
        TargetLaunch::from_parts(&self.program, command.get_args().skip(self.prefix), command)
    }
}

impl TargetLaunch {
    pub(crate) fn direct(command: &Command) -> Self {
        Self::from_parts(command.get_program(), command.get_args(), command)
    }

    fn from_parts<'a>(
        program: &OsStr,
        args: impl Iterator<Item = &'a OsStr>,
        command: &Command,
    ) -> Self {
        Self {
            program: program.to_owned(),
            args: args.map(OsStr::to_owned).collect(),
            cwd: command.get_current_dir().map(Path::to_path_buf),
            // Only an explicit assignment is proof of this command's PATH.
            // No assignment does not prove inheritance: env_clear is opaque.
            path: command
                .get_envs()
                .find_map(|(key, value)| (key == "PATH").then(|| value.map(OsStr::to_owned)))
                .flatten(),
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests;
