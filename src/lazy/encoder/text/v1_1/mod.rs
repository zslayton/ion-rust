mod value_writer;
mod writer;

use crate::lazy::encoder::text::v1_1::writer::LazyRawTextWriter_1_1;
use crate::lazy::encoder::{LazyEncoder, SymbolCreationPolicy};
use crate::lazy::encoding::TextEncoding_1_1;
use std::io::Write;

impl LazyEncoder for TextEncoding_1_1 {
    const SUPPORTS_TEXT_TOKENS: bool = false;
    const DEFAULT_SYMBOL_CREATION_POLICY: SymbolCreationPolicy =
        SymbolCreationPolicy::RequireSymbolId;
    type Writer<W: Write> = LazyRawTextWriter_1_1<W>;
}