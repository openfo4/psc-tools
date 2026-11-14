mod lexer;
mod parser;

use core::fmt::{self, Display, Formatter};
use core::range::Range;

pub use pex;
use pex::Module;

use crate::lexer::tokenize;
use crate::parser::Parser;

#[derive(Clone, Debug)]
pub struct Error {
    msg: String,
    span: Range<usize>,
}

impl Error {
    /// Returns the span in the input string where the error occured.
    pub fn span(&self) -> Range<usize> {
        self.span
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&self.msg)
    }
}

pub fn assemble(input: &str) -> Result<Module, Error> {
    let tokens = tokenize(input)?;
    let parser = Parser::new(&tokens);
    let module = parser.parse()?;
    Ok(module)
}
