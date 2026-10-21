use super::{StaticTool, ToolKind};
use crate::install::LoggedStatus;
use crate::install::Recipe;
pub(super) static ADAPTER: StaticTool = StaticTool::new(
    "cli2key",
    Recipe::Npm {
        package: "codex-anthropic-bridge@0.1.10",
        executable: "codex-bridge",
    },
    ToolKind::Support,
)
.with_post_install(post_install);

fn post_install(
    executable: &std::path::Path,
    progress: &mut dyn FnMut(u8, crate::install::ProgressStage),
    output: &mut dyn std::io::Write,
) -> Result<(), crate::Error> {
    progress(90, crate::install::ProgressStage::Finalizing);
    let status = std::process::Command::new(executable)
        .args(["key", "refresh"])
        .logged_status()
        .map_err(|error| crate::Error::Io(format!("cannot refresh cli2key API key: {error}")))?;
    if !status.success() {
        return Err(crate::Error::Io(
            "cli2key was installed, but its local API key could not be generated".into(),
        ));
    }
    writeln!(output, "Generated a local cli2key API key for project runs").map_err(crate::io_error)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::tools::ToolAdapter;

    #[test]
    fn adapter_hook_reports_success_and_failure() {
        let mut updates = Vec::new();
        let mut output = Vec::new();
        ADAPTER
            .post_install(
                std::path::Path::new("/bin/true"),
                &mut |percent, message| updates.push((percent, message.to_owned())),
                &mut output,
            )
            .unwrap();
        assert_eq!(updates, [(90, crate::install::ProgressStage::Finalizing)]);
        assert!(
            String::from_utf8(output)
                .unwrap()
                .contains("Generated a local cli2key API key")
        );
        let error = ADAPTER
            .post_install(
                std::path::Path::new("/bin/false"),
                &mut |_, _| {},
                &mut Vec::new(),
            )
            .unwrap_err();
        assert!(error.to_string().contains("could not be generated"));
    }
}
