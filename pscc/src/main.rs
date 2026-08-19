mod ast;
mod codegen;
mod lexer;
mod parser;
mod registry;

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use codespan_reporting::diagnostic::{Diagnostic, Label};
use codespan_reporting::files::SimpleFiles;
use codespan_reporting::term::termcolor::{ColorChoice, StandardStream};
use codespan_reporting::term::{self, Config};
use pex::{Class, Module, NamedFunction, State, StringKey, StringTable};

use crate::ast::Ast;
use crate::registry::{ClassInfo, Registry};

#[derive(Clone, Debug, Parser)]
struct Args {
    input: PathBuf,
    #[arg(short, long)]
    import: Vec<PathBuf>,
}

fn main() -> ExitCode {
    let args = Args::parse();

    let mut db = SimpleFiles::new();
    let writer = StandardStream::stderr(ColorChoice::Auto);
    let config = Config::default();

    let mut files = args.import;
    files.push(args.input);

    let mut data = Vec::new();
    for path in &files {
        let s = std::fs::read_to_string(path).unwrap();
        let id = db.add(path.to_string_lossy(), s.clone());
        data.push((s, id));
    }

    let mut file_tokens = Vec::new();
    for (s, id) in &data {
        let Ok(tokens) = lexer::tokenize(s).inspect_err(|err| {
            let diag = Diagnostic::error()
                .with_message(&err.kind)
                .with_label(Label::primary(*id, err.span.clone()));
            term::emit_to_write_style(&mut writer.lock(), &config, &db, &diag).unwrap();
        }) else {
            return ExitCode::FAILURE;
        };
        file_tokens.push(tokens);
    }

    let mut ast_to_compile = None;

    for ((_, id), tokens) in data.iter().zip(file_tokens) {
        let mut parser = parser::Parser::new(&tokens);
        let Ok(ast) = parser.parse().inspect_err(|err| {
            dbg!(&err.span);
            let diag = Diagnostic::error()
                .with_message(&err.kind)
                .with_label(Label::primary(*id, err.span.clone()).with_message(&err.kind));
            term::emit_to_write_style(&mut writer.lock(), &config, &db, &diag).unwrap();
        }) else {
            return ExitCode::FAILURE;
        };

        let mut info = ClassInfo::default();
        for prop in &ast.properties {
            info.properties.insert(prop.name.clone(), prop.ty.clone());
        }

        for group in &ast.property_groups {
            for prop in &group.properties {
                info.properties.insert(prop.name.clone(), prop.ty.clone());
            }
        }

        ast_to_compile = Some(ast);
    }

    let Some(ast) = ast_to_compile else {
        return ExitCode::SUCCESS;
    };

    let module = compile_module(&ast);
    let bytes = module.write();
    std::fs::write("a.pex", bytes).unwrap();

    ExitCode::SUCCESS
}

#[derive(Clone, Debug, Default)]
pub struct StringBuilder {
    strings: StringTable,
    table: HashMap<String, StringKey>,
}

impl StringBuilder {
    pub fn insert(&mut self, s: &str) -> StringKey {
        if let Some(key) = self.table.get(s) {
            return *key;
        }

        let key = self.strings.insert(s.to_owned().into());
        self.table.insert(s.to_owned(), key);
        key
    }
}

fn compile_module(ast: &Ast) -> pex::Module {
    let mut strings = StringBuilder::default();

    let mut functions = Vec::new();
    for func in &ast.functions {
        let function = codegen::lower_function(&ast, func, &mut strings);
        functions.push(NamedFunction {
            name: strings.insert(&func.name),
            function,
        });
    }

    let classes = vec![Class {
        name: strings.insert(&ast.script_name),
        parent: strings.insert(&ast.extends),
        docstring: strings.insert(""),
        is_const: false,
        user_flags: 0,
        auto_state_name: strings.insert(""),
        structs: Vec::new(),
        variables: Vec::new(),
        properties: Vec::new(),
        states: vec![State {
            name: strings.insert(""),
            functions,
        }],
    }];

    pex::Module {
        endian: pex::Endianess::Little,
        classes,
        strings: strings.strings,
    }
}
