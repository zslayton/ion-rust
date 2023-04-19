use crate::raw_symbol_token::RawSymbolToken;
use crate::result::decoding_error_raw;
use crate::symbol_ref::AsSymbolRef;
use crate::types::SymbolId;
use crate::{IonResult, Symbol, SymbolRef, SymbolTable};

/// Like RawSymbolToken, but the Text variant holds a borrowed reference instead of a String.
#[derive(Debug, PartialEq, Eq)]
pub enum RawSymbolTokenRef<'a> {
    SymbolId(SymbolId),
    Text(&'a str),
}

impl<'a> RawSymbolTokenRef<'a> {
    pub fn matches(&self, sid: SymbolId, text: &str) -> bool {
        match self {
            RawSymbolTokenRef::SymbolId(s) if *s == sid => true,
            RawSymbolTokenRef::Text(t) if t == &text => true,
            _ => false,
        }
    }

    pub fn resolve<'b>(&'b self, symbol_table: &'b SymbolTable) -> IonResult<SymbolRef<'b>> {
        match self {
            RawSymbolTokenRef::SymbolId(sid) => symbol_table
                .symbol_for(*sid)
                .map(|symbol| symbol.as_symbol_ref())
                .ok_or_else(|| decoding_error_raw("symbol ID not found in symbol table")),
            RawSymbolTokenRef::Text(text) => Ok(SymbolRef::with_text(text)),
        }
    }

    pub fn text(&self) -> Option<&str> {
        match self {
            RawSymbolTokenRef::SymbolId(_) => None,
            RawSymbolTokenRef::Text(t) => Some(t),
        }
    }

    pub fn to_owned(&self) -> RawSymbolToken {
        match self {
            RawSymbolTokenRef::SymbolId(sid) => RawSymbolToken::SymbolId(*sid),
            RawSymbolTokenRef::Text(text) => RawSymbolToken::Text(text.to_string()),
        }
    }
}

// Raw symbol tokens are not resolved, so we compare them structurally. This means that even in cases
// where the resolved tokens would be equal (`$7` == "symbols"), `eq` would return false.
impl<'a> PartialEq<RawSymbolToken> for RawSymbolTokenRef<'a> {
    fn eq(&self, other: &RawSymbolToken) -> bool {
        match (self, other) {
            (RawSymbolTokenRef::Text(t1), RawSymbolToken::Text(t2)) => t1 == t2,
            (RawSymbolTokenRef::SymbolId(sid1), RawSymbolToken::SymbolId(sid2)) => sid1 == sid2,
            _ => false,
        }
    }
}

/// Implemented by types that can be viewed as a [RawSymbolTokenRef] without allocations.
pub trait AsRawSymbolTokenRef {
    fn as_raw_symbol_token_ref(&self) -> RawSymbolTokenRef;
}

impl<'a> AsRawSymbolTokenRef for RawSymbolTokenRef<'a> {
    fn as_raw_symbol_token_ref(&self) -> RawSymbolTokenRef {
        match self {
            RawSymbolTokenRef::SymbolId(sid) => RawSymbolTokenRef::SymbolId(*sid),
            RawSymbolTokenRef::Text(text) => RawSymbolTokenRef::Text(text),
        }
    }
}

impl AsRawSymbolTokenRef for SymbolId {
    fn as_raw_symbol_token_ref(&self) -> RawSymbolTokenRef {
        RawSymbolTokenRef::SymbolId(*self)
    }
}

impl AsRawSymbolTokenRef for String {
    fn as_raw_symbol_token_ref(&self) -> RawSymbolTokenRef {
        RawSymbolTokenRef::Text(self.as_str())
    }
}

impl AsRawSymbolTokenRef for &str {
    fn as_raw_symbol_token_ref(&self) -> RawSymbolTokenRef {
        RawSymbolTokenRef::Text(self)
    }
}

impl AsRawSymbolTokenRef for Symbol {
    fn as_raw_symbol_token_ref(&self) -> RawSymbolTokenRef {
        match self.text() {
            Some(text) => RawSymbolTokenRef::Text(text),
            None => RawSymbolTokenRef::SymbolId(0),
        }
    }
}

impl<T> AsRawSymbolTokenRef for &T
where
    T: AsRawSymbolTokenRef,
{
    fn as_raw_symbol_token_ref(&self) -> RawSymbolTokenRef {
        (*self).as_raw_symbol_token_ref()
    }
}

impl AsRawSymbolTokenRef for RawSymbolToken {
    fn as_raw_symbol_token_ref(&self) -> RawSymbolTokenRef {
        match self {
            RawSymbolToken::SymbolId(sid) => RawSymbolTokenRef::SymbolId(*sid),
            RawSymbolToken::Text(text) => RawSymbolTokenRef::Text(text.as_str()),
        }
    }
}
