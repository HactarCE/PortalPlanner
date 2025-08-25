use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(1);

/// Monotonically-increasing unique ID.
///
/// ID 0 is reserved for "new generated portal".
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct PortalId(u64);
impl PortalId {
    pub const NEW_GENERATED_PORTAL: Self = Self(0);

    /// Returns a new unique ID. Panics on overflow.
    pub fn new() -> Self {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        assert_ne!(id, 0, "ID overflow");
        Self(id)
    }
}
