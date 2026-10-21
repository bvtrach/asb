//! Bounded semantic progress, selected by the phase that actually executes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum ProgressStage {
    Preparing = 1,
    Dependencies,
    Downloading,
    Copying,
    Extracting,
    Configuring,
    Building,
    InstallingPackage,
    Validating,
    DownloadingAndValidating,
    Finalizing,
    Ready,
    CheckingHost,
}

impl ProgressStage {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Preparing => "preparing",
            Self::Dependencies => "installing dependencies",
            Self::Downloading => "downloading",
            Self::Copying => "copying",
            Self::Extracting => "extracting",
            Self::Configuring => "configuring",
            Self::Building => "building",
            Self::InstallingPackage => "installing package",
            Self::Validating => "validating",
            Self::DownloadingAndValidating => "downloading and validating",
            Self::Finalizing => "finalizing",
            Self::Ready => "ready",
            Self::CheckingHost => "checking host tools",
        }
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn from_code(code: u8) -> Option<Self> {
        Some(match code {
            1 => Self::Preparing,
            2 => Self::Dependencies,
            3 => Self::Downloading,
            4 => Self::Copying,
            5 => Self::Extracting,
            6 => Self::Configuring,
            7 => Self::Building,
            8 => Self::InstallingPackage,
            9 => Self::Validating,
            10 => Self::DownloadingAndValidating,
            11 => Self::Finalizing,
            12 => Self::Ready,
            13 => Self::CheckingHost,
            _ => return None,
        })
    }
}
