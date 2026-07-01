use crate::{
    analyzer_state::OwnershipState,
    cfg::{BlockState, Cfg, Statement, build_cfg},
    diagnostics::{Diagnostic, Severity},
    variable_info::{AllocKind, VariableInfo},
};
use petgraph::graph::NodeIndex;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tree_sitter::Tree;

pub enum CheckMode {
    C,
    Sea,
}

struct SeaMethodInfo {
    name: String,
    param_count: usize,
}

struct SeaClassInfo {
    has_drop: bool,
    methods: Vec<SeaMethodInfo>,
    field_names: Vec<String>,
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

// ─── Scope for semantic checks ───────────────────────────────────────────────

struct Scope {
    /// variable name → type name
    locals: HashMap<String, String>,
    /// all known class names (from file + imports)
    classes: HashSet<String>,
}

impl Scope {
    fn new(file_info: &SeaFileInfo, params: &[(String, String)]) -> Self {
        let mut locals = HashMap::new();
        let mut classes = HashSet::new();

        for name in file_info.class_info.keys() {
            classes.insert(name.clone());
        }

        for (type_name, var_name) in params {
            locals.insert(var_name.clone(), type_name.clone());
        }

        Scope { locals, classes }
    }

    fn declare(&mut self, var_name: String, type_name: String) {
        self.locals.insert(var_name, type_name);
    }

    fn get_type(&self, var_name: &str) -> Option<&String> {
        self.locals.get(var_name)
    }

    fn is_known_class(&self, name: &str) -> bool {
        self.classes.contains(name)
    }
}

// ─── Primitive type set ───────────────────────────────────────────────────────

const PRIMITIVES: &[&str] = &["int", "char", "float", "double", "void", "String", "bool"];

fn is_primitive(t: &str) -> bool {
    PRIMITIVES.contains(&t)
}

// ─── impl Sea ────────────────────────────────────────────────────────────────

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
            let file_info = self.collect_file_info(file);
            let mut cursor = root.walk();

            for child in root.children(&mut cursor) {
                match child.kind() {
                    "class_declaration" => {
                        let name_node = child.child_by_field_name("name").unwrap();
                        let class_name = &self.source[name_node.start_byte()..name_node.end_byte()];

                        let has_drop = file_info
                            .class_info
                            .get(class_name)
                            .map(|i| i.has_drop)
                            .unwrap_or(false);

                        let mut cursor2 = child.walk();
                        for member in child.children(&mut cursor2) {
                            match member.kind() {
                                "init_declaration" => {
                                    // CFG ownership analysis
                                    if let Some(body) = member.child_by_field_name("body") {
                                        let cfg = build_cfg(body, &self.source);
                                        diagnostics.extend(self.analyze_cfg(&cfg, file, has_drop));
                                    }
                                    // semantic checks
                                    let params = self.collect_method_params(&member);
                                    if let Some(body) = member.child_by_field_name("body") {
                                        self.check_method_body(
                                            &body,
                                            class_name,
                                            file,
                                            &mut diagnostics,
                                            &file_info,
                                            params,
                                        );
                                    }
                                }
                                "method_declaration" => {
                                    let method_node = member.child(0).unwrap();
                                    // CFG ownership analysis
                                    if let Some(body) = method_node.child_by_field_name("body") {
                                        let cfg = build_cfg(body, &self.source);
                                        diagnostics.extend(self.analyze_cfg(&cfg, file, has_drop));
                                    }
                                    // semantic checks
                                    let params = self.collect_method_params(&method_node);
                                    if let Some(body) = method_node.child_by_field_name("body") {
                                        self.check_method_body(
                                            &body,
                                            class_name,
                                            file,
                                            &mut diagnostics,
                                            &file_info,
                                            params,
                                        );
                                    }
                                }
                                "drop_declaration" => {
                                    if let Some(body) = member.child_by_field_name("body") {
                                        let cfg = build_cfg(body, &self.source);
                                        diagnostics.extend(self.analyze_cfg(&cfg, file, has_drop));
                                    }
                                    if let Some(body) = member.child_by_field_name("body") {
                                        self.check_method_body(
                                            &body,
                                            class_name,
                                            file,
                                            &mut diagnostics,
                                            &file_info,
                                            vec![],
                                        );
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

                        // semantic checks for main body
                        if let Some(body) = child.child_by_field_name("body") {
                            self.check_method_body(
                                &body,
                                "",
                                file,
                                &mut diagnostics,
                                &file_info,
                                vec![],
                            );
                        }
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

    // ─── Collect file-level info (classes, interfaces, imports) ──────────────

    fn collect_file_info(&self, file_path: &str) -> SeaFileInfo {
        let mut class_info: HashMap<String, SeaClassInfo> = HashMap::new();
        let mut interface_methods: HashMap<String, Vec<String>> = HashMap::new();
        let root = self.tree.root_node();
        let mut cursor = root.walk();

        for child in root.children(&mut cursor) {
            match child.kind() {
                "import_declaration" => {
                    let path_node = child.child_by_field_name("path").unwrap();
                    let path_text = &self.source[path_node.start_byte()..path_node.end_byte()];
                    let path_str = path_text.trim_matches('"');

                    let current_dir = std::path::Path::new(file_path)
                        .parent()
                        .unwrap_or(std::path::Path::new("."));
                    let imported_path = current_dir.join(format!("{}.sea", path_str));

                    if imported_path.exists() {
                        let imported_source = std::fs::read_to_string(&imported_path)
                            .unwrap_or_default()
                            .replace("\r\n", "\n");

                        let mut parser = tree_sitter::Parser::new();
                        parser
                            .set_language(&tree_sitter_sea::LANGUAGE.into())
                            .expect("Error loading Sea grammar");

                        if let Some(imported_tree) = parser.parse(&imported_source, None) {
                            let imported_root = imported_tree.root_node();
                            let mut cursor2 = imported_root.walk();
                            for imported_child in imported_root.children(&mut cursor2) {
                                if imported_child.kind() == "class_declaration" {
                                    let info = self
                                        .extract_class_info_from(&imported_child, &imported_source);
                                    class_info.insert(info.0, info.1);
                                }
                            }
                        }
                    }
                }
                "class_declaration" => {
                    let info = self.extract_class_info_from(&child, &self.source);
                    class_info.insert(info.0, info.1);
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

    /// Extract `SeaClassInfo` from a `class_declaration` node, using the
    /// provided source string (may differ from `self.source` for imports).
    fn extract_class_info_from(
        &self,
        node: &tree_sitter::Node,
        source: &str,
    ) -> (String, SeaClassInfo) {
        let name_node = node.child_by_field_name("name").unwrap();
        let class_name = source[name_node.start_byte()..name_node.end_byte()].to_string();

        let mut has_drop = false;
        let mut methods: Vec<SeaMethodInfo> = Vec::new();
        let mut field_names: Vec<String> = Vec::new();

        let mut cursor = node.walk();
        for member in node.children(&mut cursor) {
            match member.kind() {
                "drop_declaration" => {
                    has_drop = true;
                }
                "field_declaration" => {
                    if let Some(name_node) = member.child_by_field_name("name") {
                        let field_name =
                            source[name_node.start_byte()..name_node.end_byte()].to_string();
                        field_names.push(field_name);
                    }
                }
                "method_declaration" => {
                    let method_node = member.child(0).unwrap();
                    if let Some(method_name_node) = method_node.child_by_field_name("name") {
                        let method_name = source
                            [method_name_node.start_byte()..method_name_node.end_byte()]
                            .to_string();
                        let param_count = self.count_params_in_node(&method_node, source);
                        methods.push(SeaMethodInfo {
                            name: method_name,
                            param_count,
                        });
                    }
                }
                "init_declaration" => {
                    let param_count = self.count_params_in_node(&member, source);
                    methods.push(SeaMethodInfo {
                        name: "init".to_string(),
                        param_count,
                    });
                }
                _ => {}
            }
        }

        (
            class_name,
            SeaClassInfo {
                has_drop,
                methods,
                field_names,
            },
        )
    }

    fn count_params_in_node(&self, node: &tree_sitter::Node, source: &str) -> usize {
        match node.child_by_field_name("parameters") {
            Some(params_node) => {
                let mut cursor = params_node.walk();
                params_node
                    .children(&mut cursor)
                    .filter(|c| c.kind() == "sea_parameter")
                    .count()
            }
            None => 0,
        }
    }

    fn collect_method_params(&self, node: &tree_sitter::Node) -> Vec<(String, String)> {
        let mut params = Vec::new();
        if let Some(params_node) = node.child_by_field_name("parameters") {
            let mut cursor = params_node.walk();
            for param in params_node.children(&mut cursor) {
                if param.kind() == "sea_parameter" {
                    if let (Some(type_node), Some(name_node)) = (
                        param.child_by_field_name("type"),
                        param.child_by_field_name("name"),
                    ) {
                        let type_text =
                            self.source[type_node.start_byte()..type_node.end_byte()].to_string();
                        let name_text =
                            self.source[name_node.start_byte()..name_node.end_byte()].to_string();
                        params.push((type_text, name_text));
                    }
                }
            }
        }
        params
    }

    // ─── Semantic body checker ────────────────────────────────────────────────

    fn check_method_body(
        &self,
        body: &tree_sitter::Node,
        class_name: &str,
        file: &str,
        diagnostics: &mut Vec<Diagnostic>,
        file_info: &SeaFileInfo,
        params: Vec<(String, String)>,
    ) {
        let mut scope = Scope::new(file_info, &params);

        // add class fields into scope so `this.field` lookups resolve
        if let Some(class_info) = file_info.class_info.get(class_name) {
            for field_name in &class_info.field_names {
                scope.declare(field_name.clone(), "field".to_string());
            }
        }

        self.check_body_node(body, class_name, file, diagnostics, file_info, &mut scope);
    }

    fn check_body_node(
        &self,
        node: &tree_sitter::Node,
        class_name: &str,
        file: &str,
        diagnostics: &mut Vec<Diagnostic>,
        file_info: &SeaFileInfo,
        scope: &mut Scope,
    ) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "{" | "}" => {}

                "declaration" => {
                    // resolve the declared type
                    let type_text = child
                        .child_by_field_name("type")
                        .or_else(|| {
                            let mut c = child.walk();
                            child
                                .children(&mut c)
                                .find(|n| n.kind() == "type_identifier")
                        })
                        .map(|n| self.source[n.start_byte()..n.end_byte()].to_string());

                    if let Some(ref t) = type_text {
                        // check unknown type
                        if !is_primitive(t) && !scope.is_known_class(t) {
                            let row = child.start_position().row + 1;
                            let col = child.start_position().column;
                            diagnostics.push(Diagnostic {
                                file: file.to_string(),
                                line: row,
                                col,
                                message: format!("unknown type '{}'", t),
                                severity: Severity::Error,
                            });
                        }

                        // register the variable name in scope
                        let mut c = child.walk();
                        if let Some(init_node) = child
                            .children(&mut c)
                            .find(|n| n.kind() == "init_declarator")
                        {
                            if let Some(decl_node) = init_node.child_by_field_name("declarator") {
                                let var_name = self.source
                                    [decl_node.start_byte()..decl_node.end_byte()]
                                    .to_string();
                                scope.declare(var_name, t.clone());
                            }
                        } else {
                            // plain declaration without initializer: `Animal a;`
                            let mut c2 = child.walk();
                            if let Some(decl_node) =
                                child.children(&mut c2).find(|n| n.kind() == "identifier")
                            {
                                let var_name = self.source
                                    [decl_node.start_byte()..decl_node.end_byte()]
                                    .to_string();
                                scope.declare(var_name, t.clone());
                            }
                        }
                    }

                    // recurse into rhs to check expressions inside
                    self.check_body_node(&child, class_name, file, diagnostics, file_info, scope);
                }

                "expression_statement" => {
                    self.check_body_node(&child, class_name, file, diagnostics, file_info, scope);
                }

                "call_expression" => {
                    if let Some(func) = child.child_by_field_name("function") {
                        if func.kind() == "field_expression" {
                            let obj = func.child_by_field_name("argument").unwrap();
                            let method = func.child_by_field_name("field").unwrap();
                            let obj_text =
                                self.source[obj.start_byte()..obj.end_byte()].to_string();
                            let method_text =
                                self.source[method.start_byte()..method.end_byte()].to_string();

                            let resolved_type = if obj_text == "this" {
                                if class_name.is_empty() {
                                    None
                                } else {
                                    Some(class_name.to_string())
                                }
                            } else {
                                scope.get_type(&obj_text).cloned()
                            };

                            match resolved_type {
                                Some(ref type_name) => {
                                    if file_info.class_info.contains_key(type_name.as_str()) {
                                        // check method exists and arg count
                                        let arg_count = self.count_call_args(&child);
                                        self.check_method_call(
                                            type_name,
                                            &method_text,
                                            arg_count,
                                            child.start_position().row + 1,
                                            child.start_position().column,
                                            file,
                                            diagnostics,
                                            file_info,
                                        );
                                    }
                                    // if type not a known Sea class, silently allow (C type)
                                }
                                None => {
                                    if obj_text != "this" {
                                        let row = child.start_position().row + 1;
                                        let col = child.start_position().column;
                                        diagnostics.push(Diagnostic {
                                            file: file.to_string(),
                                            line: row,
                                            col,
                                            message: format!("undefined variable '{}'", obj_text),
                                            severity: Severity::Error,
                                        });
                                    }
                                }
                            }
                        }
                        // plain call (printf, malloc, etc.) — silently allow
                    }

                    // recurse into args
                    self.check_body_node(&child, class_name, file, diagnostics, file_info, scope);
                }

                "assignment_expression" => {
                    // check rhs — lhs is always a declared var or field so skip
                    if let Some(rhs) = child.child_by_field_name("right") {
                        self.check_body_node(&rhs, class_name, file, diagnostics, file_info, scope);
                    }
                }

                "return_statement" | "if_statement" | "while_statement" | "for_statement"
                | "switch_statement" => {
                    self.check_body_node(&child, class_name, file, diagnostics, file_info, scope);
                }

                // new_expression — check constructor arg count
                "new_expression" => {
                    let type_text = child
                        .child_by_field_name("type")
                        .map(|n| self.source[n.start_byte()..n.end_byte()].to_string());

                    if let Some(ref t) = type_text {
                        if !scope.is_known_class(t) && !is_primitive(t) {
                            let row = child.start_position().row + 1;
                            let col = child.start_position().column;
                            diagnostics.push(Diagnostic {
                                file: file.to_string(),
                                line: row,
                                col,
                                message: format!("unknown class '{}'", t),
                                severity: Severity::Error,
                            });
                        } else if let Some(ref t) = type_text {
                            let arg_count = self.count_call_args(&child);
                            self.check_method_call(
                                t,
                                "init",
                                arg_count,
                                child.start_position().row + 1,
                                child.start_position().column,
                                file,
                                diagnostics,
                                file_info,
                            );
                        }
                    }
                }

                _ => {
                    self.check_body_node(&child, class_name, file, diagnostics, file_info, scope);
                }
            }
        }
    }

    fn check_method_call(
        &self,
        class_name: &str,
        method_name: &str,
        arg_count: usize,
        row: usize,
        col: usize,
        file: &str,
        diagnostics: &mut Vec<Diagnostic>,
        file_info: &SeaFileInfo,
    ) {
        if let Some(class_info) = file_info.class_info.get(class_name) {
            if let Some(method) = class_info.methods.iter().find(|m| m.name == method_name) {
                // method exists — check arg count
                if arg_count != method.param_count {
                    diagnostics.push(Diagnostic {
                        file: file.to_string(),
                        line: row,
                        col,
                        message: format!(
                            "method '{}' on '{}' expects {} argument(s) but got {}",
                            method_name, class_name, method.param_count, arg_count
                        ),
                        severity: Severity::Error,
                    });
                }
            } else {
                // method doesn't exist on the class
                diagnostics.push(Diagnostic {
                    file: file.to_string(),
                    line: row,
                    col,
                    message: format!(
                        "no method '{}' found on class '{}'",
                        method_name, class_name
                    ),
                    severity: Severity::Error,
                });
            }
        }
    }

    fn count_call_args(&self, node: &tree_sitter::Node) -> usize {
        if let Some(args) = node.child_by_field_name("arguments") {
            let mut cursor = args.walk();
            args.children(&mut cursor)
                .filter(|c| c.kind() != "(" && c.kind() != ")" && c.kind() != ",")
                .count()
        } else {
            0
        }
    }

    // ─── Existing class-level checks (unchanged) ──────────────────────────────

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

        let implements = match node.child_by_field_name("implements") {
            Some(n) => n,
            None => return,
        };

        let mut cursor = implements.walk();
        let interface_names: Vec<String> = implements
            .children(&mut cursor)
            .filter(|c| c.kind() == "identifier")
            .map(|c| self.source[c.start_byte()..c.end_byte()].to_string())
            .collect();

        let mut class_methods: Vec<String> = Vec::new();
        let mut cursor2 = node.walk();
        for member in node.children(&mut cursor2) {
            if member.kind() == "method_declaration" {
                let method_node = member.child(0).unwrap();
                if let Some(method_name_node) = method_node.child_by_field_name("name") {
                    let method_name = self.source
                        [method_name_node.start_byte()..method_name_node.end_byte()]
                        .to_string();
                    class_methods.push(method_name);
                }
            }
        }

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
                "init_declaration" => {
                    has_constructor = true;
                    constructor_count += 1;
                    if let Some(body) = child.child_by_field_name("body") {
                        if self.body_contains_malloc(&body) {
                            has_malloc = true;
                            malloc_row = child.start_position().row + 1;
                            malloc_col = child.start_position().column;
                        }
                    }
                }
                "drop_declaration" => {
                    has_drop = true;
                    if let Some(body) = child.child_by_field_name("body") {
                        if !self.body_contains_free(&body) {
                            let row = child.start_position().row + 1;
                            let col = child.start_position().column;
                            diagnostics.push(Diagnostic {
                                file: file.to_string(),
                                line: row,
                                col,
                                message: format!(
                                    "class '{}' has drop() but never calls free() — possible memory leak",
                                    class_name
                                ),
                                severity: Severity::Warning,
                            });
                        }
                    }
                }
                "method_declaration" => {
                    let method_node = child.child(0).unwrap();
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
                                "C style method '{}' detected — use Sea style: {}() -> {}",
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
                    "class '{}' has no constructor — add an 'init()' method",
                    class_name
                ),
                severity: Severity::Error,
            });
        }

        if constructor_count > 1 {
            let row = node.start_position().row + 1;
            let col = node.start_position().column;
            diagnostics.push(Diagnostic {
                file: file.to_string(),
                line: row,
                col,
                message: format!(
                    "class '{}' has multiple constructors — Sea does not support constructor overloading",
                    class_name
                ),
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
            if child.kind() == "call_expression" {
                if let Some(func) = child.child_by_field_name("function") {
                    let func_text = &self.source[func.start_byte()..func.end_byte()];
                    if matches!(func_text, "malloc" | "calloc" | "realloc") {
                        return true;
                    }
                }
            }
            if self.body_contains_malloc(&child) {
                return true;
            }
        }
        false
    }

    // ─── CFG ownership analysis (unchanged) ───────────────────────────────────

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
}

// ─── Free functions for CFG handlers (unchanged) ─────────────────────────────

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
        Some(OwnershipState::MaybeFreed) => {
            diagnostics.push(Diagnostic {
                file: file.to_string(),
                line: row,
                col,
                message: format!("possible double free of '{}'", var),
                severity: Severity::Warning,
            });
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
                diagnostics.push(Diagnostic {
                    file: file.to_string(),
                    line: row,
                    col,
                    message: format!(
                        "returning address of stack variable '{}' which will be invalid after function returns",
                        var
                    ),
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
