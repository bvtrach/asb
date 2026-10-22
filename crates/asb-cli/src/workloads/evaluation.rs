//! Known producer categories and lossless classification of extensible history.
use super::session::BaselineEvidence;

/// Internal producer domain, deliberately not a closed persisted wire enum.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum KnownEvaluationKind {
    BundledFixture,
    ResponseReview,
    AdapterSmoke,
    UpstreamHarness,
    Ungraded,
    InvalidBaseline,
}

impl KnownEvaluationKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::BundledFixture => "bundled-fixture",
            Self::ResponseReview => "response-review",
            Self::AdapterSmoke => "adapter-smoke",
            Self::UpstreamHarness => "upstream-harness",
            Self::Ungraded => "ungraded",
            Self::InvalidBaseline => "invalid-baseline",
        }
    }

    pub(crate) const fn resolution_eligible(self) -> bool {
        matches!(
            self,
            Self::BundledFixture | Self::ResponseReview | Self::UpstreamHarness
        )
    }

    pub(crate) fn scored(self, baseline: &BaselineEvidence) -> Self {
        if self.resolution_eligible()
            && !baseline.supports_evaluation(SavedEvaluationKind::Known(self))
        {
            Self::InvalidBaseline
        } else {
            self
        }
    }
}

/// Saved readers preserve unknown spellings and bytes without registration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SavedEvaluationKind<'a> {
    Known(KnownEvaluationKind),
    Extension(&'a str),
}

impl<'a> SavedEvaluationKind<'a> {
    pub(crate) fn classify(value: &'a str) -> Self {
        use KnownEvaluationKind as Kind;
        let known = match value {
            "bundled-fixture" => Kind::BundledFixture,
            "response-review" => Kind::ResponseReview,
            "adapter-smoke" => Kind::AdapterSmoke,
            "upstream-harness" => Kind::UpstreamHarness,
            "ungraded" => Kind::Ungraded,
            "invalid-baseline" => Kind::InvalidBaseline,
            _ => return Self::Extension(value),
        };
        Self::Known(known)
    }

    pub(crate) fn as_str(self) -> &'a str {
        match self {
            Self::Known(kind) => kind.as_str(),
            Self::Extension(value) => value,
        }
    }

    pub(crate) fn resolution_eligible(self) -> bool {
        matches!(self, Self::Known(kind) if kind.resolution_eligible())
    }

    pub(crate) fn scored(self, baseline: &BaselineEvidence) -> Self {
        match self {
            Self::Known(kind) => Self::Known(kind.scored(baseline)),
            extension => extension,
        }
    }
}
