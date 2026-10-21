//! Shell completions generated from the authoritative Clap command tree.

use crate::{Error, io_error};
use clap_complete::Shell;
use std::io::Write;

/// Write Bash completions without starting project discovery or any providers.
pub(crate) fn bash(output: &mut dyn Write) -> Result<(), Error> {
    write(Shell::Bash, output)
}

/// Write Zsh completions from the same schema as the CLI and Bash completions.
pub(crate) fn zsh(output: &mut dyn Write) -> Result<(), Error> {
    write(Shell::Zsh, output)
}

fn generate(shell: Shell, mut command: clap::Command) -> Vec<u8> {
    let mut bytes = if matches!(shell, Shell::Bash) {
        b"# bash completion for asb\n".to_vec()
    } else {
        Vec::new()
    };
    // Upstream expects an infallible writer. Buffer locally, then propagate
    // user output failures instead of panicking on a broken pipe.
    clap_complete::generate(shell, &mut command, "asb", &mut bytes);
    bytes
}

fn write(shell: Shell, output: &mut dyn Write) -> Result<(), Error> {
    output
        .write_all(&generate(shell, crate::parser::completion_command()))
        .map_err(io_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_schema(command: &clap::Command, script: &str) {
        for argument in command.get_arguments().filter(|arg| !arg.is_hide_set()) {
            if let Some(long) = argument.get_long() {
                assert!(
                    script.contains(&format!("--{long}")),
                    "{}: missing --{long}",
                    command.get_name()
                );
            }
            if let Some(short) = argument.get_short() {
                assert!(script.contains(&format!("-{short}")), "missing -{short}");
            }
        }
        for child in command.get_subcommands().filter(|cmd| !cmd.is_hide_set()) {
            assert!(
                script.contains(child.get_name()),
                "missing {}",
                child.get_name()
            );
            assert_schema(child, script);
        }
    }

    #[test]
    fn every_current_public_command_and_option_is_in_both_shells() {
        for shell in [Shell::Bash, Shell::Zsh] {
            let mut command = crate::parser::completion_command();
            command.build();
            let script = String::from_utf8(generate(shell, command.clone())).unwrap();
            assert_schema(&command, &script);
            for tool in crate::install::tool_names() {
                assert!(
                    script.contains(tool),
                    "{shell:?} missing registry tool {tool}"
                );
            }
            for option in [
                "--configure-host",
                "--show-output",
                "--respect-record-timings",
                "--import-grades",
                "--grade",
                "--inspect-grade",
            ] {
                assert!(script.contains(option), "{shell:?} missing {option}");
            }
            assert!(!script.contains("--__generate-completion"));
        }
    }

    #[test]
    fn new_commands_and_options_do_not_need_a_second_completion_definition() {
        let command = crate::parser::completion_command().subcommand(
            clap::Command::new("future-command")
                .arg(clap::Arg::new("future-flag").long("future-flag")),
        );
        for shell in [Shell::Bash, Shell::Zsh] {
            let script = String::from_utf8(generate(shell, command.clone())).unwrap();
            assert!(script.contains("future-command"));
            assert!(script.contains("--future-flag"));
        }
    }

    #[test]
    fn both_completion_outputs_propagate_writer_failure() {
        struct Broken;
        impl Write for Broken {
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
        assert!(bash(&mut Broken).is_err());
        assert!(zsh(&mut Broken).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn bash_completes_each_command_context_with_its_actual_flags() {
        fn visit(command: &clap::Command, path: Vec<String>, script: &str) {
            let mut child = std::process::Command::new("bash");
            child.env_clear();
            if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
                child.env("LLVM_PROFILE_FILE", profile);
            }
            let result = child
                .args(["--noprofile", "--norc", "-c"])
                .arg(format!("{script}\nCOMP_WORDS=(\"$@\"); COMP_CWORD=$(($# - 1)); _asb asb --; printf '%s\\n' \"${{COMPREPLY[@]}}\""))
                .arg("completion-fixture").args(&path).arg("--")
                .output().unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            let candidates = String::from_utf8(result.stdout).unwrap();
            for argument in command.get_arguments().filter(|arg| !arg.is_hide_set()) {
                if let Some(long) = argument.get_long() {
                    assert!(
                        candidates
                            .lines()
                            .any(|candidate| candidate == format!("--{long}")),
                        "{path:?} missing --{long}: {candidates}"
                    );
                }
            }
            for child in command.get_subcommands().filter(|cmd| !cmd.is_hide_set()) {
                let mut next = path.clone();
                next.push(child.get_name().into());
                visit(child, next, script);
            }
        }
        let mut command = crate::parser::completion_command();
        command.build();
        let script = String::from_utf8(generate(Shell::Bash, command.clone())).unwrap();
        visit(&command, vec!["asb".into()], &script);
    }
}
