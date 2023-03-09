use crate::value::owned::{Element, List, SExp, Struct};
use crate::Symbol;

pub struct ListBuilder {
    values: Vec<Element>,
}

impl ListBuilder {
    pub(crate) fn new() -> Self {
        ListBuilder { values: Vec::new() }
    }

    pub fn push<E: Into<Element>>(mut self, element: E) -> Self {
        self.values.push(element.into());
        self
    }

    pub fn remove(mut self, index: usize) -> Self {
        // This has O(n) behavior; the removals could be
        // buffered until the build() if needed.
        self.values.remove(index);
        self
    }

    pub fn build_list(self) -> List {
        List::new(self.values)
    }

    pub fn build(self) -> Element {
        self.build_list().into()
    }
}

pub struct SExpBuilder {
    values: Vec<Element>,
}

impl SExpBuilder {
    pub(crate) fn new() -> Self {
        SExpBuilder { values: Vec::new() }
    }

    pub fn push<E: Into<Element>>(mut self, element: E) -> Self {
        self.values.push(element.into());
        self
    }

    pub fn remove(mut self, index: usize) -> Self {
        // This has O(n) behavior; the removals could be
        // buffered until the build() if needed.
        self.values.remove(index);
        self
    }

    pub fn build_sexp(self) -> SExp {
        SExp::new(self.values)
    }

    pub fn build(self) -> Element {
        self.build_sexp().into()
    }
}

pub struct StructBuilder {
    values: Vec<(Symbol, Element)>,
}

impl StructBuilder {
    pub(crate) fn new() -> Self {
        StructBuilder { values: Vec::new() }
    }

    pub fn with_field<S: Into<Symbol>, E: Into<Element>>(
        mut self,
        field_name: S,
        field_value: E,
    ) -> Self {
        self.values.push((field_name.into(), field_value.into()));
        self
    }

    pub fn remove_field<A: AsRef<str>>(mut self, field_to_remove: A) -> Self {
        // TODO: This removes the first field with a matching name.
        //       Do we need other versions for remove_all or remove_last?
        // TODO: This has O(n) behavior; it could be optimized.
        let field_to_remove: &str = field_to_remove.as_ref();
        let _ = self
            .values
            .iter()
            .position(|&(ref name, _)| name == &field_to_remove)
            .map(|index| self.values.remove(index));
        self
    }

    pub fn build_struct(self) -> Struct {
        Struct::from_iter(self.values.into_iter())
    }

    pub fn build(self) -> Element {
        self.build_struct().into()
    }
}

// These `From` implementations allow a builder to be passed into
// any method that takes an `Into<Element>`, allowing you to avoid
// having to explicitly call `build()` on them.

impl From<ListBuilder> for Element {
    fn from(list_builder: ListBuilder) -> Self {
        list_builder.build().into()
    }
}

impl From<SExpBuilder> for Element {
    fn from(s_expr_builder: SExpBuilder) -> Self {
        s_expr_builder.build().into()
    }
}

impl From<StructBuilder> for Element {
    fn from(struct_builder: StructBuilder) -> Self {
        struct_builder.build().into()
    }
}

#[macro_export]
macro_rules! ion_list {
    ($($element:expr),*) => {
        crate::value::owned::List::builder()$(.push($element))*.build()
    };
}

#[macro_export]
macro_rules! ion_sexp {
    ($($element:expr)*) => {
        crate::value::owned::SExp::builder()$(.push($element))*.build()
    };
}

#[macro_export]
macro_rules! ion_struct {
    ($($field_name:ident:$element:expr),*) => {
        crate::value::owned::Struct::builder()$(.with_field(stringify!($field_name), $element))*.build()
    };
}

pub use ion_list;
pub use ion_sexp;
pub use ion_struct;

#[cfg(test)]
mod tests {
    use crate::value::builders::{ListBuilder, SExpBuilder, StructBuilder};
    use crate::value::owned::Element;
    use crate::value::reader::element_reader;
    use crate::value::reader::ElementReader;
    use crate::Symbol;

    #[test]
    fn build_list() {
        let actual: Element = ListBuilder::new()
            .push(1)
            .push(true)
            .push("foo")
            .push(Symbol::owned("bar"))
            .build();
        let expected = element_reader()
            .read_one(b"[1, true, \"foo\", bar]")
            .unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn build_list_with_macro() {
        let actual: Element = ion_list![1, true, "foo", Symbol::owned("bar")];
        let expected = element_reader()
            .read_one(b"[1, true, \"foo\", bar]")
            .unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn build_sexp() {
        let actual: Element = SExpBuilder::new()
            .push(1)
            .push(true)
            .push("foo")
            .push(Symbol::owned("bar"))
            .build();
        let expected = element_reader().read_one(b"(1 true \"foo\" bar)").unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn build_sexp_with_macro() {
        let actual: Element = ion_sexp!(1 true "foo" Symbol::owned("bar"));
        let expected = element_reader().read_one(b"(1 true \"foo\" bar)").unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn build_struct() {
        let actual: Element = StructBuilder::new()
            .with_field("a", 1)
            .with_field("b", true)
            .with_field("c", "foo")
            .with_field("d", Symbol::owned("bar"))
            .build();
        let expected = element_reader()
            .read_one(b"{a: 1, b: true, c: \"foo\", d: bar}")
            .unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn build_struct_with_macro() {
        let actual: Element = ion_struct! {
            a: 1,
            b: true,
            c: "foo",
            d: Symbol::owned("bar")
        };
        let expected = element_reader()
            .read_one(b"{a: 1, b: true, c: \"foo\", d: bar}")
            .unwrap();
        assert_eq!(actual, expected);
    }
}
