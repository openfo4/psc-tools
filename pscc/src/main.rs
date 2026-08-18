mod ast;
mod codegen;
mod lexer;
mod parser;

use std::path::PathBuf;

use clap::Parser;
use codespan_reporting::diagnostic::{Diagnostic, Label};
use codespan_reporting::files::SimpleFiles;
use codespan_reporting::term::termcolor::{ColorChoice, StandardStream};
use codespan_reporting::term::{self, Config};

#[derive(Clone, Debug, Parser)]
struct Args {
    input: PathBuf,
}

fn main() {
    let args = Args::parse();

    let file = std::fs::read_to_string(&args.input).unwrap();

    let mut files = SimpleFiles::new();
    let file_id = files.add(args.input.to_string_lossy(), &file);

    let writer = StandardStream::stderr(ColorChoice::Auto);
    let mut config = Config::default();
    config.before_label_lines = 1;
    config.after_label_lines = 1;

    let tokens = match lexer::tokenize(&file) {
        Ok(tokens) => tokens,
        Err(err) => {
            let diag = Diagnostic::error()
                .with_message(err.kind)
                .with_code("E23")
                .with_label(Label::primary(file_id, err.span));

            term::emit_to_write_style(&mut writer.lock(), &config, &files, &diag).unwrap();

            return;
        }
    };

    let mut parser = parser::Parser::new(&tokens);
    let Ok(ast) = parser.parse().inspect_err(|err| {
        let diag = Diagnostic::error()
            .with_message(&err.kind)
            .with_code("E24")
            .with_label(Label::primary(file_id, err.span.clone()).with_message(&err.kind));

        term::emit_to_write_style(&mut writer.lock(), &config, &files, &diag).unwrap();
    }) else {
        return;
    };

    for func in &ast.functions {
        dbg!(&func.name);

        if func.name == "OnInit" {
            let cfg = codegen::lower_function(func);
            dbg!(&func);
            println!("{}", cfg);
        }
    }
}
