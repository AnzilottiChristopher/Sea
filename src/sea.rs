use std::path::PathBuf;

use tree_sitter::{Node, Tree};

use crate::{
    analyzer_state::{AnalyzerState, OwnershipState},
    diagnostics::{Diagnostic, Severity},
};

pub struct Sea {
    source: String,
    tree: Tree,
}

impl Sea {
    pub fn new(path: &PathBuf) -> Self {
        let source = std::fs::read_to_string(path).unwrap_or_else(|e| {
            eprintln!("Error reading {:?}: {}", path, e);
            std::process::exit(1);
        });

        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_c::LANGUAGE.into())
            .expect("Error loading C grammer");

        let tree = parser.parse(&source, None).unwrap();

        Sea { source, tree }
    }

    pub fn analyze(&self, file: &str) -> Vec<Diagnostic> {
        let mut state = AnalyzerState::new();
        let root = self.tree.root_node();

        walk(root, &self.source, file, &mut state);

        state.diagnostics
    }
}

fn walk(node: Node, source: &str, file: &str, state: &mut AnalyzerState) {
    check_node(node, source, file, state);

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i as u32) {
            walk(child, source, file, state);
        }
    }
}

fn check_node(node: Node, source: &str, file: &str, state: &mut AnalyzerState) {
    if node.kind() == "call_expression" {
        if let Some(func) = node.child_by_field_name("function") {
            let name = &source[func.start_byte()..func.end_byte()];

            match name {
                "malloc" => handle_malloc(node, source, state),
                "free" => handle_free(node, source, file, state),
                _ => {}
            }
        }
    }
}

fn handle_malloc(node: Node, source: &str, state: &mut AnalyzerState) {
    let init_declarator = match node.parent() {
        Some(p) if p.kind() == "init_declarator" => p,
        _ => {
            println!(
                "debug malloc: parent was not init_declarator, was {:?}",
                node.parent().map(|p| p.kind())
            );
            return;
        }
    };

    let declarator = match init_declarator.child_by_field_name("declarator") {
        Some(d) => d,
        None => {
            println!("debug malloc: no identifier found");
            return;
        }
    };

    let var_name = match get_identifier(declarator, source) {
        Some(name) => name,
        None => return,
    };

    match state.ownership.get(var_name.as_str()) {
        Some(OwnershipState::Freed) => {
            println!(" IN HERE: {}", var_name);
            state.ownership.insert(var_name, OwnershipState::Allocated);
        }
        //TODO add the rest of the cases
        _ => {
            state.ownership.insert(var_name, OwnershipState::Allocated);
        }
    }
}

fn handle_free(node: Node, source: &str, file: &str, state: &mut AnalyzerState) {
    if let Some(args) = node.child_by_field_name("arguments") {
        if let Some(arg) = args.named_child(0) {
            let var_name = &source[arg.start_byte()..arg.end_byte()];
            let line = node.start_position().row + 1;
            let col = node.start_position().column;

            match state.ownership.get(var_name) {
                Some(OwnershipState::Freed) => {
                    state.report(
                        file,
                        line,
                        col,
                        &format!("double free of '{}'", var_name),
                        Severity::Error,
                    );
                }
                Some(OwnershipState::Allocated) => {
                    state
                        .ownership
                        .insert(var_name.to_string(), OwnershipState::Freed);
                }
                None => {
                    state.report(
                        file,
                        line,
                        col,
                        &format!("free of untracked pointer '{}'", var_name),
                        Severity::Warning,
                    );
                }
            }
        }
    }
}

fn get_identifier(node: Node, source: &str) -> Option<String> {
    if node.kind() == "identifier" {
        return Some(source[node.start_byte()..node.end_byte()].to_string());
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i as u32) {
            if let Some(name) = get_identifier(child, source) {
                return Some(name);
            }
        }
    }

    None
}
