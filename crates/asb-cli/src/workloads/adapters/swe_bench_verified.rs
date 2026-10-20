use super::swe_bench::SweBenchAdapter;
pub(super) static ADAPTER: SweBenchAdapter = SweBenchAdapter {
    id: "swe-bench-verified",
    dataset: "SWE-bench/SWE-bench_Verified",
    revision: "78f471bf655a3137b2e8a75af1501690ec009ec3",
    expected_tasks: 500,
};
