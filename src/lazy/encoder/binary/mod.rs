pub mod v1_0;
pub mod v1_1;

impl<F, V> MakeValueWriter for F
where
    F: FnMut() -> V,
    V: AnnotatableValueWriter,
{
    type ValueWriter<'a> = V where Self: 'a;

    fn make_value_writer(&mut self) -> Self::ValueWriter<'_> {
        self()
    }
}

/// Takes a series of `TYPE => METHOD` pairs, generating a function for each that calls the host
/// type's `encode_annotated` method to encode an annotations sequence and then delegates encoding
/// the value to the corresponding value writer method.
// This macro is used in the v1_0 and v1_1 binary writer implementations, which both define an
// `encode_annotated` method. That method is not codified (for example: in a trait); this relies
// solely on convention between the two.
macro_rules! annotate_and_delegate_1_0 {
    // End of iteration
    () => {};
    // Recurses one argument pair at a time
    ($value_type:ty => $method:ident, $($rest:tt)*) => {
        fn $method(self, value: $value_type) -> IonResult<()> {
            let allocator = self.allocator;
            let mut buffer = allocator.alloc_with(|| BumpVec::new_in(allocator));
            // let mut buffer = BumpVec::new_in(allocator);
            let value_writer =
                $crate::lazy::encoder::binary::v1_0::value_writer::BinaryValueWriter_1_0::new(
                    self.allocator,
                    &mut buffer,
                );
            value_writer.$method(value)?;
            self.annotate_encoded_value(buffer.as_slice())
        }
        annotate_and_delegate_1_0!($($rest)*);
    };
}
use crate::lazy::encoder::value_writer::internal::MakeValueWriter;
use crate::lazy::encoder::value_writer::AnnotatableValueWriter;
pub(crate) use annotate_and_delegate_1_0;

macro_rules! annotate_and_delegate_1_1 {
    // End of iteration
    () => {};
    // Recurses one argument pair at a time
    ($value_type:ty => $method:ident, $($rest:tt)*) => {
        fn $method(mut self, value: $value_type) -> IonResult<()> {
            match self.annotations {
                [] => {
                    // There are no annotations; nothing to do.
                }
                [a] => {
                    // Opcode 0xE7: A single FlexSym annotation follows
                    self.buffer.push(0xE7);
                    FlexSym::encode_symbol(self.buffer, a);
                }
                [a1, a2] => {
                    // Opcode 0xE8: Two FlexSym annotations follow
                    self.buffer.push(0xE8);
                    FlexSym::encode_symbol(self.buffer, a1);
                    FlexSym::encode_symbol(self.buffer, a2);
                }
                _ => {
                    self.write_length_prefixed_flex_sym_annotation_sequence();
                }
            }
            // We've encoded the annotations, now create a no-annotations ValueWriter to encode the value itself.
            let value_writer = $crate::lazy::encoder::binary::v1_1::value_writer::BinaryValueWriter_1_1::new(self.allocator, self.buffer);
            value_writer.$method(value)
            // encode_value_fn(value_writer)
            // self.encode_annotated(|value_writer| value_writer.$method(value))
            // <Self as AnnotateAndDelegate>::encode_annotated(self, |value_writer| value_writer.$method(value))
        }
        annotate_and_delegate_1_1!($($rest)*);
    };
}

pub(crate) use annotate_and_delegate_1_1;
