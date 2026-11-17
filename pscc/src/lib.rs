mod ast;
mod codegen;
mod lexer;
mod parser;
mod registry;
mod typecheck;

use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};
use std::io;
use std::ops::{Deref, Range};
use std::path::{Path, PathBuf};

use pex::{Class, Module, NamedFunction, State, StringKey, StringTable};

use crate::ast::Ast;
use crate::codegen::CodegenOptions;
use crate::lexer::tokenize;
use crate::parser::Parser;
use crate::registry::{ClassInfo, Registry, StructInfo};

/// The main driver for the compiler.
#[derive(Debug, Default)]
pub struct Compiler {
    registry: Registry,
}

impl Compiler {
    /// Creates a new `Compiler`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a new import source to load from the given [`Path`].
    ///
    /// The [`Path`] may both be a single file or a directory, in which case the entire directory
    /// will be recursively scanned.
    ///
    /// # Errors
    ///
    /// Returns an error if the given [`Path`] cannot be accessed, or an invalid file was loaded.
    /// Note that the imported files are only parsed, not typechecked.
    pub fn add_file_source<P>(&mut self, path: P) -> Result<(), Error>
    where
        P: AsRef<Path>,
    {
        // self.sources.extend(enumate_dir_recursive(path).unwrap());

        for source in enumate_dir_recursive(&path).map_err(|err| Error::from_msg(err))? {
            self.add_source(&source).map_err(|mut err| {
                err.path = Some(path.as_ref().to_owned());
                err
            })?;
        }

        Ok(())
    }

    /// Adds a new import source from an in-memory file.
    ///
    /// # Errors
    ///
    /// Returns an error if the given `file` cannot be parsed.
    pub fn add_source(&mut self, file: &str) -> Result<(), Error> {
        self.parse_register(&file)?;
        Ok(())
    }

    /// Compile the module from the given file [`Path`].
    ///
    /// The given [`Path`] must point to a file.
    pub fn compile_file<P>(&mut self, path: P) -> Result<Module, ErrorList>
    where
        P: AsRef<Path>,
    {
        let file =
            std::fs::read_to_string(&path).map_err(|err| ErrorList::from(Error::from_msg(err)))?;
        self.compile(&file).map_err(|mut errs| {
            for err in &mut errs.0 {
                err.path = Some(path.as_ref().to_owned());
            }

            errs
        })
    }

    /// Compile the module in the given `file`.
    ///
    /// # Errors
    ///
    /// Returns an error if the given `file` cannot be compiled. Note that multiple errors can be
    /// returned at once. See [`Error`] for more information on how to handle returned errors.
    pub fn compile(&mut self, file: &str) -> Result<Module, ErrorList> {
        let ast = self
            .parse_register(file)
            .map_err(|err| ErrorList(vec![err]))?;

        if let Err(errs) = typecheck::typecheck(&self.registry, &ast) {
            let list = errs
                .into_iter()
                .map(|err| {
                    let notes = err.kind.notes();
                    let hints = err
                        .kind
                        .help()
                        .into_iter()
                        .map(|spanned| Hint {
                            msg: spanned.value,
                            span: spanned.span,
                        })
                        .collect();

                    Error {
                        msg: err.kind.to_string(),
                        span: err.span,
                        hints,
                        notes,
                        inline_msg: err.kind.error_class().to_owned(),
                        path: None,
                    }
                })
                .collect::<Vec<_>>();
            return Err(ErrorList(list));
        }

        Ok(compile_module(&ast, &self.registry))
    }

    fn parse_register(&mut self, input: &str) -> Result<Ast, Error> {
        let ast = parse_ast(input)?;

        let mut info = ClassInfo::default();

        for global in &ast.globals {
            info.globals
                .insert(global.name.value.clone(), global.ty.value.clone());
        }

        for prop in &ast.properties {
            info.properties
                .insert(prop.name.clone(), prop.ty.value.clone());
        }

        for group in &ast.property_groups {
            for prop in &group.properties {
                info.properties
                    .insert(prop.name.clone(), prop.ty.value.clone());
            }
        }

        for st in &ast.structs {
            self.registry.structs.insert(
                st.name.clone(),
                StructInfo {
                    members: st
                        .members
                        .iter()
                        .map(|def| (def.name.value.clone(), def.ty.value.clone()))
                        .collect(),
                },
            );
        }

        for func in &ast.functions {
            info.functions.insert(func.name.clone(), func.clone());
        }

        self.registry.classes.insert(ast.script_name.clone(), info);

        Ok(ast)
    }
}

#[derive(Debug)]
pub struct ErrorList(Vec<Error>);

impl Deref for ErrorList {
    type Target = [Error];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Error> for ErrorList {
    fn from(value: Error) -> Self {
        Self(vec![value])
    }
}

/// An error that occured during script compilation.
///
/// # Rendering the error
///
/// An [`Error`] comes with a few properties:
/// - A message with the main error message (exposed via the [`Display`]` implementation).
/// - A span in the source file where the error occured.
/// - An inline message that should be rendered at the span.
/// - A list of optional hints, that point to other locations related to the error.
/// - A list of optional note messages, with potential solutions to the error.
///
/// Here is how the output may look like using the [`codespan-reporting`] crate:
/// ```text
/// error: mismatched types: expected `int`, found `bool`        <-- Message
///   ┌─ tests/psc/ShortCircuit.psc:8:12
///   │
/// 7 │ int function X()
///   │ --- type `int` expected due to definition here           <-- Hint
/// 8 │     return true
///   │            ^^^^ mismatched types                         <-- Inline Message
///   │
///   = help: Consider returning an `int` instead :) <-- Help    <-- Note
/// ```
///
/// [`codespan-reporting`]: https://docs.rs/codespan-reporting
#[derive(Debug)]
pub struct Error {
    msg: String,
    span: Range<usize>,
    hints: Vec<Hint>,
    notes: Vec<String>,
    inline_msg: String,
    path: Option<PathBuf>,
}

impl Error {
    /// Returns the location in the source where the error occured.
    pub fn span(&self) -> Range<usize> {
        self.span.clone()
    }

    pub fn inline_message(&self) -> &str {
        &self.inline_msg
    }

    /// Returns an optiona list of additional hints.
    pub fn hints(&self) -> &[Hint] {
        &self.hints
    }

    pub fn notes(&self) -> &[String] {
        &self.notes
    }

    /// The file path where the error occured.
    ///
    /// If the source was not loaded from a file, `None` is returned.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    fn from_msg(msg: impl ToString) -> Self {
        Self {
            msg: msg.to_string(),
            span: Range::default(),
            hints: Vec::new(),
            notes: Vec::new(),
            inline_msg: String::new(),
            path: None,
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&self.msg)
    }
}

#[derive(Debug)]
pub struct Hint {
    msg: String,
    span: Range<usize>,
}

impl Hint {
    /// Returns the location in the source where the hint should be shown.
    pub fn span(&self) -> Range<usize> {
        self.span.clone()
    }
}

impl Display for Hint {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&self.msg)
    }
}

fn parse_ast(input: &str) -> Result<Ast, Error> {
    let tokens = tokenize(input).map_err(|err| Error {
        msg: err.kind.to_string(),
        span: err.span,
        hints: Vec::new(),
        notes: Vec::new(),
        inline_msg: err.kind.to_string(),
        path: None,
    })?;
    let mut parser = Parser::new(&tokens);
    let ast = parser.parse().map_err(|err| Error {
        msg: err.kind.to_string(),
        span: err.span,
        hints: Vec::new(),
        notes: Vec::new(),
        inline_msg: err.kind.to_string(),
        path: None,
    })?;

    Ok(ast)
}

#[derive(Clone, Debug, Default)]
struct StringBuilder {
    strings: StringTable,
    table: HashMap<String, StringKey>,
}

impl StringBuilder {
    fn insert(&mut self, s: &str) -> StringKey {
        if let Some(key) = self.table.get(s) {
            return *key;
        }

        let key = self.strings.insert(s.to_owned().into());
        self.table.insert(s.to_owned(), key);
        key
    }
}

fn enumate_dir_recursive<P>(path: P) -> io::Result<Vec<String>>
where
    P: AsRef<Path>,
{
    let mut queue = vec![path.as_ref().to_owned()];
    let mut output = Vec::new();

    while let Some(path) = queue.pop() {
        let meta = path.metadata()?;

        if meta.file_type().is_dir() {
            for entry in std::fs::read_dir(&path)? {
                let entry = entry?;
                queue.push(entry.path());
            }
        }

        if meta.file_type().is_file() {
            let buf = std::fs::read_to_string(&path)?;
            output.push(buf);
        }
    }

    Ok(output)
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
