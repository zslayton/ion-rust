use crate::types::SymbolId;

/// A symbol token encountered in a text or binary Ion stream.
/// [RawSymbolToken]s do not store import source information for the token encountered. Similarly,
/// a [RawSymbolToken] cannot store both a symbol ID _and_ text, which means that it is not suitable
/// for representing a resolved symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawSymbolToken {
    SymbolId(SymbolId),
    Text(String),
}

impl RawSymbolToken {
    pub fn local_sid(&self) -> Option<SymbolId> {
        match self {
            RawSymbolToken::SymbolId(s) => Some(*s),
            RawSymbolToken::Text(_t) => None,
        }
    }

    pub fn text(&self) -> Option<&str> {
        match self {
            RawSymbolToken::SymbolId(_s) => None,
            RawSymbolToken::Text(t) => Some(t.as_str()),
        }
    }
}

impl From<SymbolId> for RawSymbolToken {
    fn from(symbol_id: SymbolId) -> Self {
        RawSymbolToken::SymbolId(symbol_id)
    }
}

impl From<String> for RawSymbolToken {
    fn from(text: String) -> Self {
        RawSymbolToken::Text(text)
    }
}

impl From<&str> for RawSymbolToken {
    fn from(text: &str) -> Self {
        RawSymbolToken::Text(text.to_string())
    }
}

impl<T> From<&T> for RawSymbolToken
where
    T: Clone + Into<RawSymbolToken>,
{
    fn from(value: &T) -> Self {
        value.clone().into()
    }
}
