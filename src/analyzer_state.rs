use crate::diagnostics::{Diagnostic, Severity};
use std::collections::HashMap;

#[derive(Debug, PartialEq, Clone)]
pub enum OwnershipState {
    Allocated,
    Freed,
    Uninitialized,
}

pub struct AnalyzerState {
    pub ownership: HashMap<String, OwnershipState>,
    pub diagnostics: Vec<Diagnostic>,
}

impl AnalyzerState {
    pub fn new() -> Self {
        AnalyzerState {
            ownership: HashMap::new(),
            diagnostics: Vec::new(),
        }
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
