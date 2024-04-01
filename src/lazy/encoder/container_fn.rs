use crate::IonResult;
use crate::lazy::encoder::value_writer::ValueWriter;

///! Container population traits that allow closures to be used in places where the borrow checker
/// would normally balk due to point-in-time limitations. TODO link
///

macro_rules! container_fn_trait {
    // End of iteration
    () => {};
    // Recurses one argument pair at a time
    ($trait_name:ident => $assoc_type_name:ident, $($rest:tt)*) => {
        pub trait $trait_name<V: ValueWriter>: FnOnce(&mut V::$assoc_type_name) -> IonResult<()> {
            fn populate(self, writer: &mut V::$assoc_type_name) -> IonResult<()>;
        }

        impl<F, V: ValueWriter> $trait_name<V> for F
            where
                F: FnOnce(&mut V::$assoc_type_name) -> IonResult<()>,
        {
            fn populate(self, writer: &mut V::$assoc_type_name) -> IonResult<()> {
                self(writer)
            }
        }

        container_fn_trait!($($rest)*);
    };
}

container_fn_trait!(
    ListFn => ListWriter,
    SExpFn => SExpWriter,
    StructFn => StructWriter,
    MacroArgsFn => MacroArgsWriter,
);