use super::ProjectSymbolKind;
use crate::LanguageId;

/// One AST-extracted declaration: name, kind, and 1-based line/column of the
/// identifier.
pub(super) type AstSymbol = (String, ProjectSymbolKind, usize, usize);

/// Parses `text` with the grammar registered for `language` and walks
/// declaration nodes. Returns `None` when no grammar is registered or the
/// text fails to parse; the caller falls back to line scanning in both cases.
pub(super) fn extract_ast_symbols(
    language: LanguageId,
    text: &str,
    cap: usize,
) -> Option<Vec<AstSymbol>> {
    let extract = ast_extractor(language)?;
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&extract.language).ok()?;
    let tree = parser.parse(text, None)?;
    let mut symbols = Vec::new();
    for node in walk(tree.root_node()) {
        if symbols.len() >= cap {
            return Some(symbols);
        }
        if node.is_error() || node.is_missing() {
            continue;
        }
        if let Some(symbol) = (extract.select)(&node, text) {
            symbols.push(symbol);
        }
    }
    Some(symbols)
}

/// Picks the symbol name node and kind out of a declaration node. Receives
/// the source text for identifier extraction and returns owned data, so
/// implementations never fight node lifetimes.
type SelectFn = fn(&tree_sitter::Node<'_>, &str) -> Option<AstSymbol>;

struct AstExtractor {
    language: tree_sitter::Language,
    select: SelectFn,
}

/// Depth-first iteration in source order. Declaration nodes are yielded and
/// only descended into when they can nest further declarations (modules,
/// impls, class bodies, trait bodies); everything else — function bodies
/// especially — is skipped so locals never surface as symbols.
fn walk(root: tree_sitter::Node<'_>) -> DeclarationNodes<'_> {
    DeclarationNodes { stack: vec![root] }
}

struct DeclarationNodes<'a> {
    stack: Vec<tree_sitter::Node<'a>>,
}

impl<'a> Iterator for DeclarationNodes<'a> {
    type Item = tree_sitter::Node<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(node) = self.stack.pop() {
            let is_declaration = node_is_declaration(node.kind());
            if !is_declaration || node_is_container(node.kind()) {
                // Pushed reversed so the LIFO stack visits children in
                // source order.
                let mut cursor = node.walk();
                let children: Vec<_> = node.children(&mut cursor).collect();
                for child in children.into_iter().rev() {
                    self.stack.push(child);
                }
            }
            if is_declaration {
                return Some(node);
            }
        }
        None
    }
}

fn node_is_declaration(kind: &str) -> bool {
    matches!(
        kind,
        "function_item"
            | "function_signature_item"
            | "function_declaration"
            | "method_definition"
            | "method_declaration"
            | "generator_function_declaration"
            | "function_definition"
            | "class_definition"
            | "class_declaration"
            | "struct_item"
            | "struct_specifier"
            | "enum_item"
            | "enum_declaration"
            | "enum_specifier"
            | "union_item"
            | "union_specifier"
            | "class_specifier"
            | "trait_item"
            | "interface_declaration"
            | "interface_specifier"
            | "type_item"
            | "type_definition"
            | "type_alias_declaration"
            | "type_spec"
            | "const_item"
            | "static_item"
            | "const_spec"
            | "var_spec"
            | "lexical_declaration"
            | "macro_definition"
            | "mod_item"
    )
}

fn node_is_container(kind: &str) -> bool {
    matches!(
        kind,
        "source_file"
            | "mod_item"
            | "impl_item"
            | "class_definition"
            | "class_declaration"
            | "class_specifier"
            | "declaration_list"
            | "enum_item"
            | "enum_specifier"
            | "enumerator_list"
            | "trait_item"
            | "interface_declaration"
            | "interface_specifier"
            | "struct_item"
            | "struct_specifier"
            | "union_specifier"
            | "type_declaration"
            | "const_declaration"
            | "var_declaration"
            | "namespace_definition"
    )
}

fn rust_select(node: &tree_sitter::Node<'_>, source: &str) -> Option<AstSymbol> {
    let kind = match node.kind() {
        "function_item" => ProjectSymbolKind::Function,
        "struct_item" => ProjectSymbolKind::Struct,
        "enum_item" => ProjectSymbolKind::Enum,
        "union_item" => ProjectSymbolKind::Struct,
        "trait_item" => ProjectSymbolKind::Interface,
        "mod_item" => ProjectSymbolKind::Module,
        "const_item" | "static_item" => ProjectSymbolKind::Constant,
        "type_item" => ProjectSymbolKind::Type,
        "macro_definition" | "function_signature_item" => ProjectSymbolKind::Function,
        _ => return None,
    };
    ast_name(node, source, kind)
}

fn python_select(node: &tree_sitter::Node<'_>, source: &str) -> Option<AstSymbol> {
    let kind = match node.kind() {
        "function_definition" => ProjectSymbolKind::Function,
        "class_definition" => ProjectSymbolKind::Class,
        _ => return None,
    };
    ast_name(node, source, kind)
}

fn go_select(node: &tree_sitter::Node<'_>, source: &str) -> Option<AstSymbol> {
    match node.kind() {
        "function_declaration" | "method_declaration" => {
            ast_name(node, source, ProjectSymbolKind::Function)
        }
        "type_spec" => {
            let kind = match node.child_by_field_name("type")?.kind() {
                "struct_type" => ProjectSymbolKind::Struct,
                "interface_type" => ProjectSymbolKind::Interface,
                _ => ProjectSymbolKind::Type,
            };
            ast_name(node, source, kind)
        }
        "const_spec" => ast_name(node, source, ProjectSymbolKind::Constant),
        "var_spec" => ast_name(node, source, ProjectSymbolKind::Variable),
        _ => None,
    }
}

fn typescript_select(node: &tree_sitter::Node<'_>, source: &str) -> Option<AstSymbol> {
    let kind = match node.kind() {
        "function_declaration"
        | "generator_function_declaration"
        | "method_definition"
        | "function_signature" => ProjectSymbolKind::Function,
        "class_declaration" => ProjectSymbolKind::Class,
        "enum_declaration" => ProjectSymbolKind::Enum,
        "interface_declaration" => ProjectSymbolKind::Interface,
        "type_alias_declaration" => ProjectSymbolKind::Type,
        "variable_declaration" | "lexical_declaration" => {
            // const → Constant; let/var → Variable. Declarators are plain
            // named children of the declaration node.
            let is_const = node
                .child_by_field_name("kind")
                .and_then(|kind| kind.utf8_text(source.as_bytes()).ok())
                .map(|kind| kind == "const")
                .unwrap_or(false);
            let mut cursor = node.walk();
            for declarator in node.children(&mut cursor) {
                if declarator.kind() == "variable_declarator" {
                    // A const/let initialized with an arrow function or
                    // function expression is a function to the user, not a
                    // data binding.
                    let value_kind = declarator
                        .child_by_field_name("value")
                        .map(|value| value.kind())
                        .unwrap_or_default();
                    let kind = if value_kind.contains("function") {
                        ProjectSymbolKind::Function
                    } else if is_const {
                        ProjectSymbolKind::Constant
                    } else {
                        ProjectSymbolKind::Variable
                    };
                    return ast_name(&declarator, source, kind);
                }
            }
            return None;
        }
        _ => return None,
    };
    ast_name(node, source, kind)
}

fn c_family_select(node: &tree_sitter::Node<'_>, source: &str) -> Option<AstSymbol> {
    match node.kind() {
        "function_definition" => {
            // The name lives at declarator → function_declarator → declarator.
            let declarator = node.child_by_field_name("declarator")?;
            let name_node = if declarator.kind() == "function_declarator" {
                declarator.child_by_field_name("declarator")?
            } else {
                declarator
            };
            ast_name_at(name_node, source, ProjectSymbolKind::Function)
        }
        "struct_specifier" | "union_specifier" => ast_name(node, source, ProjectSymbolKind::Struct),
        "enum_specifier" => ast_name(node, source, ProjectSymbolKind::Enum),
        "class_specifier" => ast_name(node, source, ProjectSymbolKind::Class),
        "type_definition" => {
            // The typedef name is the last identifier child (the new name).
            let mut cursor = node.walk();
            let mut name_node = None;
            for child in node.children(&mut cursor) {
                if child.kind() == "type_identifier" || child.kind() == "identifier" {
                    name_node = Some(child);
                }
            }
            ast_name_at(name_node?, source, ProjectSymbolKind::Type)
        }
        _ => None,
    }
}

/// Extracts the `name`-field child of a declaration node, falling back to
/// the first identifier-like child for grammars that omit the field (trait
/// method signatures, Go methods, TypeScript enums).
fn ast_name(
    node: &tree_sitter::Node<'_>,
    source: &str,
    kind: ProjectSymbolKind,
) -> Option<AstSymbol> {
    if let Some(name_node) = node.child_by_field_name("name") {
        return ast_name_at(name_node, source, kind);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(
            child.kind(),
            "identifier" | "field_identifier" | "type_identifier"
        ) {
            return ast_name_at(child, source, kind);
        }
    }
    None
}

/// Builds the symbol tuple from an explicit identifier node.
fn ast_name_at(
    name_node: tree_sitter::Node<'_>,
    source: &str,
    kind: ProjectSymbolKind,
) -> Option<AstSymbol> {
    let name = name_node.utf8_text(source.as_bytes()).ok()?;
    if name.is_empty() {
        return None;
    }
    let position = name_node.start_position();
    Some((
        name.to_owned(),
        kind,
        position.row.saturating_add(1),
        position.column.saturating_add(1),
    ))
}

fn ast_extractor(language: LanguageId) -> Option<AstExtractor> {
    let (language, select): (tree_sitter::Language, SelectFn) = match language {
        LanguageId::Rust => (tree_sitter_rust::LANGUAGE.into(), rust_select),
        LanguageId::Python => (tree_sitter_python::LANGUAGE.into(), python_select),
        LanguageId::Go => (tree_sitter_go::LANGUAGE.into(), go_select),
        LanguageId::TypeScript => (
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            typescript_select,
        ),
        LanguageId::JavaScript => (tree_sitter_javascript::LANGUAGE.into(), typescript_select),
        LanguageId::C => (tree_sitter_c::LANGUAGE.into(), c_family_select),
        LanguageId::Cpp => (tree_sitter_cpp::LANGUAGE.into(), c_family_select),
        _ => return None,
    };
    Some(AstExtractor { language, select })
}
