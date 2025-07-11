//! Error management

use std::convert::From;
use std::fmt;

/// Generic error for all LDraw operations.
#[derive(Debug)]
pub enum Error {
    /// An error encountered while parsing some LDraw file content.
    Parse(ParseError),

    /// An error encountered while resolving a sub-file reference.
    Resolve(ResolveError),
}

/// Error related to parsing the content of an LDraw file.
#[derive(Debug)]
pub struct ParseError {
    /// Filename of the sub-file reference, generally relative to some canonical catalog path(s).
    pub filename: String,

    /// The line of the LDraw file that failed to parse.
    pub line: String,

    /// Optional underlying error raised by the internal parser.
    pub parse_error: Option<Box<dyn std::error::Error>>,
}

/// Error related to resolving a sub-file reference of a source file.
#[derive(Debug)]
pub struct ResolveError {
    /// Filename of the sub-file reference, generally relative to some canonical catalog path(s).
    pub filename: String,

    /// Optional underlying error raised by the resolver implementation.
    pub resolve_error: Option<Box<dyn std::error::Error>>,
}

impl ParseError {
    /// Create a [`ParseError`] that stems from an arbitrary error of an underlying parser.
    pub fn new(filename: &str, line: String, err: impl Into<Box<dyn std::error::Error>>) -> Self {
        Self {
            filename: filename.to_string(),
            line,
            parse_error: Some(err.into()),
        }
    }

    /// Create a [`ParseError`] that stems from a [`nom`] parsing error, capturing the [`nom::error::ErrorKind`]
    /// from the underlying parser which failed.
    pub fn new_from_nom(
        filename: &str,
        line: String,
        err: &nom::Err<nom::error::Error<&[u8]>>,
    ) -> Self {
        Self {
            filename: filename.to_string(),
            line,
            parse_error: match err {
                nom::Err::Incomplete(_) => None,
                nom::Err::Error(e) => {
                    // Discard input slice due to lifetime constraint
                    Some(nom::Err::Error(e.code).into())
                }
                nom::Err::Failure(e) => {
                    // Discard input slice due to lifetime constraint
                    Some(nom::Err::Error(e.code).into())
                }
            },
        }
    }
}

impl ResolveError {
    /// Create a [`ResolveError`] that stems from an arbitrary error of an underlying resolution error.
    pub fn new(filename: String, err: impl Into<Box<dyn std::error::Error>>) -> Self {
        Self {
            filename,
            resolve_error: Some(err.into()),
        }
    }

    /// Create a [`ResolveError`] without any underlying error.
    pub fn new_raw(filename: &str) -> Self {
        Self {
            filename: filename.to_string(),
            resolve_error: None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Parse(ParseError {
                filename,
                line,
                parse_error,
            }) => write!(
                f,
                "parse error in file {filename:?} while processing {line:?}: {parse_error:?}"
            ),
            Error::Resolve(ResolveError {
                filename,
                resolve_error,
            }) => write!(
                f,
                "resolve error for filename {filename:?}: {resolve_error:?}"
            ),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
        // match self {
        //     Error::Parse(ParseError {
        //         filename,
        //         parse_error,
        //     }) => parse_error,
        //     Error::Resolve(ResolveError {
        //         filename,
        //         resolve_error,
        //     }) => resolve_error,
        // }
    }
}

impl From<ResolveError> for Error {
    fn from(e: ResolveError) -> Self {
        Error::Resolve(e)
    }
}

impl From<ParseError> for Error {
    fn from(e: ParseError) -> Self {
        Error::Parse(e)
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    fn get_error() -> Result<u32, Error> {
        let underlying = Error::Parse(ParseError {
            filename: "low_level.ldr".to_string(),
            line: "abc".to_string(),
            parse_error: None,
        });
        Err(Error::Resolve(ResolveError::new(
            "test_file.ldr".to_string(),
            underlying,
        )))
    }

    #[test]
    fn test_error() {
        if let Err(e) = get_error() {
            eprintln!("Error: {e}")
        };
    }

    #[test]
    fn test_new_from_nom() {
        let nom_error = nom::Err::Error(nom::error::Error::new(
            &b""[..],
            nom::error::ErrorKind::Alpha,
        ));
        let parse_error = ParseError::new_from_nom("file", String::new(), &nom_error);
        assert_eq!(parse_error.filename, "file");
        assert!(parse_error.parse_error.is_some());
    }

    #[test]
    fn test_source() {
        let resolve_error = ResolveError::new_raw("file");
        let error: Error = resolve_error.into();
        assert!(std::error::Error::source(&error).is_none());
    }

    #[test]
    fn test_from() {
        let resolve_error = ResolveError::new_raw("file");
        let error: Error = resolve_error.into();
        eprintln!("err: {error}");
        match &error {
            Error::Resolve(resolve_error) => assert_eq!(resolve_error.filename, "file"),
            _ => panic!("Unexpected error type."),
        }

        let parse_error = ParseError::new("file", String::new(), error);
        let error: Error = parse_error.into();
        eprintln!("err: {error}");
        match &error {
            Error::Parse(parse_error) => assert_eq!(parse_error.filename, "file"),
            _ => panic!("Unexpected error type."),
        }
    }
}
