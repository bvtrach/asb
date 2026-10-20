//! Peak aggregate RSS of selected agent processes, excluding tools.
pub(super) static ADAPTER: super::agent_processes::AgentProcesses<true> =
    super::agent_processes::AgentProcesses;
