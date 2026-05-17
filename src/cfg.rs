use crate::analyzer_state::OwnershipState;
use std::collections::HashMap;
use tree_sitter::Node;

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
    },
    PassToFunction {
        var: String,
        func: String,
        row: usize,
        col: usize,
    },
}

pub struct BasicBlock {
    pub id: usize,
    pub statements: Vec<Statement>,
}

#[derive(Clone)]
pub struct BlockState {
    pub ownership: HashMap<String, OwnershipState>,
}

impl BlockState {
    pub fn new() -> Self {
        BlockState {
            ownership: HashMap::new(),
        }
    }

    pub fn merge(&self, other: &BlockState) -> BlockState {
        let mut merged = self.clone();

        for (var, state) in &other.ownership {
            match merged.ownership.get(var) {
                Some(existing) if existing == state => {}
                Some(_) => {
                    merged
                        .ownership
                        .insert(var.clone(), OwnershipState::MaybeFreed);
                }
                None => {
                    merged.ownership.insert(var.clone(), state.clone());
                }
            }
        }
        merged
    }
}

pub fn build_linear_block(node: Node, source: &str) -> BasicBlock {
    let mut stmts: Vec<Statement> = Vec::new();
    collect_statements(node, source, &mut stmts);
    BasicBlock {
        id: 0,
        statements: stmts,
    }
}
fn collect_statements(node: Node, source: &str, stmts: &mut Vec<Statement>) {
    match node.kind() {
        "call_expression" => {
            if let Some(stmt) = try_extract_call(node, source) {
                stmts.push(stmt);
            }
        }
        "pointer_expression" => {
            if let Some(stmt) = try_extract_deref(node, source) {
                stmts.push(stmt);
            }
        }
        "return_statement" => {
            if let Some(stmt) = try_extract_return(node, source) {
                stmts.push(stmt);
            }
        }
        "field_access" => {
            if let Some(stmt) = try_extract_field_access(node, source) {
                stmts.push(stmt);
            }
        }
        _ => {}
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
            Some(Statement::Return { var: var, row, col })
        }
        "pointer_expression" => {
            let operand = expr.named_child(0)?;
            let (var, row, col) = get_source(operand, source);
            Some(Statement::Return { var: var, row, col })
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
        _ => None,
    }
}

fn malloc_var(node: Node, source: &str) -> Option<(String, usize, usize)> {
    let parent = match node.parent() {
        Some(p) => p,
        None => return None,
    };

    match parent.kind() {
        "init_declarator" => {
            let declarator = match parent.child_by_field_name("declarator") {
                Some(d) => d,
                None => return None,
            };
            let (var, line, col) = get_source(declarator, source);
            Some((var, line, col))
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
