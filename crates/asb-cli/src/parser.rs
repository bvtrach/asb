//! The single typed argument parser for the `asb` executable.

use super::{
    CliError, Error, completion, config, install, models, project, runner, sweep, tape, tui,
};
use clap::{ArgGroup, Args, CommandFactory, Parser, Subcommand};
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::PathBuf;

pub(crate) fn run(args: &[OsString], stdout: &mut dyn Write, stderr: &mut dyn Write) -> u8 {
    let mut argv = Vec::with_capacity(args.len() + 1);
    argv.push(OsString::from("asb"));
    argv.extend_from_slice(args);
    let cli = match Cli::try_parse_from(argv) {
        Ok(cli) => cli,
        Err(error) => {
            let mut text = error.render().to_string();
            if let Some(suggestion) = close_command(args) {
                text.push_str(&format!("\nasb: did you mean `{suggestion}`?\n"));
            }
            if error.use_stderr() {
                let _ = write!(stderr, "{text}");
            } else {
                let _ = write!(stdout, "{text}");
            }
            return error.exit_code() as u8;
        }
    };
    if let Some(shell) = cli.completion.as_deref() {
        let result = match shell {
            "bash" => completion::bash(stdout),
            "zsh" => completion::zsh(stdout),
            _ => Err(Error::Input("completion shell must be bash or zsh".into())),
        };
        return result.map_or_else(|error| fail(stderr, &error.to_string()), |_| 0);
    }
    let result = match cli.command {
        None => stdout
            .write_all(Cli::command().render_help().to_string().as_bytes())
            .map_err(io_error),
        Some(CommandLine::Init(options)) => project::init(options.root, options.reset, stdout),
        Some(CommandLine::Setup(options)) => match options.mode {
            Some(SetupMode::Tui) => tui::setup(stdout),
            None => config::setup(options.into(), stdout),
        },
        Some(CommandLine::Run(options)) => {
            let mode = match tape_mode(options.record, options.replay) {
                Ok(mode) => mode,
                Err(error) => return fail(stderr, &error.to_string()),
            };
            if options.cassette.is_some() && mode.is_none() {
                return fail(
                    stderr,
                    "--cassette requires --record or --replay; choose a tape mode first",
                );
            }
            runner::run_with_tape(
                &options.path,
                options.json,
                options.show_output,
                options.measurements.as_deref(),
                mode,
                options.cassette.as_deref(),
                stdout,
            )
        }
        Some(CommandLine::Sweep(options)) => {
            let mode = match tape_mode(options.record, options.replay) {
                Ok(mode) => mode,
                Err(error) => return fail(stderr, &error.to_string()),
            };
            sweep::run(
                &options.path,
                options.json,
                options.show_output,
                mode,
                stdout,
            )
        }
        Some(CommandLine::Compare(options)) => super::compare::run(&options.path, stdout),
        Some(CommandLine::Provider(options)) => match options.command {
            ProviderCommand::Models(args) => models::list(args.into(), stdout),
        },
        Some(CommandLine::Tool(options)) => match options.command {
            ToolCommand::List(_) => install::tool_list(stdout),
            ToolCommand::Install(args) => install::tool(args.into(), stdout),
        },
        Some(CommandLine::Workload(options)) => match options.command {
            WorkloadCommand::List(args) => install::workload_list(args.into(), stdout),
            WorkloadCommand::Install(args) => install::workload(args.into(), stdout),
            WorkloadCommand::Tui => tui::workload_tui(stdout),
        },
    };
    match result {
        Ok(()) => 0,
        Err(error) => fail(stderr, &error.to_string()),
    }
}

fn close_command(args: &[OsString]) -> Option<&'static str> {
    let first = args.first()?.to_str()?;
    let (value, choices): (&str, &[&str]) = match first {
        "provider" => (args.get(1)?.to_str()?, &["models"]),
        "tool" => (args.get(1)?.to_str()?, &["list", "install"]),
        "workload" => (args.get(1)?.to_str()?, &["list", "install", "tui"]),
        _ => (
            first,
            &[
                "init", "setup", "run", "sweep", "compare", "provider", "tool", "workload",
            ],
        ),
    };
    super::suggestions::closest(value, choices)
}

fn tape_mode(record: bool, replay: bool) -> Result<Option<tape::Mode>, CliError> {
    match (record, replay) {
        (true, true) => Err(CliError::Input(
            "--record and --replay are mutually exclusive; choose one".into(),
        )),
        (true, false) => Ok(Some(tape::Mode::Record)),
        (false, true) => Ok(Some(tape::Mode::Replay)),
        (false, false) => Ok(None),
    }
}

fn fail(stderr: &mut dyn Write, message: &str) -> u8 {
    let _ = writeln!(stderr, "asb: {message}");
    2
}

fn io_error(error: io::Error) -> CliError {
    CliError::Io(error.to_string())
}

#[derive(Debug, Parser)]
#[command(
    name = "asb",
    version,
    about = "Agent Systems Benchmark command line interface"
)]
struct Cli {
    #[arg(long = "__generate-completion", value_name = "SHELL", hide = true)]
    completion: Option<String>,
    #[command(subcommand)]
    command: Option<CommandLine>,
}

#[derive(Debug, Subcommand)]
enum CommandLine {
    Init(InitArgs),
    Setup(SetupArgs),
    Run(RunArgs),
    Sweep(SweepArgs),
    Compare(CompareArgs),
    Provider(ProviderArgs),
    Tool(ToolArgs),
    Workload(WorkloadArgs),
}

#[derive(Debug, Args)]
struct InitArgs {
    #[arg(value_name = "PROJECT_DIR")]
    root: Option<PathBuf>,
    #[arg(long)]
    reset: bool,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum SetupMode {
    Tui,
}

#[derive(Debug, Args)]
struct SetupArgs {
    #[arg(value_enum)]
    mode: Option<SetupMode>,
    #[arg(long)]
    agent: Option<String>,
    #[arg(long)]
    workload: Option<String>,
    #[arg(long)]
    provider: Option<String>,
    #[arg(long)]
    model: Option<String>,
    #[arg(long)]
    workspace: Option<String>,
    #[arg(long)]
    output: Option<String>,
}

impl From<SetupArgs> for config::SetupOptions {
    fn from(value: SetupArgs) -> Self {
        Self {
            agent: value.agent,
            workload: value.workload,
            provider: value.provider,
            model: value.model,
            workspace: value.workspace,
            output: value.output,
        }
    }
}

#[derive(Debug, Args)]
#[command(group(ArgGroup::new("tape-mode").args(["record", "replay"]).multiple(false)))]
struct RunArgs {
    #[arg(value_name = "CONFIG")]
    path: String,
    #[arg(long)]
    record: bool,
    #[arg(long)]
    replay: bool,
    #[arg(long)]
    cassette: Option<String>,
    #[arg(long)]
    json: bool,
    #[arg(long = "show-output")]
    show_output: bool,
    #[arg(long)]
    measurements: Option<String>,
}

#[derive(Debug, Args)]
#[command(group(ArgGroup::new("tape-mode").args(["record", "replay"]).multiple(false)))]
struct SweepArgs {
    #[arg(value_name = "SWEEP")]
    path: String,
    #[arg(long)]
    record: bool,
    #[arg(long)]
    replay: bool,
    #[arg(long)]
    json: bool,
    #[arg(long = "show-output")]
    show_output: bool,
}

#[derive(Debug, Args)]
struct CompareArgs {
    #[arg(value_name = "SWEEP")]
    path: String,
}
#[derive(Debug, Args)]
struct ProviderArgs {
    #[command(subcommand)]
    command: ProviderCommand,
}
#[derive(Debug, Subcommand)]
enum ProviderCommand {
    Models(ModelArgs),
}
#[derive(Debug, Args)]
struct ToolArgs {
    #[command(subcommand)]
    command: ToolCommand,
}
#[derive(Debug, Subcommand)]
enum ToolCommand {
    List(ToolListArgs),
    Install(ToolInstallArgs),
}
#[derive(Debug, Args, Default)]
struct ToolListArgs {}
#[derive(Debug, Args)]
struct WorkloadArgs {
    #[command(subcommand)]
    command: WorkloadCommand,
}
#[derive(Debug, Subcommand)]
enum WorkloadCommand {
    List(WorkloadListArgs),
    Install(WorkloadInstallArgs),
    Tui,
}
#[derive(Debug, Args, Default)]
struct WorkloadListArgs {
    #[arg(long)]
    dir: Option<PathBuf>,
}
#[derive(Debug, Args)]
struct WorkloadInstallArgs {
    #[arg(value_name = "SOURCE")]
    source: String,
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    dir: Option<PathBuf>,
}
#[derive(Debug, Args)]
struct ModelArgs {
    #[arg(value_name = "PROVIDER")]
    provider: Option<String>,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    free: bool,
}

impl From<ModelArgs> for models::ModelListOptions {
    fn from(value: ModelArgs) -> Self {
        Self {
            provider: value.provider,
            json: value.json,
            free_only: value.free,
        }
    }
}
impl From<ToolInstallArgs> for install::ToolInstallOptions {
    fn from(value: ToolInstallArgs) -> Self {
        Self {
            name: value.name,
            source: value.source,
            dir: value.dir,
            dry_run: value.dry_run,
        }
    }
}
impl From<WorkloadInstallArgs> for install::WorkloadInstallOptions {
    fn from(value: WorkloadInstallArgs) -> Self {
        Self {
            source: value.source,
            name: value.name,
            dir: value.dir,
        }
    }
}
impl From<WorkloadListArgs> for install::WorkloadListOptions {
    fn from(value: WorkloadListArgs) -> Self {
        Self { dir: value.dir }
    }
}
#[derive(Debug, Args)]
#[command(
    after_help = "The cli2key tool is a provider bridge. Install it locally with `asb tool install cli2key`."
)]
struct ToolInstallArgs {
    #[arg(value_name = "TOOL")]
    name: String,
    #[arg(long)]
    source: Option<String>,
    #[arg(long)]
    dir: Option<PathBuf>,
    #[arg(long)]
    dry_run: bool,
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::Parser;

    #[test]
    fn all_setup_options_are_typed_and_order_independent() {
        let cli = Cli::try_parse_from([
            "asb",
            "setup",
            "--model",
            "gpt-6.1-sol",
            "--agent",
            "opencode",
            "--output",
            "benchmark.toml",
            "--provider",
            "cli2key",
            "--workload",
            "bug-fix",
            "--workspace",
            "workspace",
        ])
        .expect("typed setup options parse");
        let rendered = format!("{cli:?}");
        assert!(rendered.contains("gpt-6.1-sol"));
        assert!(rendered.contains("benchmark.toml"));
    }

    #[test]
    fn single_dash_long_options_are_rejected_by_the_common_parser() {
        let error = Cli::try_parse_from(["asb", "setup", "-workload", "bug-fix"])
            .expect_err("single dash long option must be rejected");
        assert!(error.to_string().contains("unexpected argument"));
    }
}
