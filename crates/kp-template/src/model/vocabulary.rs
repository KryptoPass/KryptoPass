//! Declaration of closed vocabularies: an enum plus the name each variant has in documents.
//!
//! The name is written once, next to the variant. Reading and writing both go
//! through it, so no other code needs to know the text.
//!
//! Why a macro and not a derive crate: the whole need is one table per enum,
//! `variant => "text"`, and a `match` kept by hand beside the enum would drift
//! the first time a variant is added. A `macro_rules!` keeps the variant and its
//! name on the same line without adding a dependency.

/// Implemented by every vocabulary enum so generic code can take any of them.
///
/// The reader's helpers (`attr`, `check_attrs`, `vocab`...) are generic over
/// this trait. Without it each element's attribute enum would need its own copy
/// of every helper, or the helpers would take `&str` again and the spelling
/// would leak back into the parser.
///
/// It repeats `name` and `from_name` as trait methods, delegating to the inherent
/// ones the macro generates. Inherent methods stay callable without importing
/// the trait (the public enums), and the trait makes them usable generically.
pub(crate) trait Named: Copy {
    /// The name used in documents.
    fn name(self) -> &'static str;

    /// The variant with exactly this name; documents are case-sensitive.
    fn lookup(name: &str) -> Option<Self>;
}

/// Declares a vocabulary enum with `ALL`, `name` and `from_name`.
///
/// Callers may add derives (such as `Default`) and doc comments on variants.
/// The derives every vocabulary needs are added here so declarations stay short;
/// repeating one in a caller's `#[derive]` would be a duplicate-impl error.
macro_rules! vocabulary {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $( $(#[$vmeta:meta])* $variant:ident => $text:literal ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        $vis enum $name {
            $( $(#[$vmeta])* $variant ),+
        }

        impl $name {
            /// Every variant, in declaration order.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// The name used in documents.
            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $text),+
                }
            }

            /// The variant with exactly this name; documents are case-sensitive.
            // Attribute vocabularies are only read through `ALL` and `name`; the
            // allow keeps one macro for every enum instead of two variants of it.
            #[allow(dead_code)]
            pub fn from_name(name: &str) -> Option<Self> {
                // A linear scan: vocabularies have at most a few dozen entries and
                // documents are small, so a map would only add setup code.
                Self::ALL.iter().copied().find(|variant| variant.name() == name)
            }
        }

        // Absolute path because the macro expands in other modules (`parse`),
        // where a relative `Named` would not resolve.
        impl $crate::model::vocabulary::Named for $name {
            fn name(self) -> &'static str {
                // `Self::name` picks the inherent method, not this one, so this
                // delegates instead of recursing.
                Self::name(self)
            }

            fn lookup(name: &str) -> Option<Self> {
                Self::from_name(name)
            }
        }
    };
}

// Re-exporting by path lets callers write `use ...::vocabulary::vocabulary;`
// instead of relying on `#[macro_use]` and module declaration order.
pub(crate) use vocabulary;
