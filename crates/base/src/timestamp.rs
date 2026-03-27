/// Represents a monotonic timestamp in nanoseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Timestamp(u128);

impl Timestamp {
    /// Creates a new timestamp from a nanosecond value.
    pub const fn new(nanos: u128) -> Self {
        Self(nanos)
    }

    /// Returns the stored nanosecond value.
    pub const fn as_nanos(self) -> u128 {
        self.0
    }
}
