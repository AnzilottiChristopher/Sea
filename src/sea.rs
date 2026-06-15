use crate::{
    analyzer_state::OwnershipState,
    cfg::{BlockState, Cfg, Statement, build_cfg},
    diagnostics::{Diagnostic, Severity},
    variable_info::{AllocKind, VariableInfo},
};
use petgraph::graph::NodeIndex;
use std::{collections::HashMap, path::PathBuf};
use tree_sitter::Tree;

pub enum CheckMode {
    C,
    Sea,
}

struct SeaClassInfo {
    has_drop: bool,
}
struct SeaFileInfo {
    class_info: HashMap<String, SeaClassInfo>,
    interface_methods: HashMap<String, Vec<String>>,
}

pub struct Sea {
    source: String,
    tree: Tree,
    mode: CheckMode,
}

impl Sea {
    pub fn new(path: &PathBuf) -> Self {
        let source = std::fs::read_to_string(path).unwrap_or_else(|e| {
            eprintln!("Error reading {:?}: {}", path, e);
            std::process::exit(1);
        });

        let source = source.replace("\r\n", "\n");

        let mut parser = tree_sitter::Parser::new();

        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("c");

        let mode = match extension {
            "sea" => {
                parser
                    .set_language(&tree_sitter_sea::LANGUAGE.into())
                    .expect("Error loading Sea grammar");
                CheckMode::Sea
            }
            _ => {
                parser
                    .set_language(&tree_sitter_c::LANGUAGE.into())
                    .expect("Error loading C grammar");
                CheckMode::C
            }
        };

        let tree = parser.parse(&source, None).unwrap();

        Sea { source, tree, mode }
    }

    pub fn analyze(&self, file: &str) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let root = self.tree.root_node();
        if matches!(self.mode, CheckMode::Sea) {
            let file_info = self.collect_file_info(); // renamed
            let mut cursor = root.walk();
            for child in root.children(&mut cursor) {
                match child.kind() {
                    "class_declaration" => {
                        let name_node = child.child_by_field_name("name").unwrap();
                        let class_name = &self.source[name_node.start_byte()..name_node.end_byte()];

                        // updated to use file_info.class_info
                        let has_drop = file_info
                            .class_info
                            .get(class_name)
                            .map(|i| i.has_drop)
                            .unwrap_or(false);

                        let mut cursor2 = child.walk();
                        for member in child.children(&mut cursor2) {
                            match member.kind() {
                                "constructor_declaration"
                                | "method_declaration"
                                | "drop_declaration" => {
                                    if let Some(body) = member.child_by_field_name("body") {
                                        let cfg = build_cfg(body, &self.source);
                                        diagnostics.extend(self.analyze_cfg(&cfg, file, has_drop));
                                    }
                                }
                                _ => {}
                            }
                        }
                        self.check_class(&child, file, &mut diagnostics, &file_info);
                        self.check_class_interfaces(
                            &child,
                            file,
                            &mut diagnostics,
                            &file_info.interface_methods,
                        );
                    }
                    "main_declaration" => {
                        let cfg = build_cfg(child, &self.source);
                        diagnostics.extend(self.analyze_cfg(&cfg, file, false));
                    }
                    _ => {
                        let cfg = build_cfg(child, &self.source);
                        diagnostics.extend(self.analyze_cfg(&cfg, file, false));
                    }
                }
            }
        } else {
            let cfg = build_cfg(root, &self.source);
            diagnostics.extend(self.analyze_cfg(&cfg, file, false));
        }
        diagnostics
    }

    fn collect_file_info(&self) -> SeaFileInfo {
        let mut class_info: HashMap<String, SeaClassInfo> = HashMap::new();
        let mut interface_methods: HashMap<String, Vec<String>> = HashMap::new();
        let root = self.tree.root_node();
        let mut cursor = root.walk();

        for child in root.children(&mut cursor) {
            match child.kind() {
                "class_declaration" => {
                    let name_node = child.child_by_field_name("name").unwrap();
                    let class_name =
                        self.source[name_node.start_byte()..name_node.end_byte()].to_string();
                    let mut has_drop = false;
                    let mut cursor2 = child.walk();
                    for member in child.children(&mut cursor2) {
                        if member.kind() == "drop_declaration" {
                            has_drop = true;
                        }
                    }
                    class_info.insert(class_name, SeaClassInfo { has_drop });
                }
                "interface_declaration" => {
                    let name_node = child.child_by_field_name("name").unwrap();
                    let interface_name =
                        self.source[name_node.start_byte()..name_node.end_byte()].to_string();
                    let mut methods: Vec<String> = Vec::new();
                    let mut cursor2 = child.walk();
                    for member in child.children(&mut cursor2) {
                        if member.kind() == "interface_method" {
                            if let Some(method_name_node) = member.child_by_field_name("name") {
                                let method_name = self.source
                                    [method_name_node.start_byte()..method_name_node.end_byte()]
                                    .to_string();
                                methods.push(method_name);
                            }
                        }
                    }
                    interface_methods.insert(interface_name, methods);
                }
                _ => {}
            }
        }

        SeaFileInfo {
            class_info,
            interface_methods,
        }
    }

    fn check_class_interfaces(
        &self,
        node: &tree_sitter::Node,
        file: &str,
        diagnostics: &mut Vec<Diagnostic>,
        interface_methods: &HashMap<String, Vec<String>>,
    ) {
        let name_node = node.child_by_field_name("name").unwrap();
        let class_name = &self.source[name_node.start_byte()..name_node.end_byte()];
        let row = node.start_position().row + 1;
        let col = node.start_position().column;

        // get implements clause
        let implements = match node.child_by_field_name("implements") {
            Some(n) => n,
            None => return, // no interfaces — nothing to check
        };

        // collect interface names from implements clause
        let mut cursor = implements.walk();
        let interface_names: Vec<String> = implements
            .children(&mut cursor)
            .filter(|c| c.kind() == "identifier")
            .map(|c| self.source[c.start_byte()..c.end_byte()].to_string())
            .collect();

        // collect class method names
        let mut class_methods: Vec<String> = Vec::new();
        let mut cursor2 = node.walk();
        for member in node.children(&mut cursor2) {
            match member.kind() {
                "method_declaration" => {
                    let method_node = member.child(0).unwrap();
                    if let Some(method_name_node) = method_node.child_by_field_name("name") {
                        let method_name = self.source
                            [method_name_node.start_byte()..method_name_node.end_byte()]
                            .to_string();
                        class_methods.push(method_name);
                    }
                }
                "constructor_declaration" => {
                    // GLR matched methods as constructors
                    let con_name_node = member.child_by_field_name("name").unwrap();
                    let con_name = self.source
                        [con_name_node.start_byte()..con_name_node.end_byte()]
                        .to_string();
                    if con_name != class_name {
                        class_methods.push(con_name);
                    }
                }
                _ => {}
            }
        }

        // check each interface
        for interface_name in &interface_names {
            if let Some(required_methods) = interface_methods.get(interface_name) {
                for method in required_methods {
                    if !class_methods.contains(method) {
                        diagnostics.push(Diagnostic {
                            file: file.to_string(),
                            line: row,
                            col,
                            message: format!(
                                "class '{}' implements '{}' but is missing method '{}'",
                                class_name, interface_name, method
                            ),
                            severity: Severity::Error,
                        });
                    }
                }
            } else {
                diagnostics.push(Diagnostic {
                    file: file.to_string(),
                    line: row,
                    col,
                    message: format!(
                        "class '{}' implements unknown interface '{}'",
                        class_name, interface_name
                    ),
                    severity: Severity::Error,
                });
            }
        }
    }

    fn check_class(
        &self,
        node: &tree_sitter::Node,
        file: &str,
        diagnostics: &mut Vec<Diagnostic>,
        file_info: &SeaFileInfo,
    ) {
        let name_node = node.child_by_field_name("name").unwrap();
        let class_name = &self.source[name_node.start_byte()..name_node.end_byte()];

        if let Some(inherit) = node.child_by_field_name("inherit") {
            if let Some(parent_node) = inherit.child_by_field_name("parent") {
                let parent_name = &self.source[parent_node.start_byte()..parent_node.end_byte()];
                if !file_info.class_info.contains_key(parent_name) {
                    let row = node.start_position().row + 1;
                    let col = node.start_position().column;
                    diagnostics.push(Diagnostic {
                        file: file.to_string(),
                        line: row,
                        col,
                        message: format!(
                            "class '{}' inherits unknown class '{}'",
                            class_name, parent_name
                        ),
                        severity: Severity::Error,
                    });
                }
            }
        }

        let mut has_malloc = false;
        let mut has_drop = false;
        let mut has_constructor = false;
        let mut malloc_row = 0;
        let mut malloc_col = 0;
        let mut constructor_count = 0;

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "constructor_declaration" => {
                    let con_name_node = child.child_by_field_name("name").unwrap();
                    let con_name =
                        &self.source[con_name_node.start_byte()..con_name_node.end_byte()];

                    if con_name == class_name {
                        has_constructor = true;
                        constructor_count += 1;

                        // check if constructor body contains malloc
                        if let Some(body) = child.child_by_field_name("body") {
                            if self.body_contains_malloc(&body) {
                                has_malloc = true;
                                malloc_row = child.start_position().row + 1;
                                malloc_col = child.start_position().column;
                            }
                        }
                    }
                }
                "drop_declaration" => {
                    has_drop = true;

                    if let Some(body) = child.child_by_field_name("body") {
                        if !self.body_contains_free(&body) {
                            let row = child.start_position().row + 1;
                            let col = child.start_position().column;
                            let message = format!(
                                "class '{}' has drop() but never calls free() — possible memory leak",
                                class_name
                            );

                            diagnostics.push(Diagnostic {
                                file: file.to_string(),
                                line: row,
                                col,
                                message,
                                severity: Severity::Warning,
                            });
                        }
                    }
                }
                "method_declaration" => {
                    let method_node = child.child(0).unwrap();
                    // check if it's c_style_method
                    if method_node.kind() == "c_style_method" {
                        let row = child.start_position().row + 1;
                        let col = child.start_position().column;
                        let name_node = method_node.child_by_field_name("name").unwrap();
                        let method_name =
                            &self.source[name_node.start_byte()..name_node.end_byte()];
                        let type_node = method_node.child_by_field_name("return_type").unwrap();
                        let type_text = &self.source[type_node.start_byte()..type_node.end_byte()];
                        diagnostics.push(Diagnostic {
                            file: file.to_string(),
                            line: row,
                            col,
                            message: format!(
                                "C style method '{}' detected — it's better to use Sea style: {}() -> {}",
                                method_name, method_name, type_text
                            ),
                            severity: Severity::Warning,
                        });
                    }
                }
                _ => {}
            }
        }

        if !has_constructor {
            let row = node.start_position().row + 1;
            let col = node.start_position().column;
            diagnostics.push(Diagnostic {
                file: file.to_string(),
                line: row,
                col,
                message: format!(
                    "class '{}' has no constructor -- add a '{}()' method",
                    class_name, class_name
                ),
                severity: Severity::Error,
            });
        }
        // TODO possibly remove the next rule if we ever support multiple constructors
        if constructor_count > 1 {
            let row = node.start_position().row + 1;
            let col = node.start_position().column;
            let message = format!(
                "class '{}' has multiple constructors — Sea does not support constructor overloading",
                class_name
            );

            diagnostics.push(Diagnostic {
                file: file.to_string(),
                line: row,
                col,
                message,
                severity: Severity::Error,
            });
        }

        if has_malloc && !has_drop {
            diagnostics.push(Diagnostic {
                file: file.to_string(),
                line: malloc_row,
                col: malloc_col,
                message: format!(
                    "class '{}' uses malloc in constructor but has no drop() method",
                    class_name
                ),
                severity: Severity::Error,
            });
        }
    }
    fn body_contains_free(&self, node: &tree_sitter::Node) -> bool {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "call_expression" {
                if let Some(func) = child.child_by_field_name("function") {
                    let func_text = &self.source[func.start_byte()..func.end_byte()];
                    if func_text == "free" {
                        return true;
                    }
                }
            }
            if self.body_contains_free(&child) {
                return true;
            }
        }
        false
    }

    fn body_contains_malloc(&self, node: &tree_sitter::Node) -> bool {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            // check if this node is a malloc call
            if child.kind() == "call_expression" {
                if let Some(func) = child.child_by_field_name("function") {
                    let func_text = &self.source[func.start_byte()..func.end_byte()];
                    if matches!(func_text, "malloc" | "calloc" | "realloc") {
                        return true;
                    }
                }
            }
            // recurse into children
            if self.body_contains_malloc(&child) {
                return true;
            }
        }
        false
    }

    pub fn analyze_cfg(&self, cfg: &Cfg, file: &str, class_has_drop: bool) -> Vec<Diagnostic> {
        let mut diagnostics: Vec<Diagnostic> = Vec::new();
        let mut block_in_states: HashMap<NodeIndex, BlockState> = HashMap::new();
        let mut block_out_states: HashMap<NodeIndex, BlockState> = HashMap::new();

        let mut worklist: Vec<NodeIndex> = vec![cfg.node_indices().next().unwrap()];

        while let Some(index) = worklist.pop() {
            let mut predecessors = cfg.neighbors_directed(index, petgraph::Direction::Incoming);

            let incoming_state = match predecessors.next() {
                None => BlockState::new(class_has_drop),
                Some(first_pred) => {
                    let mut state = block_out_states
                        .get(&first_pred)
                        .cloned()
                        .unwrap_or_else(|| BlockState::new(class_has_drop));

                    for pred in predecessors {
                        if let Some(pred_state) = block_out_states.get(&pred) {
                            state = state.merge(pred_state);
                        }
                    }
                    state
                }
            };
            let old_state = block_in_states.get(&index).cloned();
            let state_changed = match &old_state {
                None => true,
                Some(old) => old.ownership != incoming_state.ownership,
            };

            if !state_changed {
                continue;
            }

            block_in_states.insert(index, incoming_state.clone());

            let mut state = incoming_state;
            let leading_scopes = cfg[index]
                .statements
                .iter()
                .take_while(|s| matches!(s, Statement::EnterScope))
                .count();
            state.base_scope_depth = state.scope_depth + leading_scopes;
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
                        let incoming = block_in_states
                            .get(&index)
                            .map(|s| s.ownership.clone())
                            .unwrap_or_default();
                        state.exit_scope(file, *row, *col, &mut diagnostics, &incoming);
                    }
                    Statement::PointerAssign { var, points_to, .. } => {
                        if let Some(info) = state.ownership.get_mut(var) {
                            info.points_to = Some(points_to.clone());
                            info.state = OwnershipState::Allocated;
                        }
                    }
                }
            }
            block_out_states.insert(index, state);
            for successor in cfg.neighbors_directed(index, petgraph::Direction::Outgoing) {
                if !worklist.contains(&successor) {
                    worklist.push(successor);
                }
            }
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
        _ => {}
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
        _ => {}
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
            _ => {
                if let Some(info) = state.ownership.get_mut(var) {
                    info.state = OwnershipState::Returned;
                }
            }
        }
    }
}
