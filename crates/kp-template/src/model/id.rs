//! Identifier newtypes.
//!
//! Each wraps a `String` that is private, so a value of these types is only
//! ever built by the parser after validating it. Code holding one can rely on
//! that without re-checking.

macro_rules! identifier {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            /// Wraps a string the parser already validated.
            pub(crate) fn from_validated(value: String) -> Self {
                Self(value)
            }

            /// The identifier as written in the template file.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

identifier!(
    /// Stable template id, for example `login` or `user.casa`.
    ///
    /// User templates are namespaced under `user.` so they cannot shadow a
    /// built-in one.
    TemplateId
);
identifier!(
    /// Stable field id. It is the key of the stored value and must never change.
    ///
    /// Renaming a field's label is free; changing its id orphans the data
    /// stored under the old one.
    FieldId
);
identifier!(
    /// Section id, used for grouping fields and as the id of its title in the catalog.
    SectionId
);
identifier!(
    /// Id of a message inside one scope of a [`Catalog`](crate::Catalog), for example `username`.
    ///
    /// It carries no prefix: the scope that says what it translates is a
    /// separate [`MessageScope`](crate::MessageScope).
    MessageId
);
identifier!(
    /// Name of a generator declared in the generators file.
    GeneratorId
);
identifier!(
    /// freedesktop icon name, for example `dialog-password`.
    IconName
);

/// A string that is not a valid identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidId;

impl std::fmt::Display for InvalidId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("invalid identifier")
    }
}

impl std::error::Error for InvalidId {}
