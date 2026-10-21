//! Stable domain identities; presentation text never controls a selection.
use super::*;

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(super) enum ChoiceKey {
    Agent(String),
    Provider(String),
    Model { provider: String, model: String },
    Prompt(String),
    TrialSet { workload: String, file: PathBuf },
    Collector(String),
    Concurrency(usize),
    Trial(String),
}

impl ChoiceKey {
    pub fn valid(&self) -> bool {
        let valid = |name: &str| crate::selection_names::validate(name, "selection").is_ok();
        match self {
            Self::Agent(name)
            | Self::Provider(name)
            | Self::Prompt(name)
            | Self::Collector(name)
            | Self::Trial(name) => valid(name),
            Self::TrialSet { workload, .. } => valid(workload),
            Self::Model { provider, model } => valid(provider) && valid(model),
            Self::Concurrency(value) => (1..=crate::benchmark::MAX_CONCURRENCY).contains(value),
        }
    }
    pub fn workload(&self) -> Option<&str> {
        match self {
            Self::Prompt(name) | Self::TrialSet { workload: name, .. } => Some(name),
            _ => None,
        }
    }
    pub fn agent(&self) -> Option<&str> {
        if let Self::Agent(name) = self {
            Some(name)
        } else {
            None
        }
    }
    pub fn provider(&self) -> Option<&str> {
        if let Self::Provider(name) = self {
            Some(name)
        } else {
            None
        }
    }
    pub fn collector(&self) -> Option<&str> {
        if let Self::Collector(name) = self {
            Some(name)
        } else {
            None
        }
    }
    pub fn trial(&self) -> Option<&str> {
        if let Self::Trial(name) = self {
            Some(name)
        } else {
            None
        }
    }
    pub fn model(&self) -> Option<(&str, &str)> {
        if let Self::Model { provider, model } = self {
            Some((provider, model))
        } else {
            None
        }
    }
    pub fn concurrency(&self) -> Option<usize> {
        if let Self::Concurrency(value) = self {
            Some(*value)
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PaneKind {
    Agents,
    Providers,
    Models,
    Workloads,
    Measurements,
    Concurrency,
}

impl PaneKind {
    pub const ORDER: [Self; 6] = [
        Self::Agents,
        Self::Providers,
        Self::Models,
        Self::Workloads,
        Self::Measurements,
        Self::Concurrency,
    ];
    pub fn index(self) -> usize {
        self as usize
    }
    pub fn next(self) -> Option<Self> {
        Self::ORDER.get(self.index() + 1).copied()
    }
    pub fn previous(self) -> Option<Self> {
        self.index().checked_sub(1).map(|index| Self::ORDER[index])
    }
    pub fn installable(self) -> bool {
        matches!(self, Self::Agents | Self::Workloads | Self::Measurements)
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Agents => "agent",
            Self::Providers => "provider",
            Self::Models => "model",
            Self::Workloads => "workload",
            Self::Measurements => "measurement",
            Self::Concurrency => "concurrency",
        }
    }
    pub fn description(self) -> &'static str {
        DESCRIPTIONS[self.index()]
    }
}
