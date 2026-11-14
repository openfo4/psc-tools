use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use codespan_reporting::diagnostic::{Diagnostic, Label};
use codespan_reporting::files::SimpleFiles;
use codespan_reporting::term::termcolor::{ColorChoice, StandardStream};
use codespan_reporting::term::{self, Config};
use pex_as::assemble;

#[derive(Clone, Debug, Parser)]
struct Args {
    input: PathBuf,
    #[arg(long, short = 'o')]
    output: Option<PathBuf>,
}

fn main() -> ExitCode {
    let args = Args::parse();

    let mut db = SimpleFiles::new();
    let writer = StandardStream::stderr(ColorChoice::Auto);
    let config = Config::default();

    let Ok(input) = std::fs::read_to_string(&args.input).inspect_err(|err| {
        eprintln!(
            "failed to read from input file {}: {}",
            args.input.to_string_lossy(),
            err,
        )
    }) else {
        return ExitCode::FAILURE;
    };

    let file_id = db.add(args.input.to_string_lossy(), &input);

    let Ok(module) = assemble(&input).inspect_err(|err| {
        let diag = Diagnostic::error()
            .with_message(err)
            .with_label(Label::primary(file_id, err.span()));
        term::emit_to_write_style(&mut writer.lock(), &config, &db, &diag).unwrap();
    }) else {
        return ExitCode::FAILURE;
    };

    let bytes = module.write();
    let output = args
        .output
        .unwrap_or_else(|| args.input.with_extension("pex"));

    if let Err(err) = std::fs::write(&output, bytes) {
        eprintln!(
            "failed to write output to {}: {}",
            output.to_string_lossy(),
            err
        );
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
