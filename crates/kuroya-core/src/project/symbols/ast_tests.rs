use super::ast_symbols::extract_ast_symbols;
use crate::LanguageId;

#[test]
fn rust_ast_extracts_declarations_with_positions() {
    let source = r#"
mod engine {
    pub struct Piston;
    impl Piston {
        pub fn fire(&self) {}
    }
    pub trait Coolable { fn chill(&self); }
    const MAX_RPM: u32 = 9000;
}
fn read_input() {}
"#;
    let symbols = extract_ast_symbols(LanguageId::Rust, source, 128).expect("valid rust parses");
    let names: Vec<&str> = symbols.iter().map(|(name, ..)| name.as_str()).collect();
    for expected in [
        "engine",
        "Piston",
        "fire",
        "Coolable",
        "chill",
        "MAX_RPM",
        "read_input",
    ] {
        assert!(names.contains(&expected), "{expected} missing: {names:?}");
    }
    // Trait methods are found through the trait body, impl methods through
    // the impl body, and the fire/chill identifiers are not duplicated.
    assert_eq!(names.iter().filter(|name| **name == "fire").count(), 1);
}

#[test]
fn python_ast_extracts_functions_and_classes() {
    let source = r#"
class Engine:
    def fire(self):
        pass

def read_input():
    pass
"#;
    let symbols = extract_ast_symbols(LanguageId::Python, source, 32).expect("valid python parses");
    let names: Vec<&str> = symbols.iter().map(|(name, ..)| name.as_str()).collect();
    assert_eq!(names, vec!["Engine", "fire", "read_input"]);
}

#[test]
fn go_ast_extracts_functions_types_and_consts() {
    let source = r#"
package engine

const MaxRPM = 9000

type Piston struct {
    Bore int
}

type Cooler interface {
    Chill()
}

func (p *Piston) Fire() {}
func ReadInput() {}
"#;
    let symbols = extract_ast_symbols(LanguageId::Go, source, 32).expect("valid go parses");
    let names: Vec<&str> = symbols.iter().map(|(name, ..)| name.as_str()).collect();
    for expected in ["MaxRPM", "Piston", "Cooler", "Fire", "ReadInput"] {
        assert!(names.contains(&expected), "{expected} missing: {names:?}");
    }
}

#[test]
fn typescript_ast_extracts_declarations_and_const_variables() {
    let source = r#"
export class Engine {
    fire(): void {}
}
export interface Cooler { chill(): void; }
export type Rpm = number;
export enum Stroke { Up }
const MAX_RPM = 9000;
function readInput(): void {}
"#;
    let symbols = extract_ast_symbols(LanguageId::TypeScript, source, 32).expect("valid ts parses");
    let names: Vec<&str> = symbols.iter().map(|(name, ..)| name.as_str()).collect();
    for expected in [
        "Engine",
        "fire",
        "Cooler",
        "Rpm",
        "Stroke",
        "MAX_RPM",
        "readInput",
    ] {
        assert!(names.contains(&expected), "{expected} missing: {names:?}");
    }
}

#[test]
fn c_ast_extracts_functions_structs_and_typedefs() {
    let source = r#"
struct Piston { int bore; };
typedef struct Piston PistonT;
enum Stroke { UP };
static const int MAX_RPM = 9000;
void read_input(void) {}
"#;
    let symbols = extract_ast_symbols(LanguageId::C, source, 32).expect("valid c parses");
    let names: Vec<&str> = symbols.iter().map(|(name, ..)| name.as_str()).collect();
    for expected in ["Piston", "PistonT", "Stroke", "read_input"] {
        assert!(names.contains(&expected), "{expected} missing: {names:?}");
    }
}

#[test]
fn ast_extraction_respects_the_symbol_cap() {
    let mut source = String::new();
    for index in 0..64 {
        source.push_str(&format!("fn symbol_{index}() {{}}\n"));
    }
    let symbols = extract_ast_symbols(LanguageId::Rust, &source, 10).expect("valid rust parses");
    assert_eq!(symbols.len(), 10);
}

#[test]
fn ast_extraction_is_none_without_a_registered_grammar() {
    assert_eq!(
        extract_ast_symbols(LanguageId::Dart, "class X {}", 8),
        None,
        "languages without a registered grammar fall back to line scanning"
    );
}
