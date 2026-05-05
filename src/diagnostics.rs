pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    pub fn as_str(&self) -> &str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

pub struct Diagnostic {
    pub file: String,
    pub line: usize,
    pub col: usize,
    pub message: String,
    pub severity: Severity,
}

impl Diagnostic {
    pub fn display(&self) {
        println!(
            "{}:{}:{}: {}: {}",
            self.file,
            self.line,
            self.col,
            self.severity.as_str(),
            self.message
        );
    }
}
