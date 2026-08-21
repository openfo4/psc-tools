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
use crate::codegen::CodegenOptions;
use crate::registry::{ClassInfo, Registry};

#[derive(Clone, Debug, Parser)]
struct Args {
    input: PathBuf,
    /// Directory or file to import external definitions.
    #[arg(short, long)]
    import: Vec<PathBuf>,
    /// Output directory for compiled scripts.
    #[arg(long, default_value = ".")]
    out_dir: PathBuf,
}

fn main() -> ExitCode {
    let args = Args::parse();

    let mut db = SimpleFiles::new();
    let writer = StandardStream::stderr(ColorChoice::Auto);
    let config = Config::default();

    let mut files = args.import;
    files.push(args.input.clone());

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

    let mut registry = Registry::default();

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

        registry.classes.insert(ast.script_name.clone(), info);

        ast_to_compile = Some(ast);
    }

    let Some(ast) = ast_to_compile else {
        return ExitCode::SUCCESS;
    };

    let filename = args.input.file_name().unwrap();

    let module = compile_module(&ast, &registry);
    let bytes = module.write();

    std::fs::create_dir_all(&args.out_dir).unwrap();
    let mut dst = args.out_dir.join(filename);
    dst.set_extension("pex");

    std::fs::write(dst, bytes).unwrap();

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

fn compile_module(ast: &Ast, registry: &Registry) -> pex::Module {
    let mut strings = StringBuilder::default();

    let mut functions = Vec::new();
    for func in &ast.functions {
        let function = codegen::lower_function(
            &ast,
            func,
            &mut strings,
            &CodegenOptions {
                preserve_parameter_names: true,
            },
            registry,
        );
        functions.push(NamedFunction {
            name: strings.insert(&func.name),
            function,
        });
    }

    let mut properties = Vec::new();
    let mut globals = Vec::new();
    for prop in ast.properties.iter().chain(
        ast.property_groups
            .iter()
            .flat_map(|group| &group.properties),
    ) {
        let (prop, glob) = codegen::lower_property(prop, &mut strings);
        properties.push(prop);
        globals.push(glob);
    }

    for glob in &ast.globals {
        let glob = codegen::lower_global(glob, &mut strings);
        globals.push(glob);
    }

    let classes = vec![Class {
        name: strings.insert(&ast.script_name),
        parent: strings.insert(ast.extends.as_ref().map(|v| v.as_str()).unwrap_or("none")),
        docstring: strings.insert(""),
        is_const: false,
        user_flags: 0,
        auto_state_name: strings.insert(""),
        structs: Vec::new(),
        variables: globals,
        properties,
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
