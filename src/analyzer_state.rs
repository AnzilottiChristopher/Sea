#[derive(Debug, PartialEq, Clone)]
pub enum OwnershipState {
    Allocated,
    Freed,
    Uninitialized,
    Null,
    OutOfScope,
    MaybeFreed,
}
