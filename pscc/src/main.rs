use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use codespan_reporting::diagnostic::{Diagnostic, Label};
use codespan_reporting::files::SimpleFiles;
use codespan_reporting::term::termcolor::{ColorChoice, StandardStream};
use codespan_reporting::term::{self, Config};
use pscc::{Compiler, Error};

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

    let mut writer = ErrorWriter::new();
    let mut compiler = Compiler::new();

    for path in &args.import {
        if let Err(err) = compiler.add_file_source(path) {
            writer.write(&err);
            return ExitCode::FAILURE;
        }
    }

    let Ok(output) = compiler.compile_file(&args.input).inspect_err(|errs| {
        for err in &**errs {
            writer.write(err);
        }
    }) else {
        return ExitCode::FAILURE;
    };

    let bytes = output.write();

    let filename = args.input.file_name().unwrap();

    std::fs::create_dir_all(&args.out_dir).unwrap();
    let mut dst = args.out_dir.join(filename);
    dst.set_extension("pex");

    std::fs::write(dst, bytes).unwrap();

    ExitCode::SUCCESS
}

struct ErrorWriter {
    db: SimpleFiles<String, String>,
    writer: StandardStream,
    config: Config,
    files: HashMap<PathBuf, usize>,
}

impl ErrorWriter {
    fn new() -> Self {
        Self {
            db: SimpleFiles::new(),
            writer: StandardStream::stderr(ColorChoice::Auto),
            config: Config::default(),
            files: HashMap::new(),
        }
    }

    fn write(&mut self, err: &Error) {
        let path = err.path().unwrap();
        let id = match self.files.get(path) {
            Some(id) => *id,
            None => {
                let contents = std::fs::read_to_string(path).unwrap();
                let id = self.db.add(path.to_str().unwrap().to_owned(), contents);
                self.files.insert(path.to_owned(), id);

                id
            }
        };

        let mut diag = Diagnostic::error()
            .with_message(err)
            .with_label(Label::primary(id, err.span()));

        for hint in err.hints() {
            let label = Label::secondary(id, hint.span()).with_message(hint);
            diag = diag.with_label(label);
        }

        for note in err.notes() {
            diag = diag.with_note(note);
        }

        term::emit_to_write_style(&mut self.writer.lock(), &self.config, &self.db, &diag).ok();
    }
}
