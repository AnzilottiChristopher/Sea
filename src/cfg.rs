use crate::{
    analyzer_state::OwnershipState, diagnostics::Diagnostic, diagnostics::Severity,
    variable_info::VariableInfo,
};
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::HashMap;
use tree_sitter::Node;

pub type Cfg = DiGraph<BasicBlock, ()>;

#[derive(Debug)]
#[allow(dead_code)]
pub enum Statement {
    Malloc {
        var: String,
        row: usize,
        col: usize,
    },
    Free {
        var: String,
        row: usize,
        col: usize,
    },
    Deref {
        var: String,
        row: usize,
        col: usize,
    },
    FieldAccess {
        var: String,
        row: usize,
        col: usize,
    },
    Return {
        var: String,
        row: usize,
        col: usize,
        is_address_of: bool,
    },
    PassToFunction {
        var: String,
        func: String,
        row: usize,
        col: usize,
    },
    NullAssign {
        var: String,
        row: usize,
        col: usize,
    },
    AddrAssign {
        var: String,
        points_to: String,
        row: usize,
        col: usize,
    },
    PointerAssign {
        var: String,
        points_to: String,
        row: usize,
        col: usize,
    },
    EnterScope,
    ExitScope {
        row: usize,
        col: usize,
    },
}

pub struct BasicBlock {
    pub statements: Vec<Statement>,
}

#[derive(Clone)]
pub struct BlockState {
    pub ownership: HashMap<String, VariableInfo>,
    pub scope_depth: usize,
    pub base_scope_depth: usize,
}

impl BlockState {
    pub fn new() -> Self {
        BlockState {
            ownership: HashMap::new(),
            scope_depth: 0,
            base_scope_depth: 0,
        }
    }

    pub fn merge(&self, other: &BlockState) -> BlockState {
        let mut merged = self.clone();
        let all_vars: std::collections::HashSet<&String> = self
            .ownership
            .keys()
            .chain(other.ownership.keys())
            .collect();

        for var in all_vars {
            let state_a = self.ownership.get(var);
            let state_b = other.ownership.get(var);

            let merged_info = match (state_a, state_b) {
                // same on both paths — keep it
                (Some(a), Some(b)) if a.state == b.state => a.clone(),

                // freed on either path — maybe freed
                (Some(a), Some(b))
                    if a.state == OwnershipState::Freed || b.state == OwnershipState::Freed =>
                {
                    let mut info = a.clone();
                    info.state = OwnershipState::MaybeFreed;
                    info
                }

                // maybe freed on either path — maybe freed
                (Some(a), Some(b))
                    if a.state == OwnershipState::MaybeFreed
                        || b.state == OwnershipState::MaybeFreed =>
                {
                    let mut info = a.clone();
                    info.state = OwnershipState::MaybeFreed;
                    info
                }

                // only exists on one path
                (Some(a), None) => a.clone(),
                (None, Some(b)) => b.clone(),

                // different non-freed states — take left
                (Some(a), Some(_)) => a.clone(),

                (None, None) => continue,
            };

            merged.ownership.insert(var.clone(), merged_info);
        }

        merged.scope_depth = self.scope_depth;
        merged.base_scope_depth = self.base_scope_depth;
        merged
    }
    pub fn enter_scope(&mut self) {
        self.scope_depth += 1;
    }

    pub fn exit_scope(
        &mut self,
        file: &str,
        line: usize,
        col: usize,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if self.scope_depth == 0 {
            return;
        }
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
                    diagnostics.push(Diagnostic {
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

        self.ownership.retain(|_, info| {
            info.scope_depth <= self.base_scope_depth || info.scope_depth < self.scope_depth
        });
        self.scope_depth -= 1;
    }
}

pub fn build_cfg(node: Node, source: &str) -> Cfg {
    let mut cfg = Cfg::new();
    let entry = cfg.add_node(BasicBlock { statements: vec![] });
    collect_statements(node, source, &mut cfg, entry);
    cfg
}

// pub fn build_linear_block(node: Node, source: &str) -> BasicBlock {
//     let mut stmts: Vec<Statement> = Vec::new();
//     collect_statements(node, source, &mut stmts);
//     BasicBlock { statements: stmts }
// }
fn collect_statements(node: Node, source: &str, cfg: &mut Cfg, current: NodeIndex) -> NodeIndex {
    match node.kind() {
        "call_expression" => {
            if let Some(stmt) = try_extract_call(node, source) {
                cfg[current].statements.push(stmt);
            }
            current
        }
        "pointer_expression" => {
            if let Some(stmt) = try_extract_deref(node, source) {
                cfg[current].statements.push(stmt);
            }
            current
        }
        "return_statement" => {
            if let Some(stmt) = try_extract_return(node, source) {
                cfg[current].statements.push(stmt);
            }
            current
        }
        "field_expression" => {
            if let Some(stmt) = try_extract_field_access(node, source) {
                cfg[current].statements.push(stmt);
            }
            current
        }
        "declaration" => {
            if let Some(stmt) = try_extract_declaration(node, source) {
                cfg[current].statements.push(stmt);
            }
            let mut cursor = node.walk();
            let mut current = current;
            for child in node.children(&mut cursor) {
                current = collect_statements(child, source, cfg, current);
            }
            current
        }
        "compound_statement" => {
            cfg[current].statements.push(Statement::EnterScope);
            let mut cursor = node.walk();
            let mut current = current;
            for child in node.children(&mut cursor) {
                current = collect_statements(child, source, cfg, current);
            }
            let row = node.end_position().row + 1;
            let col = node.end_position().column;
            cfg[current]
                .statements
                .push(Statement::ExitScope { row, col });
            current
        }
        "assignment_expression" => {
            if let Some(stmt) = try_extract_assignment(node, source) {
                cfg[current].statements.push(stmt);
            }
            let mut cursor = node.walk();
            let mut current = current;
            for child in node.children(&mut cursor) {
                current = collect_statements(child, source, cfg, current);
            }
            current
        }
        "if_statement" => {
            let true_block = cfg.add_node(BasicBlock { statements: vec![] });
            let false_block = cfg.add_node(BasicBlock { statements: vec![] });
            let merge_block = cfg.add_node(BasicBlock { statements: vec![] });

            cfg.add_edge(current, true_block, ());
            cfg.add_edge(current, false_block, ());

            let then_body = node.child_by_field_name("consequence");
            let true_end = if let Some(body) = then_body {
                collect_statements(body, source, cfg, true_block)
            } else {
                true_block
            };

            let else_body = node.child_by_field_name("alternative");
            let false_end = if let Some(body) = else_body {
                collect_statements(body, source, cfg, false_block)
            } else {
                false_block
            };

            cfg.add_edge(true_end, merge_block, ());
            cfg.add_edge(false_end, merge_block, ());

            merge_block
        }
        "while_statement" => {
            let header_block = cfg.add_node(BasicBlock { statements: vec![] });
            let body_block = cfg.add_node(BasicBlock { statements: vec![] });
            let exit_block = cfg.add_node(BasicBlock { statements: vec![] });

            cfg.add_edge(current, header_block, ());
            cfg.add_edge(header_block, body_block, ());
            cfg.add_edge(header_block, exit_block, ());

            let body_end = if let Some(body) = node.child_by_field_name("body") {
                collect_statements(body, source, cfg, body_block)
            } else {
                body_block
            };

            cfg.add_edge(body_end, header_block, ());

            exit_block
        }
        "for_statement" => {
            let header_block = cfg.add_node(BasicBlock { statements: vec![] });
            let body_block = cfg.add_node(BasicBlock { statements: vec![] });
            let exit_block = cfg.add_node(BasicBlock { statements: vec![] });
            cfg.add_edge(current, header_block, ());
            cfg.add_edge(header_block, body_block, ());
            cfg.add_edge(header_block, exit_block, ());

            // collect initializer into current block before the loop
            if let Some(init) = node.child_by_field_name("initializer") {
                collect_statements(init, source, cfg, current);
            }

            let body_end = if let Some(body) = node.child_by_field_name("body") {
                collect_statements(body, source, cfg, body_block)
            } else {
                body_block
            };

            cfg.add_edge(body_end, header_block, ());
            exit_block
        }
        "switch_statement" => {
            let merge_block = cfg.add_node(BasicBlock { statements: vec![] });

            if !has_default(node) {
                cfg.add_edge(current, merge_block, ());
            }

            let mut prev_case_end: Option<NodeIndex> = None;

            if let Some(body) = node.child_by_field_name("body") {
                let mut cursor = body.walk();
                for child in body.children(&mut cursor) {
                    match child.kind() {
                        "case_statement" | "default_statement" => {
                            let case_block = cfg.add_node(BasicBlock { statements: vec![] });

                            cfg.add_edge(current, case_block, ());

                            if let Some(prev_end) = prev_case_end {
                                cfg.add_edge(prev_end, case_block, ());
                            }

                            let case_end = collect_statements(child, source, cfg, case_block);

                            if ends_with_break(child) {
                                cfg.add_edge(case_end, merge_block, ());
                                prev_case_end = None;
                            } else {
                                prev_case_end = Some(case_end);
                            }
                        }
                        _ => {}
                    }
                }
            }
            if let Some(prev_end) = prev_case_end {
                cfg.add_edge(prev_end, merge_block, ());
            }
            merge_block
        }
        _ => {
            let mut cursor = node.walk();
            let mut current = current;
            for child in node.children(&mut cursor) {
                current = collect_statements(child, source, cfg, current);
            }
            current
        }
    }
}

fn ends_with_break(node: Node) -> bool {
    // look at named children of the case_statement
    // check if the last one is a break_statement
    let count = node.named_child_count();
    if count == 0 {
        return false;
    }
    if let Some(last) = node.named_child((count - 1) as u32) {
        last.kind() == "break_statement"
    } else {
        false
    }
}

fn has_default(node: Node) -> bool {
    if let Some(body) = node.child_by_field_name("body") {
        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            if child.kind() == "default_statement" {
                return true;
            }
        }
    }
    false
}
fn try_extract_assignment(node: Node, source: &str) -> Option<Statement> {
    let left = node.child_by_field_name("left")?;
    let right = node.child_by_field_name("right")?;

    if right.kind() != "pointer_expression" {
        return None;
    }

    let operator = right.child(0)?;
    let op = &source[operator.start_byte()..operator.end_byte()];
    if op != "&" {
        return None;
    }

    let (var, row, col) = get_source(left, source);
    let target = right.named_child(0)?;
    let (points_to, _, _) = get_source(target, source);
    Some(Statement::PointerAssign {
        var,
        points_to,
        row,
        col,
    })
}

fn try_extract_declaration(node: Node, source: &str) -> Option<Statement> {
    let decl = node.child_by_field_name("declarator")?;

    match decl.kind() {
        "init_declarator" => {
            let value = decl.child_by_field_name("value")?;
            let inner = decl.child_by_field_name("declarator")?;
            let value_text = &source[value.start_byte()..value.end_byte()];

            // skip — try_extract_call handles malloc/calloc/realloc
            if value.kind() == "call_expression" {
                return None;
            }

            match inner.kind() {
                "pointer_declarator" => {
                    let var_node = inner.named_child(0).unwrap_or(inner);
                    let (var, row, col) = get_source(var_node, source);
                    if value_text == "NULL" {
                        Some(Statement::NullAssign { var, row, col })
                    } else if value.kind() == "pointer_expression" {
                        let target = value.named_child(0)?;
                        let (points_to, _, _) = get_source(target, source);
                        Some(Statement::AddrAssign {
                            var,
                            points_to,
                            row,
                            col,
                        })
                    } else {
                        Some(Statement::AddrAssign {
                            var,
                            points_to: String::new(),
                            row,
                            col,
                        })
                    }
                }
                "identifier" => {
                    let (var, row, col) = get_source(inner, source);
                    Some(Statement::AddrAssign {
                        var,
                        points_to: String::new(),
                        row,
                        col,
                    })
                }
                _ => None,
            }
        }
        "pointer_declarator" => {
            let var_node = decl.named_child(0).unwrap_or(decl);
            let (var, row, col) = get_source(var_node, source);
            Some(Statement::AddrAssign {
                var,
                points_to: String::new(),
                row,
                col,
            })
        }
        "identifier" => {
            let (var, row, col) = get_source(decl, source);
            Some(Statement::AddrAssign {
                var,
                points_to: String::new(),
                row,
                col,
            })
        }
        _ => None,
    }
}

fn try_extract_field_access(node: Node, source: &str) -> Option<Statement> {
    let argument = node.child_by_field_name("argument")?;
    let op = node
        .child_by_field_name("operator")
        .map(|op| &source[op.start_byte()..op.end_byte()]);

    if op == Some("->") {
        let (var, row, col) = get_source(argument, source);
        return Some(Statement::FieldAccess { var: var, row, col });
    } else {
        None
    }
}

fn try_extract_return(node: Node, source: &str) -> Option<Statement> {
    let expr = node.named_child(0)?;
    match expr.kind() {
        "identifier" => {
            let (var, row, col) = get_source(expr, source);
            Some(Statement::Return {
                var: var,
                row,
                col,
                is_address_of: false,
            })
        }
        "pointer_expression" => {
            let operator = expr.child(0)?;
            let op = &source[operator.start_byte()..operator.end_byte()];
            if op != "&" {
                return None;
            }
            let operand = expr.named_child(0)?;
            let (var, row, col) = get_source(operand, source);
            Some(Statement::Return {
                var,
                row,
                col,
                is_address_of: true,
            })
        }
        _ => None,
    }
}

fn try_extract_deref(node: Node, source: &str) -> Option<Statement> {
    let operator = node.child(0)?;
    let op = &source[operator.start_byte()..operator.end_byte()];
    if op != "*" {
        return None;
    }

    let operand = node.named_child(0)?;
    let (var_name, line, col) = get_source(operand, source);
    Some(Statement::Deref {
        var: var_name,
        row: line,
        col,
    })
}

fn try_extract_call(node: Node, source: &str) -> Option<Statement> {
    let func = node.child_by_field_name("function")?;
    let func_name = &source[func.start_byte()..func.end_byte()];

    match func_name {
        "malloc" | "calloc" | "realloc" => {
            let (var, line, col) = malloc_var(node, source)?;
            Some(Statement::Malloc {
                var,
                row: line,
                col,
            })
        }
        "free" => {
            let args = node.child_by_field_name("arguments")?;
            let arg = args.named_child(0)?;
            let (var, line, col) = get_source(arg, source);
            Some(Statement::Free {
                var: var,
                row: line,
                col,
            })
        }
        _ => {
            let args = node.child_by_field_name("arguments")?;
            let arg = args.named_child(0)?;
            let (var, row, col) = get_source(arg, source);
            Some(Statement::PassToFunction {
                var,
                func: func_name.to_string(),
                row,
                col,
            })
        }
    }
}

fn malloc_var(node: Node, source: &str) -> Option<(String, usize, usize)> {
    let parent = match node.parent() {
        Some(p) => p,
        None => return None,
    };

    match parent.kind() {
        "init_declarator" => {
            let declarator = parent.child_by_field_name("declarator")?;
            let inner = declarator.named_child(0).unwrap_or(declarator);
            let (name, row, col) = get_source(inner, source);
            Some((name, row, col))
        }
        "assignment_expression" => {
            let left = match parent.child_by_field_name("left") {
                Some(l) => l,
                None => return None,
            };
            let (var, line, col) = get_source(left, source);
            Some((var, line, col))
        }
        _ => None,
    }
}

fn get_source<'a>(node: Node, source: &'a str) -> (String, usize, usize) {
    (
        source[node.start_byte()..node.end_byte()].to_string(),
        node.start_position().row + 1,
        node.start_position().column,
    )
}
