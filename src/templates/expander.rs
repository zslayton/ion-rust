use std::collections::HashMap;
use crate::{IonType, Symbol};
use crate::templates::template::Template;
use crate::types::integer::IntAccess;
use crate::value::{IonElement, IonSequence};
use crate::value::owned::{Element, Sequence, Struct, Value};

struct Expander {
    templates: HashMap<String, Template>
}

impl Default for Expander {
    fn default() -> Self {
        Expander::with_templates([])
    }
}

impl Expander {
    pub fn with_templates<I: IntoIterator<Item=Template>>(templates: I) -> Self {
        let templates_map: HashMap<String, Template> = templates
            .into_iter()
            .map(|t| (t.name().to_owned(), t))
            .collect();
        Self {
            templates: templates_map
        }
    }

    pub fn expand(&self, input: &Element) -> Vec<Element> {
        match input.ion_type() {
            IonType::SExpression if input.has_annotation("ion_invoke") => self.expand_invocation(input),
            IonType::List => {
                let expanded_elements = self.expand_sequence_elements(input);
                vec![Value::List(Sequence::new(expanded_elements)).into()]
            }
            IonType::SExpression => {
                let expanded_elements = self.expand_sequence_elements(input);
                vec![Value::SExpression(Sequence::new(expanded_elements)).into()]
            },
            IonType::Struct => vec![self.expand_struct_fields(input)],
            _ => vec![input.clone()]
        }
    }

    fn expand_sequence_elements(&self, input: &Element) -> Vec<Element> {
        let mut expanded_child_elements = vec![];
        for child in input.as_sequence().unwrap().iter() {
            expanded_child_elements.extend(self.expand(child).into_iter());
        }
        expanded_child_elements
    }

    // Always returns a new struct
    fn expand_struct_fields(&self, input: &Element) -> Element {
        let mut expanded_fields = vec![];
        for (name, element) in input.as_struct().unwrap().fields() {
            for expanded_element in self.expand(element) {
                expanded_fields.push((name.clone(), expanded_element));
            }
        }
        let s = Struct::from_iter(expanded_fields);
        Value::Struct(s).into()
    }

    fn expand_invocation(&self, input: &Element) -> Vec<Element> {
        let invocation: Vec<&Element> = input.as_sequence().unwrap().iter().collect();
        if invocation.is_empty() {
            panic!("empty template invocation");
        }
        // TODO: Other forms of template identifier (int, fully qualified)
        let name = invocation.get(0).unwrap().as_str().expect("template name was not text");
        let values = &invocation[1..];
        match name {
            "empty" => vec![],
            "quote" => self.quote(values),
            // TODO: `unquote`?
            "stream" => self.stream(values),
            "repeat" => self.repeat(values),
            "inline" => self.inline(values),
            "if_empty" => self.if_empty(values),
            "annotated" => self.annotated(values),
            "string" => self.string(values),
            "symbol" => self.symbol(values),
            "list" => self.list(values),
            "sexp" => self.sexp(values),
            "struct" => self.make_struct(values), // `struct` is a keyword
            unsupported => panic!("Unsupported template name: '{}'", unsupported)
        }
    }

    // Return an empty stream
    fn empty(&self, _values: &[&Element]) -> Vec<Element> {
        Vec::new()
    }

    // Return a copy of the elements without performing expansion
    fn quote(&self, values: &[&Element]) -> Vec<Element> {
        values.iter()
            .map(|e| (*e).to_owned())
            .collect()
    }

    // Return a copy of the elements after performing expansion
    fn stream(&self, values: &[&Element]) -> Vec<Element> {
        values.iter()
            .flat_map(|e| self.expand(e))
            .collect()
    }

    fn inline(&self, values: &[&Element]) -> Vec<Element> {
        values
            .iter()
            .flat_map(|e| {
                let sequence = e
                    .as_sequence()
                    .expect("`inline` only accepts sequence types (list, sexp)");
                sequence.iter()
            })
            .flat_map(|e| self.expand(e))
            .collect()
    }

    fn repeat(&self, values: &[&Element]) -> Vec<Element> {
        if values.is_empty() {
            panic!("`repeat` requires a count: (:repeat [count] ...)")
        }
        let count = values[0]
            .as_integer()
            .expect("first argument to `repeat` must be an int")
            .as_i64()
            .expect("specified `repeat` count was too large");
        if count < 0 {
            panic!("`repeat` count cannot be negative");
        }
        let mut expanded = Vec::new();
        let values_to_repeat: Vec<Element> = values[1..]
            .iter()
            .flat_map(|e| self.expand(e))
            .collect();
        for _ in 0..count {
            expanded.extend(values_to_repeat.iter().cloned());
        }
        expanded
    }

    fn if_empty(&self, values: &[&Element]) -> Vec<Element> {
        if values.is_empty() {
            panic!("`if_empty` requires at least one argument to test");
        }
        let expr_to_test = self.expand(values[0]);
        if expr_to_test.is_empty() {
            values[1..].iter().flat_map(|e| self.expand(e)).collect()
        } else {
            expr_to_test
        }
    }

    // Params: (many::annotations required::value)
    fn annotated(&self, values: &[&Element]) -> Vec<Element> {
        if values.len() != 2 {
            panic!("(:annotated) takes two expressions, found {}", values.len());
        }
        let annotations: Vec<Symbol> = self.expand(values[0])
            .iter()
            .map(|e| Symbol::owned(e.as_str().expect("found non-text annotation")))
            .collect();
        let mut values = self.expand(values[1]);
        if values.len() > 1 {
            panic!("`annotated` takes a single value.");
        }
        let value = values.pop().unwrap();
        // TODO: Expose a better API for cloning an element while changing its annotations
        vec![Element::new(annotations, value.value)]
    }

    fn string(&self, values: &[&Element]) -> Vec<Element> {
        let new_string = self.join_text_elements(values);
        vec![Element::new(Vec::new(), Value::String(new_string))]
    }

    fn symbol(&self, values: &[&Element]) -> Vec<Element> {
        let new_string = self.join_text_elements(values);
        vec![Element::new(Vec::new(), Value::Symbol(Symbol::owned(new_string)))]
    }

    fn join_text_elements(&self, values: &[&Element]) -> String {
        let mut new_string = String::new();
        values
            .iter()
            .flat_map(|e| self.expand(e))
            .fold(&mut new_string, |new_string, element| {
                let text = element.as_str()
                    .expect("`string` only accepts text types (string, symbol)");
                new_string.push_str(text);
                new_string
            });
        new_string
    }

    fn list(&self, values: &[&Element]) -> Vec<Element> {
        vec![Value::List(self.join_into_sequence(values)).into()]
    }

    fn sexp(&self, values: &[&Element]) -> Vec<Element> {
        vec![Value::SExpression(self.join_into_sequence(values)).into()]
    }

    // `struct` is a keyword and cannot be a function name
    fn make_struct(&self, values: &[&Element]) -> Vec<Element> {
        let elements: Vec<Element> = values.iter()
            .flat_map(|e| self.expand(e))
            .collect();

        let mut new_fields: Vec<(Symbol, Element)> = Vec::new();
        let mut index: usize = 0;
        while index < elements.len() {
            let field_name_element = elements.get(index).unwrap();
            if field_name_element.ion_type() == IonType::Struct {
                // We found a struct in field name position; merge its fields in
                for (name, value) in field_name_element.as_struct().unwrap().fields() {
                    new_fields.push((Symbol::owned(name.text().unwrap()), value.to_owned()));
                }
            } else {
                // It's not a struct, so it must be a text value
                index += 1;
                let value = elements.get(index).expect("Found `struct` field name with no corresponding value.");
                new_fields.push((Symbol::owned(field_name_element.as_str().unwrap()), value.to_owned()))
            }
            index += 1;
        }
        vec![Value::Struct(Struct::from_iter(new_fields)).into()]
    }

    fn join_into_sequence(&self, values: &[&Element]) -> Sequence {
        let elements: Vec<Element> = values.iter()
            .flat_map(|e| self.expand(e))
            .collect();
        Sequence::new(elements)
    }
}

#[cfg(test)]
mod tests {
    use std::default::Default;
    use crate::templates::expander::Expander;
    use crate::value::native_reader::NativeElementReader;
    use crate::value::owned::Element;
    use crate::value::reader::ElementReader;

    fn operator_test(input_ion: &str, expected_ion: &str) {
        expansion_test(Default::default(), input_ion, expected_ion)
    }

    fn expansion_test(expander: Expander, input_ion: &str, expected_ion: &str) {
        let reader = NativeElementReader;
        let input_elements = reader.read_all(input_ion.as_bytes()).expect("Invalid input Ion");
        let expected_elements = reader.read_all(expected_ion.as_bytes()).expect("Invalid expected Ion");
        let mut output_elements = vec![];
        for input_element in &input_elements {
            let expanded = expander.expand(&input_element);
            output_elements.extend(expanded);
        }
        if output_elements != expected_elements {
            println!("Expanded output did not match expected output.");
            println!("=== Actual ===");
            show_ion(&output_elements);
            println!("=== Expected ===");
            show_ion(&expected_elements);
        }
        assert_eq!(output_elements, expected_elements);
    }

    fn show_ion(values: &Vec<Element>) {
        for (index, value) in values.iter().enumerate() {
            println!("{}. {}", index, value);
        }
    }

    #[test]
    fn plain_old_values() {
        operator_test("1 2 3", "1 2 3");
    }

    #[test]
    fn stream() {
        operator_test("1 (:stream 2 3) 4", "1 2 3 4");
        operator_test("{foo: (:stream 1 2 3) }", "{foo: 1, foo: 2, foo: 3}")
    }

    #[test]
    fn repeat() {
        operator_test("foo (:repeat 3 bar) baz", "foo bar bar bar baz");
        operator_test("foo (:repeat 0 bar) baz", "foo baz");
        operator_test("foo (:repeat 3 (:repeat 2 x)) baz", "foo x x x x x x baz");
    }

    #[test]
    fn inline() {
        operator_test("1 (:inline [2, 3] (4 5)) 6", "1 2 3 4 5 6");
        operator_test("{foo: (:inline (1 2 3)) }", "{foo: 1, foo: 2, foo: 3}")
    }

    #[test]
    fn if_empty() {
        operator_test("foo (:if_empty (:empty) baz) quux", "foo baz quux");
        operator_test("foo (:if_empty bar baz) quux", "foo bar quux");
        operator_test("foo (:if_empty (:inline [1, 2]) baz) quux", "foo 1 2 quux");
        operator_test("foo (:if_empty (:inline []) baz) quux", "foo baz quux");
    }

    #[test]
    fn annotated() {
        operator_test("(:annotated (:stream foo bar baz) 7)", "foo::bar::baz::7");
        operator_test("(:annotated (:inline (foo bar baz)) 7)", "foo::bar::baz::7");
    }

    #[test]
    fn string() {
        operator_test("(:string foo bar baz)", "\"foobarbaz\"");
        operator_test("(:string 'foo' \"bar\" \"\"\"baz\"\"\")", "\"foobarbaz\"");
        operator_test("(:string (:inline ('foo' \"bar\" \"\"\"baz\"\"\")))", "\"foobarbaz\"");
    }

    #[test]
    fn symbol() {
        operator_test("(:symbol foo bar baz)", "foobarbaz");
        operator_test("(:symbol 'foo' \"bar\" \"\"\"baz\"\"\")", "foobarbaz");
        operator_test("(:symbol (:inline ('foo' \"bar\" \"\"\"baz\"\"\")))", "foobarbaz");
    }

    #[test]
    fn list() {
        operator_test("(:list foo bar baz)", "[foo, bar, baz]");
        operator_test("(:list [])", "[[]]");
        operator_test("(:list (:inline (1 2 3)))", "[1, 2, 3]");
    }

    #[test]
    fn test_struct() {
        operator_test("(:struct foo bar baz quux)", "{foo: bar, baz: quux}");
        operator_test("(:struct foo bar {baz: quux, quuz: gary})", "{foo: bar, baz: quux, quuz: gary}");
    }
}

/*
NOTES
- Treating invocations as $ion_invoke::() means we can represent them using a lot of existing
  structures. Not great, but the devil we know.
- Using {# } syntax means we have to add parsing machinery in more places. As an alternative,
  we could use (: ) syntax, which is pretty nice.
- What's the text repr for an invocation-that-makes-an-annotation? (:foo)::bar::baz? Is that a problem?
  Should we just replace this with a system template for (:annotated)? What about field names?
 */