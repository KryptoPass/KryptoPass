//! Thin layer over `roxmltree` that enforces the namespace and attribute rules.
//!
//! Elements of the template namespace must be known to the caller; elements of
//! any other namespace are skipped so external tools can annotate files; an
//! element with no namespace is an error, since it is almost always a typo.
//! Names are always passed as vocabulary enums, never as text.

use super::error::{ParseError, ParseErrorKind};
use super::vocab::{FALSE, TEXT_CONTENT, TRUE, Tag, XML_LANG, XML_LANG_LOCAL};
use crate::model::Named;
use roxmltree::{Document, NS_XML_URI, Node, ParsingOptions};

/// Namespace of the template format; the trailing number is the format version.
pub(super) const NS: &str = "https://kryptopass.org/ns/template/1";

// The limits below bound the work done on an untrusted document (user templates
// can arrive through Sync). The numbers are generous for real templates, the
// largest of which is a few KiB; they are not derived from a measurement.

/// Largest accepted document, in bytes.
const MAX_BYTES: usize = 256 * 1024;
/// Largest accepted number of XML nodes.
const MAX_NODES: u32 = 20_000;
/// Longest accepted text content, in bytes.
const MAX_TEXT_BYTES: usize = 1024;

pub(super) fn open(xml: &str) -> Result<Document<'_>, ParseError> {
    if xml.len() > MAX_BYTES {
        return Err(ParseError {
            kind: ParseErrorKind::TooLarge,
            line: 0,
        });
    }
    // Refusing DTDs removes custom entities, so entity-expansion attacks have
    // nothing to expand; `roxmltree` also guards against them on its own.
    let options = ParsingOptions {
        allow_dtd: false,
        nodes_limit: MAX_NODES,
    };
    Document::parse_with_options(xml, options).map_err(|error| ParseError {
        kind: ParseErrorKind::Xml(error.to_string()),
        line: 0,
    })
}

/// An element of the template namespace.
///
/// `'a` is how long the parsed document is borrowed and `'i` is the lifetime of
/// the input text it points into (`roxmltree` keeps both). Names and attribute
/// values returned from here borrow the document, not the `El`, which is why
/// they are `&'a str`: callers can keep them after the `El` goes out of scope.
///
/// It is `Copy` because `Node` is a small handle (a reference and an index), so
/// passing `El` by value around the parser costs nothing.
#[derive(Clone, Copy)]
pub(super) struct El<'a, 'i> {
    node: Node<'a, 'i>,
}

impl<'a, 'i> El<'a, 'i> {
    /// The document root, which must be `tag` in the template namespace.
    pub(super) fn root(doc: &'a Document<'i>, tag: Tag) -> Result<Self, ParseError> {
        let node = doc.root_element();
        let name = node.tag_name();
        if name.namespace() == Some(NS) && name.name() == tag.name() {
            Ok(Self { node })
        } else {
            Err(Self { node }.error(ParseErrorKind::WrongRoot {
                expected: tag.name(),
            }))
        }
    }

    /// The element name as written in the document.
    pub(super) fn name(&self) -> &'a str {
        self.node.tag_name().name()
    }

    pub(super) fn is(&self, tag: Tag) -> bool {
        self.name() == tag.name()
    }

    pub(super) fn error(&self, kind: ParseErrorKind) -> ParseError {
        let line = self
            .node
            .document()
            .text_pos_at(self.node.range().start)
            .row;
        ParseError { kind, line }
    }

    pub(super) fn invalid(&self, attribute: impl Named) -> ParseError {
        self.invalid_named(attribute.name())
    }

    pub(super) fn invalid_text(&self) -> ParseError {
        self.invalid_named(TEXT_CONTENT)
    }

    pub(super) fn invalid_lang(&self) -> ParseError {
        self.invalid_named(XML_LANG)
    }

    fn invalid_named(&self, attribute: &'static str) -> ParseError {
        self.error(ParseErrorKind::InvalidValue {
            element: self.name().to_owned(),
            attribute,
        })
    }

    /// Child elements of the template namespace, in document order.
    ///
    /// Returns a `Vec` and not an iterator because a child with no namespace is an
    /// error: collecting up front lets that error surface with `?` before the
    /// caller starts using any child. Elements of other namespaces are skipped
    /// here so every caller ignores annotations from other tools the same way.
    pub(super) fn children(&self) -> Result<Vec<El<'a, 'i>>, ParseError> {
        let mut out = Vec::new();
        for node in self.node.children().filter(Node::is_element) {
            match node.tag_name().namespace() {
                Some(NS) => out.push(El { node }),
                None => {
                    let name = node.tag_name().name().to_owned();
                    return Err(El { node }.error(ParseErrorKind::UnexpectedElement(name)));
                }
                Some(_) => {}
            }
        }
        Ok(out)
    }

    /// The only child named `tag`; errors if it is missing or repeated.
    pub(super) fn one(&self, tag: Tag) -> Result<El<'a, 'i>, ParseError> {
        self.optional(tag)?.ok_or_else(|| {
            self.error(ParseErrorKind::MissingElement {
                parent: self.name().to_owned(),
                name: tag.name(),
            })
        })
    }

    // `impl Named` and not `Tag`: the catalog looks groups up by `MessageScope`,
    // which is a different vocabulary with the same need.
    pub(super) fn optional(&self, tag: impl Named) -> Result<Option<El<'a, 'i>>, ParseError> {
        let mut found = None;
        for child in self.children()? {
            if child.name() == tag.name() {
                if found.is_some() {
                    return Err(
                        child.error(ParseErrorKind::DuplicateElement(tag.name().to_owned()))
                    );
                }
                found = Some(child);
            }
        }
        Ok(found)
    }

    /// Children named `tag`, in order.
    pub(super) fn all(&self, tag: Tag) -> Result<Vec<El<'a, 'i>>, ParseError> {
        Ok(self
            .children()?
            .into_iter()
            .filter(|child| child.is(tag))
            .collect())
    }

    /// Rejects any child element outside `allowed`.
    pub(super) fn check_children<N: Named>(&self, allowed: &[N]) -> Result<(), ParseError> {
        for child in self.children()? {
            if !allowed.iter().any(|name| name.name() == child.name()) {
                return Err(child.error(ParseErrorKind::UnexpectedElement(child.name().to_owned())));
            }
        }
        Ok(())
    }

    pub(super) fn no_children(&self) -> Result<(), ParseError> {
        self.check_children::<Tag>(&[])
    }

    /// Rejects attributes outside `allowed`. Namespaced ones, like `xml:lang`, pass.
    ///
    /// Attributes in a namespace are skipped on purpose: `xml:lang` is one, and
    /// letting any namespaced attribute through is what allows other tools to
    /// annotate a file without breaking it. An attribute with no namespace that
    /// is not listed is almost certainly a typo, so it is an error.
    pub(super) fn check_attrs<A: Named>(&self, allowed: &[A]) -> Result<(), ParseError> {
        for attribute in self.node.attributes() {
            if attribute.namespace().is_none()
                && !allowed.iter().any(|name| name.name() == attribute.name())
            {
                return Err(self.error(ParseErrorKind::UnexpectedAttribute {
                    element: self.name().to_owned(),
                    name: attribute.name().to_owned(),
                }));
            }
        }
        Ok(())
    }

    pub(super) fn no_attrs(&self) -> Result<(), ParseError> {
        self.check_attrs::<NoNames>(&[])
    }

    /// Attributes without a namespace, for elements whose attribute names are data.
    ///
    /// Only `<generator ref=... length=...>` needs this: the parameter names are
    /// defined in the generators file, not in this format, so no enum can list them.
    pub(super) fn plain_attrs(&self) -> Vec<(&'a str, &'a str)> {
        self.node
            .attributes()
            .filter(|attribute| attribute.namespace().is_none())
            .map(|attribute| (attribute.name(), attribute.value()))
            .collect()
    }

    pub(super) fn attr(&self, attribute: impl Named) -> Option<&'a str> {
        self.node.attribute(attribute.name())
    }

    pub(super) fn require<A: Named>(&self, attribute: A) -> Result<&'a str, ParseError> {
        self.attr(attribute).ok_or_else(|| {
            self.error(ParseErrorKind::MissingAttribute {
                element: self.name().to_owned(),
                name: attribute.name(),
            })
        })
    }

    // `xml:lang` lives in the XML namespace, so it cannot be read by the plain name
    // `lang`: the lookup needs the namespace URI and the local name together.
    pub(super) fn lang(&self) -> Option<&'a str> {
        self.node.attribute((NS_XML_URI, XML_LANG_LOCAL))
    }

    /// `true` or `false`; `None` when the attribute is absent.
    ///
    /// Anything else is an error, not `false`: a misspelled `copy="ture"` should
    /// fail loudly instead of silently turning the feature off.
    pub(super) fn flag(&self, attribute: impl Named) -> Result<Option<bool>, ParseError> {
        match self.attr(attribute) {
            None => Ok(None),
            Some(TRUE) => Ok(Some(true)),
            Some(FALSE) => Ok(Some(false)),
            Some(_) => Err(self.invalid(attribute)),
        }
    }

    pub(super) fn number<T: std::str::FromStr>(
        &self,
        attribute: impl Named,
    ) -> Result<Option<T>, ParseError> {
        self.attr(attribute)
            .map(|raw| raw.parse().map_err(|_| self.invalid(attribute)))
            .transpose()
    }

    pub(super) fn require_number<T: std::str::FromStr>(
        &self,
        attribute: impl Named,
    ) -> Result<T, ParseError> {
        self.require(attribute)?
            .parse()
            .map_err(|_| self.invalid(attribute))
    }

    /// The attribute read as a member of vocabulary `V`; `None` when absent.
    ///
    /// Two type parameters on purpose: the attribute being read (`A`, only for its
    /// name in errors) and the vocabulary its value must belong to (`V`).
    pub(super) fn vocab<V: Named>(&self, attribute: impl Named) -> Result<Option<V>, ParseError> {
        self.attr(attribute)
            .map(|raw| V::lookup(raw).ok_or_else(|| self.invalid(attribute)))
            .transpose()
    }

    pub(super) fn require_vocab<V: Named>(&self, attribute: impl Named) -> Result<V, ParseError> {
        V::lookup(self.require(attribute)?).ok_or_else(|| self.invalid(attribute))
    }

    /// Non-empty trimmed text content of a leaf element.
    pub(super) fn text(&self) -> Result<String, ParseError> {
        self.no_children()?;
        let text = self.node.text().unwrap_or_default().trim();
        if text.is_empty() || text.len() > MAX_TEXT_BYTES {
            return Err(self.invalid_text());
        }
        Ok(text.to_owned())
    }

    /// Fails if the element has attributes beyond `allowed`, children or text.
    pub(super) fn expect_empty<A: Named>(&self, allowed: &[A]) -> Result<(), ParseError> {
        self.check_attrs(allowed)?;
        self.no_children()?;
        if self.node.text().is_some_and(|text| !text.trim().is_empty()) {
            return Err(self.invalid_text());
        }
        Ok(())
    }

    pub(super) fn expect_bare(&self) -> Result<(), ParseError> {
        self.expect_empty::<NoNames>(&[])
    }
}

/// Placeholder vocabulary for calls that allow no attributes at all.
///
/// `check_attrs` is generic, so an empty `&[]` needs some `Named` type to infer.
/// This enum has no variants, so no value of it can exist: the empty slice is the
/// only slice possible and `match self {}` in `name` is accepted by the compiler
/// as exhaustive without a fallback arm. It avoids borrowing a real vocabulary
/// just to say "nothing is allowed".
#[derive(Clone, Copy)]
pub(super) enum NoNames {}

impl Named for NoNames {
    fn name(self) -> &'static str {
        match self {}
    }

    fn lookup(_: &str) -> Option<Self> {
        None
    }
}
