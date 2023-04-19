use crate::symbol::SymbolText;
use crate::Symbol;
use std::borrow::Borrow;
use std::cmp::Ordering;
use std::hash::Hash;
use std::sync::Arc;

/// Holds a reference to the text of a given [SymbolRef].
// Originally, a SymbolRef's text was stored in an Option<&str>. However, a common use case for a
// SymbolRef is to call `to_owned()` on it, converting it to a `Symbol`.
// For example:
//     let symbol = reader.read_symbol_ref().to_owned();
// If the text is stored in an `Option<&str>`, then to convert it to a `Symbol` the application will
// either have to copy the `&str` to a `String` or re-resolve the text in the symbol table to get
// the corresponding `Arc<str>`.
// By storing an `Arc<str>` when the SymbolRef's text lives in the symbol table, we can convert a
// SymbolRef into a Symbol for free, moving the Arc<str> field from one struct to the other.
#[derive(Debug, Eq, Clone, Hash)]
enum SymbolRefText<'a> {
    // This symbol's text was found in the symbol table
    Shared(Arc<str>),
    // This symbol's text was found inline in the input stream
    Borrowed(&'a str),
    // This symbol is equivalent to SID zero (`$0`)
    Unknown,
}

impl<'a> SymbolRefText<'a> {
    fn text(&self) -> Option<&str> {
        let text = match self {
            SymbolRefText::Shared(s) => s.as_ref(),
            SymbolRefText::Borrowed(s) => s,
            SymbolRefText::Unknown => return None,
        };
        Some(text)
    }
}

impl<'a> PartialEq<Self> for SymbolRefText<'a> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl<'a> PartialOrd<Self> for SymbolRefText<'a> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<'a> Ord for SymbolRefText<'a> {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.text(), other.text()) {
            // If both Symbols have known text, delegate the comparison to their text.
            (Some(s1), Some(s2)) => s1.cmp(s2),
            // Otherwise, $0 (unknown text) is treated as 'less than' known text
            (Some(_), None) => Ordering::Greater,
            (None, Some(_)) => Ordering::Less,
            (None, None) => Ordering::Equal,
        }
    }
}

/// A reference to a fully resolved symbol. Like `Symbol` (a fully resolved symbol with a
/// static lifetime), a `SymbolRef` may have known or undefined text.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone)]
pub struct SymbolRef<'a> {
    text: SymbolRefText<'a>,
}

impl<'a> SymbolRef<'a> {
    /// If this symbol has known text, returns `Some(&str)`. Otherwise, returns `None`.
    pub fn text(&self) -> Option<&str> {
        self.text.text()
    }

    /// Constructs a `SymbolRef` with unknown text.
    pub fn with_unknown_text() -> Self {
        SymbolRef {
            text: SymbolRefText::Unknown,
        }
    }

    /// Constructs a `SymbolRef` with the specified text.
    pub fn with_text(text: &str) -> SymbolRef {
        SymbolRef {
            text: SymbolRefText::Borrowed(text),
        }
    }

    // Restricted visibility in case we want to change `Arc` later.
    pub(crate) fn with_shared_text(text: Arc<str>) -> SymbolRef<'static> {
        SymbolRef {
            text: SymbolRefText::Shared(text),
        }
    }

    pub fn to_owned(&self) -> Symbol {
        let SymbolRef { text } = self;
        match text {
            SymbolRefText::Shared(arc_str) => Symbol::shared(Arc::clone(arc_str)),
            SymbolRefText::Borrowed(text) => Symbol::owned(text.to_string()),
            SymbolRefText::Unknown => Symbol::unknown_text(),
        }
    }
}

/// Allows a `SymbolRef` to be constructed from a source value. This enables non-symbol types to be
/// viewed as a symbol with little to no runtime overhead.
pub trait AsSymbolRef {
    fn as_symbol_ref(&self) -> SymbolRef;
}

// All text types can be viewed as a `SymbolRef`.
impl<'a, A: AsRef<str> + 'a> AsSymbolRef for A {
    fn as_symbol_ref(&self) -> SymbolRef {
        SymbolRef {
            text: SymbolRefText::Borrowed(self.as_ref()),
        }
    }
}

impl<'a> From<&'a str> for SymbolRef<'a> {
    fn from(text: &'a str) -> Self {
        SymbolRef::with_text(text)
    }
}

impl<'a> From<Option<&'a str>> for SymbolRef<'a> {
    fn from(text: Option<&'a str>) -> Self {
        match text {
            Some(text) => SymbolRef::with_text(text),
            None => SymbolRef::with_unknown_text(),
        }
    }
}

// Note that this method panics if the SymbolRef has unknown text! This is unfortunate but is required
// in order to allow a HashMap<SymbolRef, _> to do lookups with a &str instead of a &SymbolRef
impl<'a> Borrow<str> for SymbolRef<'a> {
    fn borrow(&self) -> &str {
        self.text()
            .expect("cannot borrow a &str from a SymbolRef with unknown text")
    }
}

// Owned `Symbol` values can be viewed as a `SymbolRef`. Due to lifetime conflicts in the
// trait definitions, this cannot be achieved with `AsRef` or `Borrow`.
impl AsSymbolRef for Symbol {
    fn as_symbol_ref(&self) -> SymbolRef {
        let Symbol { text } = self;
        match text {
            SymbolText::Shared(arc_str) => SymbolRef::with_shared_text(Arc::clone(arc_str)),
            SymbolText::Owned(text) => SymbolRef::with_text(text.as_str()),
            SymbolText::Unknown => SymbolRef::with_unknown_text(),
        }
    }
}

impl AsSymbolRef for &Symbol {
    fn as_symbol_ref(&self) -> SymbolRef {
        (*self).as_symbol_ref()
    }
}

impl<'borrow, 'data> AsSymbolRef for &'borrow SymbolRef<'data> {
    fn as_symbol_ref(&self) -> SymbolRef<'data> {
        // This is cheap; we're cloning either a `&str` (runtime no-op) or an `Arc<str>` (which
        // requires an atomic integer increment.)

        (*self).clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbol_ref_with_text() {
        let symbol_ref = SymbolRef::with_text("foo");
        assert_eq!(Some("foo"), symbol_ref.text());
    }

    #[test]
    fn symbol_ref_with_unknown_text() {
        let symbol_ref = SymbolRef::with_unknown_text();
        assert_eq!(None, symbol_ref.text());
    }

    #[test]
    fn str_as_symbol_ref() {
        let symbol_ref: SymbolRef = "foo".as_symbol_ref();
        assert_eq!(Some("foo"), symbol_ref.text());
    }

    #[test]
    fn symbol_as_symbol_ref() {
        let symbol = Symbol::owned("foo");
        let symbol_ref: SymbolRef = symbol.as_symbol_ref();
        assert_eq!(Some("foo"), symbol_ref.text());
    }

    #[test]
    fn symbol_with_unknown_text_as_symbol_ref() {
        let symbol = Symbol::unknown_text();
        let symbol_ref: SymbolRef = symbol.as_symbol_ref();
        assert_eq!(None, symbol_ref.text());
    }
}
