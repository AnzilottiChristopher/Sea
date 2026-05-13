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
    pub fn exit_scope(&mut self) {
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
