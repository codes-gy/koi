use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct KoiError {
    line: Option<usize>,
    message: String,
    incomplete: bool,
}

impl KoiError {
    pub(crate) fn new(line: usize, message: impl Into<String>) -> Self {
        Self {
            line: Some(line),
            message: message.into(),
            incomplete: false,
        }
    }

    /// The input stopped in the middle of a construct, so more lines could still complete it.
    pub(crate) fn incomplete(line: usize, message: impl Into<String>) -> Self {
        Self {
            incomplete: true,
            ..Self::new(line, message)
        }
    }

    pub(crate) fn runtime(message: impl Into<String>) -> Self {
        Self {
            line: None,
            message: message.into(),
            incomplete: false,
        }
    }

    pub(crate) fn at_line(mut self, line: usize) -> Self {
        self.line.get_or_insert(line);
        self
    }

    pub(crate) fn complete(mut self) -> Self {
        self.incomplete = false;
        self
    }

    pub fn is_incomplete(&self) -> bool {
        self.incomplete
    }
}

impl fmt::Display for KoiError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "줄 {line}: {}", self.message),
            None => write!(f, "{}", self.message),
        }
    }
}

impl std::error::Error for KoiError {}
