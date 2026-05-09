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
    match node.kind() {
        "call_expression" => {
            if let Some(func) = node.child_by_field_name("function") {
                let name = &source[func.start_byte()..func.end_byte()];

                match name {
                    "malloc" => handle_malloc(node, source, state),
                    "free" => handle_free(node, source, file, state),
                    _ => {
                        handle_pass_freed_pointer(node, source, file, state);
                    }
                }
            }
        }
        "pointer_expression" => {
            handle_dereference(node, source, file, state);
        }
        "return_statement" => {
            handle_return(node, source, file, state);
        }
        _ => {}
    }
}

fn handle_return(node: Node, source: &str, file: &str, state: &mut AnalyzerState) {
    if let Some(expr) = node.named_child(0) {
        check_return_expr(expr, source, file, node, state);
    }
}

fn check_return_expr(
    node: Node,
    source: &str,
    file: &str,
    return_node: Node,
    state: &mut AnalyzerState,
) {
    match node.kind() {
        "identifier" => {
            let var_name = &source[node.start_byte()..node.end_byte()];
            let line = return_node.start_position().row + 1;
            let col = return_node.start_position().column;

            match state.ownership.get(var_name) {
                Some(OwnershipState::Freed) => {
                    state.report(
                        file,
                        line,
                        col,
                        &format!("returning freed pointer '{}' from function", var_name),
                        Severity::Error,
                    );
                }
                _ => {}
            }
        }
        "conditional_expression" => {
            // return p ? x : y
            // check all three branches — condition, true, false
            for i in 0..node.named_child_count() {
                if let Some(child) = node.named_child(i as u32) {
                    check_return_expr(child, source, file, return_node, state);
                }
            }
        }
        // pointer_expression and call_expression are
        // handled by their own handlers already
        _ => {}
    }
}

fn handle_pass_freed_pointer(node: Node, source: &str, file: &str, state: &mut AnalyzerState) {
    let line = node.start_position().row + 1;
    let col = node.start_position().column;

    if let Some(args) = node.child_by_field_name("arguments") {
        for i in 0..args.named_child_count() {
            if let Some(arg) = args.named_child(i as u32) {
                if let Some(var_name) = get_identifier(arg, source) {
                    match state.ownership.get(var_name.as_str()) {
                        Some(OwnershipState::Freed) => {
                            state.report(
                                file,
                                line,
                                col,
                                &format!("passing freed pointer '{}' to function", var_name),
                                Severity::Error,
                            );
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

fn handle_dereference(node: Node, source: &str, file: &str, state: &mut AnalyzerState) {
    let var_name = match node.named_child(0) {
        Some(child) => &source[child.start_byte()..child.end_byte()],
        None => return,
    };

    let line = node.start_position().row + 1;
    let col = node.start_position().column;

    match state.ownership.get(var_name) {
        Some(OwnershipState::Freed) => {
            state.report(
                file,
                line,
                col,
                &format!("use after free of '{}'", var_name),
                Severity::Error,
            );
        }
        Some(OwnershipState::Allocated) => {
            // variable is not in hashmap and still alive
            // dereference is fine
        }
        Some(OwnershipState::Uninitialized) => {
            //TODO: when use before init is implemented
            //this will report use of uninitialized pointer
            state.report(
                file,
                line,
                col,
                &format!("use of uninitialized pointer '{}'", var_name),
                Severity::Error,
            );
        }
        None => {
            // variable is not in hashmap
            // not a heap pointer, not our concern
        }
    }
}

fn handle_malloc(node: Node, source: &str, state: &mut AnalyzerState) {
    let parent = match node.parent() {
        Some(p) => p,
        None => return,
    };

    let var_name = match parent.kind() {
        "init_declarator" => {
            let declarator = match parent.child_by_field_name("declarator") {
                Some(d) => d,
                None => return,
            };
            match get_identifier(declarator, source) {
                Some(name) => name,
                None => return,
            }
        }
        "assignment_expression" => {
            let left = match parent.child_by_field_name("left") {
                Some(l) => l,
                None => return,
            };
            source[left.start_byte()..left.end_byte()].to_string()
        }
        _ => return,
    };

    match state.ownership.get(var_name.as_str()) {
        Some(OwnershipState::Freed) => {
            state.ownership.insert(var_name, OwnershipState::Allocated);
        }
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
                Some(OwnershipState::Uninitialized) => {
                    state.report(
                        file,
                        line,
                        col,
                        &format!("free of uninitialized variable: '{}'", var_name),
                        Severity::Error,
                    );
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
