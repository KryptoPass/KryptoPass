//! Parse failures. Messages name elements and attributes, never stored values.

use std::fmt;

/// A failure while reading a template, catalog or generators file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// What went wrong.
    pub kind: ParseErrorKind,
    /// 1-based line of the offending element, or 0 when there is none.
    pub line: u32,
}

/// Reason a document was rejected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseErrorKind {
    /// The text is not well-formed XML, or it uses a DTD.
    Xml(String),
    /// The document exceeds the size limit.
    TooLarge,
    /// The root element is not the expected one.
    WrongRoot {
        /// Name of the element that was expected.
        expected: &'static str,
    },
    /// An element of the template namespace that is not allowed here, or one
    /// with no namespace at all.
    UnexpectedElement(String),
    /// An attribute that is not allowed on this element.
    UnexpectedAttribute {
        /// Element carrying the attribute.
        element: String,
        /// Attribute name.
        name: String,
    },
    /// A required attribute is missing.
    MissingAttribute {
        /// Element missing the attribute.
        element: String,
        /// Attribute name.
        name: &'static str,
    },
    /// A required child element is missing.
    MissingElement {
        /// Parent element.
        parent: String,
        /// Name of the missing child.
        name: &'static str,
    },
    /// An element that may appear once appears again.
    DuplicateElement(String),
    /// An attribute or text value is not valid.
    InvalidValue {
        /// Element carrying the value.
        element: String,
        /// Attribute name, or `text` for element content.
        attribute: &'static str,
    },
    /// An attribute that only applies to another field type.
    Misplaced {
        /// Attribute used.
        attribute: &'static str,
        /// Field type the attribute requires.
        requires: &'static str,
    },
    /// Two items of the same kind share an id.
    DuplicateId {
        /// What kind of item, for example `field`.
        what: &'static str,
        /// The repeated id.
        id: String,
    },
    /// A reference to a field that does not exist in the template.
    UnknownField(String),
    /// Fields that cannot go together in one template, such as a derived
    /// field without the field it derives from.
    Inconsistent(&'static str),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line > 0 {
            write!(f, "line {}: ", self.line)?;
        }
        match &self.kind {
            ParseErrorKind::Xml(message) => write!(f, "invalid XML: {message}"),
            ParseErrorKind::TooLarge => write!(f, "document exceeds the size limit"),
            ParseErrorKind::WrongRoot { expected } => {
                write!(f, "root element must be <{expected}>")
            }
            ParseErrorKind::UnexpectedElement(name) => write!(f, "unexpected element <{name}>"),
            ParseErrorKind::UnexpectedAttribute { element, name } => {
                write!(f, "unexpected attribute `{name}` on <{element}>")
            }
            ParseErrorKind::MissingAttribute { element, name } => {
                write!(f, "<{element}> requires attribute `{name}`")
            }
            ParseErrorKind::MissingElement { parent, name } => {
                write!(f, "<{parent}> requires <{name}>")
            }
            ParseErrorKind::DuplicateElement(name) => write!(f, "<{name}> appears more than once"),
            ParseErrorKind::InvalidValue { element, attribute } => {
                write!(f, "invalid `{attribute}` on <{element}>")
            }
            ParseErrorKind::Misplaced {
                attribute,
                requires,
            } => {
                write!(f, "`{attribute}` is only valid on {requires} fields")
            }
            ParseErrorKind::DuplicateId { what, id } => write!(f, "duplicate {what} id `{id}`"),
            ParseErrorKind::UnknownField(id) => write!(f, "unknown field `{id}`"),
            ParseErrorKind::Inconsistent(reason) => write!(f, "inconsistent fields: {reason}"),
        }
    }
}

impl std::error::Error for ParseError {}
