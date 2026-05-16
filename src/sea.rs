use std::path::PathBuf;

use tree_sitter::{Node, Tree};

use crate::{
    analyzer_state::{AnalyzerState, OwnershipState},
    diagnostics::{Diagnostic, Severity},
    variable_info::{AllocKind, VariableInfo},
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
    if node.kind() == "compound_statement" {
        state.enter_scope();
    }

    check_node(node, source, file, state);

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i as u32) {
            walk(child, source, file, state);
        }
    }

    if node.kind() == "compound_statement" {
        let line = node.end_position().row + 1;
        let col = node.end_position().column;
        state.exit_scope(file, line, col);
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
        "field_expression" => {
            handle_field_expression(node, source, file, state);
        }
        "declaration" => {
            handle_declaration(node, source, state);
        }
        "assignment_expression" => {
            handle_assignment(node, source, state);
        }
        _ => {}
    }
}

fn handle_assignment(node: Node, source: &str, state: &mut AnalyzerState) {
    let left = match node.child_by_field_name("left") {
        Some(l) => l,
        None => return,
    };
    let right = match node.child_by_field_name("right") {
        Some(r) => r,
        None => return,
    };

    if right.kind() != "pointer_expression" {
        return;
    }

    if let Some(op) = right.child(0) {
        let op_text = &source[op.start_byte()..op.end_byte()];
        if op_text != "&" {
            return;
        }
    }

    let ptr_name = source[left.start_byte()..left.end_byte()].to_string();
    let target_name = match get_identifier(right, source) {
        Some(name) => name,
        None => return,
    };

    if let Some(info) = state.ownership.get_mut(ptr_name.as_str()) {
        info.points_to = Some(target_name);
    }
}

fn handle_declaration(node: Node, source: &str, state: &mut AnalyzerState) {
    if let Some(decl) = node.child_by_field_name("declarator") {
        match decl.kind() {
            "init_declarator" => {
                if let Some(value) = decl.child_by_field_name("value") {
                    let value_text = &source[value.start_byte()..value.end_byte()];

                    if value_text == "NULL" {
                        if let Some(var_name) = get_identifier(decl, source) {
                            state
                                .ownership
                                .insert(var_name, VariableInfo::null(state.scope_depth));
                        }
                    } else if let Some(inner) = decl.child_by_field_name("declarator") {
                        match inner.kind() {
                            "identifier" => {
                                let var_name =
                                    source[inner.start_byte()..inner.end_byte()].to_string();
                                state
                                    .ownership
                                    .insert(var_name, VariableInfo::stack(state.scope_depth));
                            }
                            "pointer_declarator" => {
                                if let Some(var_name) = get_identifier(inner, source) {
                                    match value.kind() {
                                        "call_expression" => {
                                            if let Some(func) =
                                                value.child_by_field_name("function")
                                            {
                                                let func_name =
                                                    &source[func.start_byte()..func.end_byte()];
                                                if func_name != "malloc" {
                                                    state.ownership.insert(
                                                        var_name,
                                                        VariableInfo::heap(state.scope_depth),
                                                    );
                                                }
                                            }
                                        }
                                        "pointer_expression" => {
                                            let mut info = VariableInfo::stack(state.scope_depth);
                                            info.state = OwnershipState::Allocated;
                                            if let Some(target) = get_identifier(value, source) {
                                                info.points_to = Some(target);
                                            }
                                            state.ownership.insert(var_name, info);
                                        }
                                        _ => {
                                            state.ownership.insert(
                                                var_name,
                                                VariableInfo::stack(state.scope_depth),
                                            );
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            "pointer_declarator" => {
                if let Some(var_name) = get_identifier(decl, source) {
                    state
                        .ownership
                        .insert(var_name, VariableInfo::stack(state.scope_depth));
                }
            }
            "identifier" => {
                if let Some(var_name) = get_identifier(decl, source) {
                    state
                        .ownership
                        .insert(var_name, VariableInfo::stack(state.scope_depth));
                }
            }
            _ => {}
        }
    }
}

fn handle_field_expression(node: Node, source: &str, file: &str, state: &mut AnalyzerState) {
    if let Some(argument) = node.child_by_field_name("argument") {
        let op = node
            .child_by_field_name("operator")
            .map(|op| &source[op.start_byte()..op.end_byte()]);

        if op == Some("->") {
            let var_name = &source[argument.start_byte()..argument.end_byte()];
            let line = node.start_position().row + 1;
            let col = node.start_position().column;

            match state.ownership.get(var_name).map(|v| &v.state) {
                Some(OwnershipState::Freed) => {
                    state.report(
                        file,
                        line,
                        col,
                        &format!("use of field of freed pointer '{}'", var_name),
                        Severity::Error,
                    );
                }
                Some(OwnershipState::Null) => {
                    state.report(
                        file,
                        line,
                        col,
                        &format!("null pointer dereference via field access '{}'", var_name),
                        Severity::Error,
                    );
                }
                Some(OwnershipState::Uninitialized) => {
                    state.report(
                        file,
                        line,
                        col,
                        &format!("field access on uninitialized pointer '{}'", var_name),
                        Severity::Error,
                    );
                }
                _ => {}
            }
        }
    }
}

fn handle_return(node: Node, source: &str, file: &str, state: &mut AnalyzerState) {
    if let Some(expr) = node.named_child(0) {
        match expr.kind() {
            "identifier" => {
                let var_name = &source[expr.start_byte()..expr.end_byte()];
                let line = node.start_position().row + 1;
                let col = node.start_position().column;

                match state.ownership.get(var_name).map(|v| &v.state) {
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
            "pointer_expression" => {
                handle_return_address_of(expr, source, file, node, state);
            }
            _ => {}
        }
    }
}

fn handle_return_address_of(
    expr: Node,
    source: &str,
    file: &str,
    return_node: Node,
    state: &mut AnalyzerState,
) {
    if let Some(operator) = expr.child(0) {
        let op = &source[operator.start_byte()..operator.end_byte()];
        if op != "&" {
            return;
        }
    }

    let var_name = match get_identifier(expr, source) {
        Some(name) => name,
        None => return,
    };

    let line = return_node.start_position().row + 1;
    let col = return_node.start_position().column;

    match state.ownership.get(var_name.as_str()) {
        Some(info) if info.alloc_kind == AllocKind::Stack => {
            state.report(
                file,
                line,
                col,
                &format!(
                    "returning address of stack variable '{}' which will be invalid after function returns",
                    var_name
                ),
                Severity::Error,
            );
        }
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
                    match state.ownership.get(var_name.as_str()).map(|v| &v.state) {
                        Some(OwnershipState::Freed) => {
                            state.report(
                                file,
                                line,
                                col,
                                &format!("passing freed pointer '{}' to function", var_name),
                                Severity::Error,
                            );
                        }
                        Some(OwnershipState::Uninitialized) => {
                            state.report(
                                file,
                                line,
                                col,
                                &format!(
                                    "passing uninitialized pointer '{}' to function",
                                    var_name
                                ),
                                Severity::Warning,
                            );
                        }
                        Some(OwnershipState::Null) => {
                            state.report(
                                file,
                                line,
                                col,
                                &format!("passing null pointer '{}' to function", var_name),
                                Severity::Warning,
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
    if let Some(operator) = node.child(0) {
        let op = &source[operator.start_byte()..operator.end_byte()];
        if op != "*" {
            return;
        }
    }

    let var_name = match node.named_child(0) {
        Some(child) => &source[child.start_byte()..child.end_byte()],
        None => return,
    };

    let line = node.start_position().row + 1;
    let col = node.start_position().column;

    match state.ownership.get(var_name).map(|v| &v.state) {
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
        Some(OwnershipState::Null) => {
            state.report(
                file,
                line,
                col,
                &format!("null pointer dereference of '{}'", var_name),
                Severity::Error,
            );
        }
        Some(OwnershipState::OutOfScope) => {
            //Checked in exit_scope
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

    match state.ownership.get(var_name.as_str()).map(|v| &v.state) {
        Some(OwnershipState::Freed) => {
            if let Some(info) = state.ownership.get_mut(var_name.as_str()) {
                info.state = OwnershipState::Allocated;
            }
        }
        Some(_) => {
            if let Some(info) = state.ownership.get_mut(var_name.as_str()) {
                info.state = OwnershipState::Allocated;
            }
        }
        None => {
            state
                .ownership
                .insert(var_name, VariableInfo::heap(state.scope_depth));
        }
    }
}

fn handle_free(node: Node, source: &str, file: &str, state: &mut AnalyzerState) {
    if let Some(args) = node.child_by_field_name("arguments") {
        if let Some(arg) = args.named_child(0) {
            let var_name = &source[arg.start_byte()..arg.end_byte()];
            let line = node.start_position().row + 1;
            let col = node.start_position().column;

            match state.ownership.get(var_name).map(|v| &v.state) {
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
                    if let Some(info) = state.ownership.get_mut(var_name) {
                        info.state = OwnershipState::Freed;
                    }
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
                Some(OwnershipState::Null) => {
                    state.report(
                        file,
                        line,
                        col,
                        &format!("free of 'NULL' pointer '{}'", var_name),
                        Severity::Warning,
                    );
                }
                Some(OwnershipState::OutOfScope) => {
                    //TODO Check if this one matters
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
