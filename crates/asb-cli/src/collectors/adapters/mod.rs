//! Individual collector adapters.

use super::CollectorAdapter;

pub(crate) mod bpftrace;
pub(crate) mod flamegraph;
pub(crate) mod iostat;
pub(crate) mod mpstat;
pub(crate) mod perf;
pub(crate) mod perfetto;
mod pidstat;
mod runtime;
mod steps;
mod success_rate;
mod system;
pub(crate) mod token_usage;

pub(super) static REGISTRY: &[&dyn CollectorAdapter] = &[
    &system::ADAPTER,
    &runtime::ADAPTER,
    &mpstat::ADAPTER,
    &iostat::ADAPTER,
    &perf::ADAPTER,
    &perfetto::ADAPTER,
    &flamegraph::ADAPTER,
    &bpftrace::ADAPTER,
    &token_usage::ADAPTER,
    &steps::ADAPTER,
    &success_rate::ADAPTER,
];
