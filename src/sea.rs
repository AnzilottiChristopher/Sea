use crate::{
    analyzer_state::OwnershipState,
    cfg::{BlockState, Cfg, Statement, build_cfg},
    diagnostics::{Diagnostic, Severity},
    variable_info::{AllocKind, VariableInfo},
};
use petgraph::{algo::toposort, graph::NodeIndex};
use std::{collections::HashMap, path::PathBuf};
use tree_sitter::Tree;

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
            .expect("Error loading C grammar");

        let tree = parser.parse(&source, None).unwrap();

        Sea { source, tree }
    }

    pub fn analyze(&self, file: &str) -> Vec<Diagnostic> {
        let root = self.tree.root_node();
        let cfg = build_cfg(root, &self.source);
        // self.analyze_cfg_linear(&cfg, file)
        self.analyze_cfg(&cfg, file)
    }

    pub fn analyze_cfg(&self, cfg: &Cfg, file: &str) -> Vec<Diagnostic> {
        let mut diagnostics: Vec<Diagnostic> = Vec::new();

        let mut block_states: HashMap<NodeIndex, BlockState> = HashMap::new();

        let order = match toposort(cfg, None) {
            Ok(order) => order,
            Err(_) => cfg.node_indices().collect(),
        };

        for index in order {
            let mut predessors = cfg.neighbors_directed(index, petgraph::Direction::Incoming);

            let incoming_state = match predessors.next() {
                None => BlockState::new(),
                Some(first_pred) => {
                    let mut state = block_states
                        .get(&first_pred)
                        .cloned()
                        .unwrap_or_else(BlockState::new);

                    for pred in predessors {
                        if let Some(pred_state) = block_states.get(&pred) {
                            state = state.merge(pred_state);
                        }
                    }
                    state
                }
            };

            let mut state = incoming_state;
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
            block_states.insert(index, state);
        }
        diagnostics
    }

    // pub fn analyze_cfg_linear(&self, cfg: &Cfg, file: &str) -> Vec<Diagnostic> {
    //     let mut diagnostics: Vec<Diagnostic> = Vec::new();
    //     let mut state = BlockState::new();
    //
    //     for index in cfg.node_indices() {
    //         let block = &cfg[index];
    //         for stmt in &block.statements {
    //             match stmt {
    //                 Statement::Malloc { var, .. } => {
    //                     state
    //                         .ownership
    //                         .insert(var.clone(), VariableInfo::heap(state.scope_depth));
    //                 }
    //                 Statement::Free { var, row, col } => {
    //                     handle_free(var, *row, *col, file, &mut state, &mut diagnostics);
    //                 }
    //                 Statement::Deref { var, row, col } => {
    //                     handle_deref(var, *row, *col, file, &mut state, &mut diagnostics);
    //                 }
    //                 Statement::FieldAccess { var, row, col } => {
    //                     handle_field_access(var, *row, *col, file, &mut state, &mut diagnostics);
    //                 }
    //                 Statement::PassToFunction {
    //                     var,
    //                     func,
    //                     row,
    //                     col,
    //                 } => {
    //                     handle_pass_freed_pointer(
    //                         var,
    //                         func,
    //                         *row,
    //                         *col,
    //                         file,
    //                         &mut state,
    //                         &mut diagnostics,
    //                     );
    //                 }
    //                 Statement::Return {
    //                     var,
    //                     row,
    //                     col,
    //                     is_address_of,
    //                 } => {
    //                     handle_return(
    //                         var,
    //                         *row,
    //                         *col,
    //                         *is_address_of,
    //                         file,
    //                         &mut state,
    //                         &mut diagnostics,
    //                     );
    //                 }
    //                 Statement::NullAssign { var, .. } => {
    //                     state
    //                         .ownership
    //                         .insert(var.clone(), VariableInfo::null(state.scope_depth));
    //                 }
    //                 Statement::AddrAssign { var, points_to, .. } => {
    //                     let mut info = VariableInfo::stack(state.scope_depth);
    //                     if !points_to.is_empty() {
    //                         info.state = OwnershipState::Allocated;
    //                         info.points_to = Some(points_to.clone());
    //                     }
    //                     state.ownership.insert(var.clone(), info);
    //                 }
    //                 Statement::EnterScope => state.enter_scope(),
    //                 Statement::ExitScope { row, col } => {
    //                     state.exit_scope(file, *row, *col, &mut diagnostics);
    //                 }
    //                 Statement::PointerAssign { var, points_to, .. } => {
    //                     if let Some(info) = state.ownership.get_mut(var) {
    //                         info.points_to = Some(points_to.clone());
    //                         info.state = OwnershipState::Allocated;
    //                     }
    //                 }
    //             }
    //         }
    //     }
    //     diagnostics
    // }
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
        Some(OwnershipState::MaybeFreed) => {
            diagnostics.push(Diagnostic {
                file: file.to_string(),
                line: row,
                col,
                message: format!("possible double free of '{}'", var),
                severity: Severity::Warning,
            });
        }
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
        Some(OwnershipState::MaybeFreed) => {
            diagnostics.push(Diagnostic {
                file: file.to_string(),
                line: row,
                col,
                message: format!("possible use after free of '{}'", var),
                severity: Severity::Warning,
            });
        }
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
        Some(OwnershipState::MaybeFreed) => {
            diagnostics.push(Diagnostic {
                file: file.to_string(),
                line: row,
                col,
                message: format!("possible field access of a free pointer '{}'", var),
                severity: Severity::Warning,
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
                message: format!(
                    "passing uninitialized pointer '{}' to function '{}'",
                    var, func
                ),
                severity: Severity::Warning,
            });
        }
        Some(OwnershipState::Null) => {
            diagnostics.push(Diagnostic {
                file: file.to_string(),
                line: row,
                col,
                message: format!("passing null pointer '{}' to function '{}'", var, func),
                severity: Severity::Warning,
            });
        }
        Some(OwnershipState::MaybeFreed) => {
            diagnostics.push(Diagnostic {
                file: file.to_string(),
                line: row,
                col,
                message: format!(
                    "potentially passing a freed pointer '{}' to function '{}'",
                    var, func
                ),
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
            Some(OwnershipState::MaybeFreed) => {
                diagnostics.push(Diagnostic {
                    file: file.to_string(),
                    line: row,
                    col,
                    message: format!("possibly returning freed pointer '{}' from function", var),
                    severity: Severity::Warning,
                });
            }
            _ => {}
        }
    }
}
