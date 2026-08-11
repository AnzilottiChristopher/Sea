use crate::analyzer_state::OwnershipState;

#[derive(Debug, PartialEq, Clone)]
pub enum AllocKind {
    Heap,
    Stack,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VariableInfo {
    pub state: OwnershipState,
    pub scope_depth: usize,
    pub alloc_kind: AllocKind,
    pub points_to: Option<String>,
}
impl VariableInfo {
    pub fn heap(scope_depth: usize) -> Self {
        VariableInfo {
            state: OwnershipState::Allocated,
            scope_depth,
            alloc_kind: AllocKind::Heap,
            points_to: None,
        }
    }
    pub fn stack(scope_depth: usize) -> Self {
        VariableInfo {
            state: OwnershipState::Uninitialized,
            scope_depth,
            alloc_kind: AllocKind::Stack,
            points_to: None,
        }
    }
    pub fn null(scope_depth: usize) -> Self {
        VariableInfo {
            state: OwnershipState::Null,
            scope_depth,
            alloc_kind: AllocKind::Stack,
            points_to: None,
        }
    }

    /// A pointer declared with an initializer we don't specifically model
    /// (e.g. `T *p = (T *)raw;`) — the malloc family and `NULL`/`&x`/plain-
    /// identifier initializers are handled elsewhere and don't reach this
    /// constructor. The declaration leaves `p` initialized and non-null, but
    /// we don't know what it points to, so it's represented the same way as
    /// `param()` (a known-good stack pointer) except scoped normally, since
    /// this is a local, not a parameter that must outlive the whole function.
    pub fn opaque_init(scope_depth: usize) -> Self {
        VariableInfo {
            state: OwnershipState::Allocated,
            scope_depth,
            alloc_kind: AllocKind::Stack,
            points_to: None,
        }
    }

    /// A function parameter: initialized by the caller before the function
    /// body runs, so it must never read as `Uninitialized`. `Allocated` +
    /// `Stack` reuses the existing "known-good pointer" representation
    /// (same as `T *b = &a;`) without tripping the heap-leak check, which
    /// only fires for `AllocKind::Heap`. `scope_depth` is pinned to 0 so it
    /// survives the function body's own scope exit.
    pub fn param() -> Self {
        VariableInfo {
            state: OwnershipState::Allocated,
            scope_depth: 0,
            alloc_kind: AllocKind::Stack,
            points_to: None,
        }
    }
}
