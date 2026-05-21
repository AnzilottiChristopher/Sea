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
}
