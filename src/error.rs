use failure::Fail;
use std::fmt;

#[derive(Debug, Fail)]
pub enum Error {
    ExecutionError(String),
    CompilationError(Vec<String>),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Error::ExecutionError(e) => writeln!(f, "execution error:\n{}", e),
            Error::CompilationError(logs) => {
                let joined = logs.join("\n----------\n");
                writeln!(f, "compilation error:\n{}", joined)
            }
        }
    }
}
