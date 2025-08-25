use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(1);

/// Monotonically-increasing unique ID.
///
/// ID 0 is reserved.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PortalId(u64);

impl fmt::Display for PortalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

impl PortalId {
    /// Returns a new unique ID. Panics on overflow.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        assert_ne!(id, 0, "ID overflow");
        Self(id)
    }
}
