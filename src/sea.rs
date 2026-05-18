use crate::{
    analyzer_state::{AnalyzerState, OwnershipState},
    cfg::{BlockState, Cfg, Statement, build_cfg},
    diagnostics::{Diagnostic, Severity},
    variable_info::{AllocKind, VariableInfo},
};
use std::path::PathBuf;
use tree_sitter::{Node, Tree};

pub struct Sea {
    source: String,
    tree: Tree,
    file_path: String,
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
            .expect("Error loading C grammar");

        let tree = parser.parse(&source, None).unwrap();
        let file_path = path.to_string_lossy().to_string();

        Sea {
            source,
            tree,
            file_path,
        }
    }

    pub fn analyze(&self, file: &str) -> Vec<Diagnostic> {
        let root = self.tree.root_node();
        let cfg = build_cfg(root, &self.source);
        self.analyze_cfg_linear(&cfg, file)
    }

    pub fn analyze_cfg_linear(&self, cfg: &Cfg, file: &str) -> Vec<Diagnostic> {
        let mut diagnostics: Vec<Diagnostic> = Vec::new();
        let mut state = BlockState::new();

        for index in cfg.node_indices() {
            let block = &cfg[index];
            for stmt in &block.statements {
                match stmt {
                    Statement::Malloc { var, .. } => {
                        state
                            .ownership
                            .insert(var.clone(), VariableInfo::heap(state.scope_depth));
                    }
                    Statement::Free { var, row, col } => {
                        handle_free(var, *row, *col, file, &mut state, &mut diagnostics);
                    }
                    Statement::Deref { var, row, col } => {
                        handle_deref(var, *row, *col, file, &mut state, &mut diagnostics);
                    }
                    Statement::FieldAccess { var, row, col } => {
                        handle_field_access(var, *row, *col, file, &mut state, &mut diagnostics);
                    }
                    Statement::PassToFunction {
                        var,
                        func,
                        row,
                        col,
                    } => {
                        handle_pass_freed_pointer(
                            var,
                            func,
                            *row,
                            *col,
                            file,
                            &mut state,
                            &mut diagnostics,
                        );
                    }
                    Statement::Return {
                        var,
                        row,
                        col,
                        is_address_of,
                    } => {
                        handle_return(
                            var,
                            *row,
                            *col,
                            *is_address_of,
                            file,
                            &mut state,
                            &mut diagnostics,
                        );
                    }
                    Statement::NullAssign { var, .. } => {
                        state
                            .ownership
                            .insert(var.clone(), VariableInfo::null(state.scope_depth));
                    }
                    Statement::AddrAssign { var, points_to, .. } => {
                        let mut info = VariableInfo::stack(state.scope_depth);
                        if !points_to.is_empty() {
                            info.state = OwnershipState::Allocated;
                            info.points_to = Some(points_to.clone());
                        }
                        state.ownership.insert(var.clone(), info);
                    }
                    Statement::EnterScope => state.enter_scope(),
                    Statement::ExitScope { row, col } => {
                        state.exit_scope(file, *row, *col, &mut diagnostics);
                    }
                    Statement::PointerAssign { var, points_to, .. } => {
                        if let Some(info) = state.ownership.get_mut(var) {
                            info.points_to = Some(points_to.clone());
                            info.state = OwnershipState::Allocated;
                        }
                    }
                }
            }
        }
        diagnostics
    }
}

fn handle_free(
    var: &str,
    row: usize,
    col: usize,
    file: &str,
    state: &mut BlockState,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match state.ownership.get(var).map(|v| &v.state) {
        Some(OwnershipState::Freed) => {
            diagnostics.push(Diagnostic {
                file: file.to_string(),
                line: row,
                col,
                message: format!("double free of '{}'", var),
                severity: Severity::Error,
            });
        }
        Some(OwnershipState::Allocated) => {
            if let Some(info) = state.ownership.get_mut(var) {
                info.state = OwnershipState::Freed;
            }
        }
        Some(OwnershipState::Uninitialized) => {
            diagnostics.push(Diagnostic {
                file: file.to_string(),
                line: row,
                col,
                message: format!("free of uninitialized variable: '{}'", var),
                severity: Severity::Error,
            });
        }
        Some(OwnershipState::Null) => {
            diagnostics.push(Diagnostic {
                file: file.to_string(),
                line: row,
                col,
                message: format!("free of 'NULL' pointer '{}'", var),
                severity: Severity::Warning,
            });
        }
        Some(OwnershipState::OutOfScope) => {}
        None => {
            // diagnostics.push(Diagnostic {
            //     file: file.to_string(),
            //     line: row,
            //     col,
            //     message: format!("free of untracked pointer '{}'", var),
            //     severity: Severity::Warning,
            // });
        }
    }
}

fn handle_deref(
    var: &str,
    row: usize,
    col: usize,
    file: &str,
    state: &mut BlockState,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match state.ownership.get(var).map(|v| &v.state) {
        Some(OwnershipState::Freed) => {
            diagnostics.push(Diagnostic {
                file: file.to_string(),
                line: row,
                col,
                message: format!("use after free of '{}'", var),
                severity: Severity::Error,
            });
        }
        Some(OwnershipState::Uninitialized) => {
            diagnostics.push(Diagnostic {
                file: file.to_string(),
                line: row,
                col,
                message: format!("use of uninitialized pointer '{}'", var),
                severity: Severity::Error,
            });
        }
        Some(OwnershipState::Null) => {
            diagnostics.push(Diagnostic {
                file: file.to_string(),
                line: row,
                col,
                message: format!("null pointer dereference of '{}'", var),
                severity: Severity::Error,
            });
        }
        Some(OwnershipState::Allocated) => {}
        Some(OwnershipState::OutOfScope) => {}
        None => {
            diagnostics.push(Diagnostic {
                file: file.to_string(),
                line: row,
                col,
                message: format!("use of uninitialized pointer '{}'", var),
                severity: Severity::Error,
            });
        }
    }
}

fn handle_field_access(
    var: &str,
    row: usize,
    col: usize,
    file: &str,
    state: &mut BlockState,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match state.ownership.get(var).map(|v| &v.state) {
        Some(OwnershipState::Freed) => {
            diagnostics.push(Diagnostic {
                file: file.to_string(),
                line: row,
                col,
                message: format!("use of field of freed pointer '{}'", var),
                severity: Severity::Error,
            });
        }
        Some(OwnershipState::Null) => {
            diagnostics.push(Diagnostic {
                file: file.to_string(),
                line: row,
                col,
                message: format!("null pointer dereference via field access '{}'", var),
                severity: Severity::Error,
            });
        }
        Some(OwnershipState::Uninitialized) => {
            diagnostics.push(Diagnostic {
                file: file.to_string(),
                line: row,
                col,
                message: format!("field access on uninitialized pointer '{}'", var),
                severity: Severity::Error,
            });
        }
        _ => {}
    }
}

fn handle_pass_freed_pointer(
    var: &str,
    func: &str,
    row: usize,
    col: usize,
    file: &str,
    state: &mut BlockState,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match state.ownership.get(var).map(|v| &v.state) {
        Some(OwnershipState::Freed) => {
            diagnostics.push(Diagnostic {
                file: file.to_string(),
                line: row,
                col,
                message: format!("passing freed pointer '{}' to function '{}'", var, func),
                severity: Severity::Error,
            });
        }
        Some(OwnershipState::Uninitialized) => {
            diagnostics.push(Diagnostic {
                file: file.to_string(),
                line: row,
                col,
                message: format!("passing uninitialized pointer '{}' to function", var),
                severity: Severity::Warning,
            });
        }
        Some(OwnershipState::Null) => {
            diagnostics.push(Diagnostic {
                file: file.to_string(),
                line: row,
                col,
                message: format!("passing null pointer '{}' to function", var),
                severity: Severity::Warning,
            });
        }
        _ => {}
    }
}

fn handle_return(
    var: &str,
    row: usize,
    col: usize,
    is_address_of: bool,
    file: &str,
    state: &mut BlockState,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if is_address_of {
        match state.ownership.get(var) {
            Some(info) if info.alloc_kind == AllocKind::Stack => {
                let message = format!(
                    "returning address of stack variable '{}' which will be invalid after function returns",
                    var
                );
                diagnostics.push(Diagnostic {
                    file: file.to_string(),
                    line: row,
                    col,
                    message,
                    severity: Severity::Error,
                });
            }
            _ => {}
        }
    } else {
        match state.ownership.get(var).map(|v| &v.state) {
            Some(OwnershipState::Freed) => {
                diagnostics.push(Diagnostic {
                    file: file.to_string(),
                    line: row,
                    col,
                    message: format!("returning freed pointer '{}' from function", var),
                    severity: Severity::Error,
                });
            }
            _ => {}
        }
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
                    "malloc" => handle_malloc_old(node, source, state),
                    "free" => handle_free_old(node, source, file, state),
                    _ => handle_pass_freed_pointer_old(node, source, file, state),
                }
            }
        }
        "pointer_expression" => handle_dereference_old(node, source, file, state),
        "return_statement" => handle_return_old(node, source, file, state),
        "field_expression" => handle_field_expression_old(node, source, file, state),
        "declaration" => handle_declaration(node, source, state),
        "assignment_expression" => handle_assignment(node, source, state),
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

fn handle_field_expression_old(node: Node, source: &str, file: &str, state: &mut AnalyzerState) {
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

fn handle_return_old(node: Node, source: &str, file: &str, state: &mut AnalyzerState) {
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
            state.report(file, line, col,
                &format!("returning address of stack variable '{}' which will be invalid after function returns", var_name),
                Severity::Error,
            );
        }
        _ => {}
    }
}

fn handle_pass_freed_pointer_old(node: Node, source: &str, file: &str, state: &mut AnalyzerState) {
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

fn handle_dereference_old(node: Node, source: &str, file: &str, state: &mut AnalyzerState) {
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
        Some(OwnershipState::Uninitialized) => {
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
        Some(OwnershipState::OutOfScope) => {}
        Some(OwnershipState::Allocated) => {}
        None => {}
        _ => {}
    }
}

fn handle_malloc_old(node: Node, source: &str, state: &mut AnalyzerState) {
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

fn handle_free_old(node: Node, source: &str, file: &str, state: &mut AnalyzerState) {
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
                Some(OwnershipState::OutOfScope) => {}
                None => {
                    state.report(
                        file,
                        line,
                        col,
                        &format!("free of untracked pointer '{}'", var_name),
                        Severity::Warning,
                    );
                }
                _ => {}
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
