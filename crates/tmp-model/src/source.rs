use crate::types::{CoreCpu, Proc, Snapshot};

/// One poll's worth of telemetry. Each part can fail on its own, the same way
/// the native DLL's queries do. [`Model::apply`](crate::Model::apply) keeps the
/// last good value for whatever failed.
#[derive(Clone, Debug)]
pub struct Sample {
    pub snap: Result<Snapshot, String>,
    /// `None` when the process list query failed.
    pub procs: Option<Vec<Proc>>,
    /// Empty when the per-core query failed.
    pub per_core: Vec<CoreCpu>,
}

/// Anything that can be polled for telemetry.
pub trait Source {
    fn sample(&mut self) -> Sample;
}
