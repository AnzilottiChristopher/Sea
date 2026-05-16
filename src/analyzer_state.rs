use crate::{
    diagnostics::{Diagnostic, Severity},
    variable_info::VariableInfo,
};
use std::collections::HashMap;

#[derive(Debug, PartialEq, Clone)]
pub enum OwnershipState {
    Allocated,
    Freed,
    Uninitialized,
    Null,
    OutOfScope,
}

pub struct AnalyzerState {
    pub ownership: HashMap<String, VariableInfo>,
    pub diagnostics: Vec<Diagnostic>,
    pub scope_depth: usize,
}

impl AnalyzerState {
    pub fn new() -> Self {
        AnalyzerState {
            ownership: HashMap::new(),
            diagnostics: Vec::new(),
            scope_depth: 0,
        }
    }

    pub fn enter_scope(&mut self) {
        self.scope_depth += 1;
    }
    pub fn exit_scope(&mut self, file: &str, line: usize, col: usize) {
        let dying: Vec<String> = self
            .ownership
            .iter()
            .filter(|(_, info)| info.scope_depth == self.scope_depth)
            .map(|(name, _)| name.clone())
            .collect();

        let dangling: Vec<String> = self
            .ownership
            .iter()
            .filter(|(_, info)| {
                if let Some(ref target) = info.points_to {
                    info.scope_depth < self.scope_depth && dying.contains(target)
                } else {
                    false
                }
            })
            .map(|(name, _)| name.clone())
            .collect();

        for ptr_name in &dangling {
            if let Some(ptr_info) = self.ownership.get(ptr_name) {
                if let Some(ref target) = ptr_info.points_to.clone() {
                    self.diagnostics.push(Diagnostic {
                        file: file.to_string(),
                        line,
                        col,
                        message: format!(
                            "pointer '{}' will outlive '{}' which it points to",
                            ptr_name, target
                        ),
                        severity: Severity::Error,
                    });
                }
            }
            if let Some(info) = self.ownership.get_mut(ptr_name) {
                info.state = OwnershipState::OutOfScope;
            }
        }

        self.ownership
            .retain(|_, info| info.scope_depth < self.scope_depth);
        self.scope_depth -= 1;
    }

    pub fn report(
        &mut self,
        file: &str,
        line: usize,
        col: usize,
        message: &str,
        severity: Severity,
    ) {
        self.diagnostics.push(Diagnostic {
            file: file.to_string(),
            line,
            col,
            message: message.to_string(),
            severity,
        });
    }
}
