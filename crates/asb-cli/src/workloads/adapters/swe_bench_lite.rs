use super::swe_bench::SweBenchAdapter;
pub(super) static ADAPTER: SweBenchAdapter = SweBenchAdapter {
    id: "swe-bench-lite",
    dataset: "SWE-bench/SWE-bench_Lite",
    revision: "b0dde1093fe417d83b7184254edf8199c1f0dff5",
    expected_tasks: 300,
};
