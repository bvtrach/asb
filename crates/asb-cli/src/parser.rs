//! The single typed argument parser for the `asb` executable.

use super::{
    CliError, Error, completion, config, install, models, project, runner, sweep, tape, tui,
};
use clap::{ArgGroup, Args, CommandFactory, Parser, Subcommand};
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::PathBuf;

/// Use the parser's schema for completions. Tool suggestions come from their
/// registry without restricting actual parsing of explicit custom tools.
pub(crate) fn completion_command() -> clap::Command {
    Cli::command().mut_subcommand("tool", |tool| {
        tool.mut_subcommand("install", |install| {
            install.mut_arg("name", |name| {
                name.value_parser(clap::builder::PossibleValuesParser::new(
                    install::tool_names(),
                ))
            })
        })
    })
}

pub(crate) fn run(args: &[OsString], stdout: &mut dyn Write, stderr: &mut dyn Write) -> u8 {
    // Like the native gate helpers, completion generation is an internal entry
    // point, not a public option to duplicate in generated shell candidates.
    const COMPLETION_FLAG: &str = "--__generate-completion";
    let separate = args.first().is_some_and(|arg| arg == COMPLETION_FLAG);
    let joined = args
        .first()
        .and_then(|arg| arg.to_str())
        .and_then(|arg| arg.strip_prefix(COMPLETION_FLAG))
        .and_then(|suffix| suffix.strip_prefix('='));
    if separate || joined.is_some() {
        if args.len() != if separate { 2 } else { 1 } {
            return fail(
                stderr,
                "completion helper requires exactly one shell: bash or zsh",
            );
        }
        let shell = if separate { args[1].to_str() } else { joined };
        let result = match shell {
            Some("bash") => completion::bash(stdout),
            Some("zsh") => completion::zsh(stdout),
            _ => Err(Error::Input("completion shell must be bash or zsh".into())),
        };
        return result.map_or_else(|error| fail(stderr, &error.to_string()), |_| 0);
    }
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
    let result = match cli.command {
        None => stdout
            .write_all(Cli::command().render_help().to_string().as_bytes())
            .map_err(io_error),
        Some(CommandLine::Init(options)) => {
            if options.configure_host {
                project::configure_host(options.root, options.yes, stdout)
            } else {
                project::init(options.root, options.reset, options.yes, stdout)
            }
        }
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
            if options.respect_record_timings && mode != Some(tape::Mode::Replay) {
                return fail(
                    stderr,
                    "--respect-record-timings requires --replay; recording always stores response delays",
                );
            }
            runner::run_with_tape(
                &options.path,
                runner::RunOptions {
                    json: options.json,
                    show_output: options.show_output,
                    measurement_dir: options.measurements.as_deref(),
                    mode,
                    cassette: options.cassette.as_deref(),
                    respect_record_timings: options.respect_record_timings,
                    ..Default::default()
                },
                stdout,
            )
        }
        Some(CommandLine::Sweep(options)) => {
            let mode = match tape_mode(options.record, options.replay) {
                Ok(mode) => mode,
                Err(error) => return fail(stderr, &error.to_string()),
            };
            if options.respect_record_timings && mode != Some(tape::Mode::Replay) {
                return fail(
                    stderr,
                    "--respect-record-timings requires --replay; recording always stores response delays",
                );
            }
            sweep::run_with_options(
                &options.path,
                sweep::Options {
                    json: options.json,
                    show_output: options.show_output,
                    mode,
                    respect_record_timings: options.respect_record_timings,
                    cassette_root: options.cassette_root.as_deref().map(std::path::Path::new),
                    jobs: usize::from(options.jobs),
                    max_active_attempts: options.max_active_attempts.map(usize::from),
                    resume: options.resume,
                    reconcile_completed: options.reconcile_completed,
                    import_grades: options.import_grades,
                },
                stdout,
            )
        }
        Some(CommandLine::Compare(options)) => super::compare::generate(&options.path, stdout)
            .and_then(|report| {
                if options.report {
                    super::compare::open_report(&report)
                } else {
                    Ok(())
                }
            }),
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
            WorkloadCommand::Recover(args) => {
                if args.grade {
                    crate::workloads::recovery::grade_retained(&args.reference, args.json, stdout)
                } else if args.inspect_grade {
                    crate::workloads::recovery::inspect_grade(&args.reference, args.json, stdout)
                } else {
                    crate::workloads::recovery::run(&args.reference, args.json, stdout)
                }
            }
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
        "workload" => (
            args.get(1)?.to_str()?,
            &["list", "install", "tui", "recover"],
        ),
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
    /// Update optional host capabilities without resetting project data.
    #[arg(long, conflicts_with = "reset")]
    configure_host: bool,
    /// Approve every privileged host-setup questionnaire without prompting.
    #[arg(long, short = 'y')]
    yes: bool,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum SetupMode {
    Tui,
}

#[derive(Debug, Args)]
#[command(
    after_help = "Selection options accept the reserved keyword 'all'. Models/trials also accept PROVIDER=all or WORKLOAD=all. --model all queries the selected providers; other selectors use local catalogs. Paths and concurrency do not accept 'all'."
)]
struct SetupArgs {
    #[arg(value_enum)]
    mode: Option<SetupMode>,
    /// Agent ID; repeat for a sweep.
    #[arg(long)]
    agent: Vec<String>,
    /// Workload ID; repeat for a sweep.
    #[arg(long)]
    workload: Vec<String>,
    /// Repeatable instance ID; optionally WORKLOAD=INSTANCE for scoped trials.
    #[arg(long, value_name = "INSTANCE")]
    trial: Vec<String>,
    /// Repeatable trial-set TOML; includes all enabled trials unless --trial is given.
    #[arg(long, value_name = "FILE")]
    trial_set: Vec<String>,
    /// Provider ID; repeat for a sweep.
    #[arg(long)]
    provider: Vec<String>,
    /// Repeat to select models; use PROVIDER=MODEL to scope a model to one provider.
    #[arg(long)]
    model: Vec<String>,
    /// Repeat to select collectors (alias: --collector). Dependencies are included.
    #[arg(long, alias = "collector")]
    measure: Vec<String>,
    #[arg(long, value_name = "N")]
    concurrency: Option<usize>,
    #[arg(long)]
    workspace: Option<String>,
    /// Benchmark TOML for one selection; sweep TOML for repeated selections.
    #[arg(long)]
    output: Option<String>,
}

impl From<SetupArgs> for config::SetupOptions {
    fn from(value: SetupArgs) -> Self {
        Self {
            agent: value.agent,
            workload: value.workload,
            trial: value.trial,
            trial_set: value.trial_set,
            provider: value.provider,
            model: value.model,
            measure: value.measure,
            concurrency: value.concurrency,
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
    #[arg(long, help = "replay each response after its recorded response delay")]
    respect_record_timings: bool,
    #[arg(long)]
    cassette: Option<String>,
    #[arg(long)]
    json: bool,
    #[arg(long = "show-output", conflicts_with = "json")]
    show_output: bool,
    #[arg(long)]
    measurements: Option<String>,
}

#[derive(Debug, Args)]
#[command(group(ArgGroup::new("tape-mode").args(["record", "replay"]).multiple(false)))]
struct SweepArgs {
    #[arg(value_name = "SWEEP")]
    path: String,
    /// Import existing official grades for reporting only; never run an agent or evaluator.
    #[arg(long, conflicts_with_all = ["resume", "reconcile_completed", "record", "replay", "show_output", "respect_record_timings", "cassette_root", "max_active_attempts"])]
    import_grades: bool,
    /// Reuse validated completed groups and run untouched configurations; incomplete groups require recovery.
    #[arg(long)]
    resume: bool,
    #[arg(
        long,
        requires = "resume",
        help = "validate retained completed grades and repair missing receipts; never retry an agent"
    )]
    reconcile_completed: bool,
    #[arg(long)]
    record: bool,
    #[arg(long)]
    replay: bool,
    #[arg(long, help = "replay each response after its recorded response delay")]
    respect_record_timings: bool,
    #[arg(long)]
    json: bool,
    #[arg(
        long = "show-output",
        conflicts_with = "json",
        help = "show agent output; parallel jobs buffer it by configuration"
    )]
    show_output: bool,
    /// Separate cassette root for recording, or the record root when replaying.
    #[arg(long, value_name = "DIR")]
    cassette_root: Option<String>,
    /// Maximum distinct configurations running at once (not identical replicas).
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u16).range(1..=32))]
    jobs: u16,
    /// Total simultaneous replicas across all jobs; defaults to max(jobs, largest concurrency).
    #[arg(long, value_parser = clap::value_parser!(u16).range(1..=64))]
    max_active_attempts: Option<u16>,
}

#[derive(Debug, Args)]
struct CompareArgs {
    #[arg(value_name = "BENCHMARK_OR_SWEEP")]
    path: String,
    /// Open the generated HTML report in the default web browser.
    #[arg(long)]
    report: bool,
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
    #[command(
        after_help = "By default, stops only a departed controller's recorded task containers, preserving partial files and volumes. --inspect-grade instead reads an existing official grade without changing the attempt or running an agent/evaluator. Neither operation resumes a sweep."
    )]
    Recover(WorkloadRecoverArgs),
}
#[derive(Debug, Args)]
struct WorkloadRecoverArgs {
    #[arg(value_name = "REFERENCE")]
    reference: PathBuf,
    /// Grade a retained unfinished submission with its official evaluator; never rerun the agent.
    #[arg(long, conflicts_with = "inspect_grade")]
    grade: bool,
    /// Read an existing official grade without running an agent/evaluator or changing the attempt.
    #[arg(long)]
    inspect_grade: bool,
    #[arg(long)]
    json: bool,
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
    after_help = "Install tools, including provider bridge tools. List available tools with `asb tool list`."
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
    fn internal_completion_forms_errors_and_writer_failure_are_explicit() {
        for shell in ["bash", "zsh"] {
            for args in [
                vec!["--__generate-completion".into(), shell.into()],
                vec![format!("--__generate-completion={shell}").into()],
            ] {
                let mut stdout = Vec::new();
                let mut stderr = Vec::new();
                assert_eq!(super::run(&args, &mut stdout, &mut stderr), 0);
                assert!(String::from_utf8(stdout).unwrap().contains("_asb"));
                assert!(stderr.is_empty());
            }
        }
        for args in [
            vec!["--__generate-completion".into()],
            vec![
                "--__generate-completion".into(),
                "bash".into(),
                "unexpected".into(),
            ],
            vec!["--__generate-completion=bash".into(), "unexpected".into()],
            vec!["--__generate-completion=fish".into()],
            vec!["--__generate-completion=".into()],
            vec!["--__generate-completion".into(), "fish".into()],
        ] {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            assert_eq!(super::run(&args, &mut stdout, &mut stderr), 2);
            assert!(stdout.is_empty());
            assert!(!stderr.is_empty());
        }
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            let args = [
                "--__generate-completion".into(),
                std::ffi::OsString::from_vec(vec![0xff]),
            ];
            assert_eq!(super::run(&args, &mut Vec::new(), &mut Vec::new()), 2);
        }
        struct Broken;
        impl std::io::Write for Broken {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "fixture",
                ))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let args = ["--__generate-completion=bash".into()];
        let mut stderr = Vec::new();
        assert_eq!(super::run(&args, &mut Broken, &mut stderr), 2);
        assert!(!stderr.is_empty());
        // Suggestions must not constrain requests for explicit custom binaries.
        Cli::try_parse_from([
            "asb",
            "tool",
            "install",
            "private-source",
            "--source",
            "fixture-binary",
        ])
        .expect("registry suggestions must not constrain actual custom-tool parsing");
    }

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
            "--trial",
            "one",
            "--trial-set",
            "selected.workload.toml",
        ])
        .expect("typed setup options parse");
        let rendered = format!("{cli:?}");
        assert!(rendered.contains("gpt-6.1-sol"));
        assert!(rendered.contains("benchmark.toml"));
        assert!(rendered.contains("selected.workload.toml"));
    }

    #[test]
    fn single_dash_long_options_are_rejected_by_the_common_parser() {
        let error = Cli::try_parse_from(["asb", "setup", "-workload", "bug-fix"])
            .expect_err("single dash long option must be rejected");
        assert!(error.to_string().contains("unexpected argument"));
    }
}
