use crate::templates::template::{Cardinality, Encoding, Parameter, Template};
use crate::types::integer::IntAccess;
use crate::value::native_reader::NativeElementReader;
use crate::value::owned::{Element, Sequence, Struct, Value};
use crate::value::reader::ElementReader;
use crate::value::{IonElement, IonSequence};
use crate::{IonType, Symbol};
use std::collections::HashMap;

type Environment = HashMap<String, Vec<Element>>;

const SYSTEM_TEMPLATES: &str = r#"
            {
                name: param,
                parameters: [
                    {name: cardinality, encoding: any, cardinality: required},
                    {name: encoding, encoding: any, cardinality: required},
                    {name: name, encoding: any, cardinality: required},
                ],
                body: {
                    name: name,
                    cardinality: cardinality,
                    encoding: encoding
                }
            }
            {
                name: params,
                parameters: [
                    (:param many template::param parameter_stream)
                ],
                body: (sexp parameter_stream) // Wraps parameter structs in an s-expression
            }
            {
                name: define,
                parameters: [
                    (:param required any name),
                    (:param required template::params parameters),
                    (:param required any body),
                ],
                body: {
                   name: name,
                   parameters: parameters,
                   body: body,
                }
            }
"#;

pub(crate) fn read_system_templates() -> Vec<Element> {
    NativeElementReader
        .read_all(SYSTEM_TEMPLATES.as_bytes())
        .expect("invalid template source")
}

struct Expander {
    // Our list of template definitions
    templates: HashMap<String, Template>,
}

impl Default for Expander {
    fn default() -> Self {
        Expander::from_template_source("") // Only the system templates
    }
}

impl Expander {
    pub fn from_template_source(ion_data: &str) -> Self {
        let mut expander = Self {
            templates: HashMap::new(),
        };

        let system_templates = read_system_templates();
        let local_templates = NativeElementReader
            .read_all(ion_data.as_bytes())
            .expect("invalid template source");

        let all_templates = system_templates.into_iter().chain(local_templates);

        for template_element in all_templates {
            let mut expanded_definition = expander.expand(&template_element);
            if expanded_definition.len() != 1 {
                // TODO: Is this true? You can use `(:stream ...)`, but should we allow unwrapped?
                panic!(
                    "template bodies must be exactly 1 expression, found {}",
                    expanded_definition.len()
                )
            }
            let expanded_definition = expanded_definition.pop().unwrap();
            println!("Expanded definition: {}", expanded_definition);
            let template = Template::from_ion(&expanded_definition).unwrap();
            println!(
                "Template '{}' loaded ok, adding to expander",
                template.name()
            );
            expander
                .templates
                .insert(template.name().to_owned(), template);
        }
        expander
    }

    pub fn with_templates<I: IntoIterator<Item = Template>>(templates: I) -> Self {
        let templates_map: HashMap<String, Template> = templates
            .into_iter()
            .map(|t| (t.name().to_owned(), t))
            .collect();
        Self {
            templates: templates_map,
        }
    }

    pub fn expand(&self, input: &Element) -> Vec<Element> {
        match input.ion_type() {
            IonType::SExpression if input.has_annotation("$ion_invoke") => {
                self.expand_invocation(input)
            }
            IonType::List => {
                let expanded_elements = self.expand_sequence_elements(input);
                vec![Value::List(Sequence::new(expanded_elements)).into()]
            }
            IonType::SExpression => {
                let expanded_elements = self.expand_sequence_elements(input);
                vec![Value::SExpression(Sequence::new(expanded_elements)).into()]
            }
            IonType::Struct => vec![self.expand_struct_fields(input)],
            _ => vec![input.clone()],
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

    // Expansions come in two flavors: they can appear in the data stream
    //    (:name arg1 arg2)
    // and they can appear in template definitions:
    //    (name arg1 arg2) // no leading ':' in the s-expression
    // When they appear in the data stream, the arguments must be expanded during evaluation.
    // Whey they appear in a template def, the arguments have already been expanded and doing so
    // again would only be wasted work.
    fn expand_invocation(&self, input: &Element) -> Vec<Element> {
        let invocation: Vec<&Element> = input.as_sequence().unwrap().iter().collect();
        if invocation.is_empty() {
            panic!("empty template invocation");
        }
        // TODO: Other forms of template identifier (int, fully qualified)
        let name = invocation
            .get(0)
            .unwrap()
            .as_str()
            .expect("template name was not text");
        let arguments = &invocation[1..];
        match name {
            "empty" => vec![],
            "stream" => self.stream(arguments),
            "repeat" => self.repeat(arguments),
            "inline" => self.inline(arguments),
            "default" => self.default(arguments),
            "annotated" => self.annotated(arguments),
            "string" => self.string(arguments),
            "symbol" => self.symbol(arguments),
            "list" => self.list(arguments),
            "sexp" => self.sexp(arguments),
            "struct" => self.make_struct(arguments), // `struct` is a keyword
            "quote" => self.quote(arguments),
            template_name => self.expand_template(template_name, arguments),
        }
    }

    // Return an empty stream
    fn empty(&self, _values: &[&Element]) -> Vec<Element> {
        Vec::new()
    }

    // TODO: This only has meaning inside a template definition body and shouldn't be available
    //       anywhere else.
    // Return a copy of the elements without performing expansion
    fn quote(&self, values: &[&Element]) -> Vec<Element> {
        values.iter().map(|e| (*e).to_owned()).collect()
    }

    // Return a copy of the elements after performing expansion
    fn stream(&self, values: &[&Element]) -> Vec<Element> {
        values.iter().flat_map(|e| self.expand(e)).collect()
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
        let values_to_repeat: Vec<Element> =
            values[1..].iter().flat_map(|e| self.expand(e)).collect();
        for _ in 0..count {
            expanded.extend(values_to_repeat.iter().cloned());
        }
        expanded
    }

    fn default(&self, values: &[&Element]) -> Vec<Element> {
        if values.is_empty() {
            panic!("`default` requires at least one argument to test");
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
        let annotations: Vec<Symbol> = self
            .expand(values[0])
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
        vec![Element::new(
            Vec::new(),
            Value::Symbol(Symbol::owned(new_string)),
        )]
    }

    fn join_text_elements(&self, values: &[&Element]) -> String {
        let mut new_string = String::new();
        values
            .iter()
            .flat_map(|e| self.expand(e))
            .fold(&mut new_string, |new_string, element| {
                let text = element
                    .as_str()
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
        let elements: Vec<Element> = values.iter().flat_map(|e| self.expand(e)).collect();

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
                let value = elements
                    .get(index)
                    .expect("Found `struct` field name with no corresponding value.");
                new_fields.push((
                    Symbol::owned(field_name_element.as_str().unwrap()),
                    value.to_owned(),
                ))
            }
            index += 1;
        }
        vec![Value::Struct(Struct::from_iter(new_fields)).into()]
    }

    fn join_into_sequence(&self, values: &[&Element]) -> Sequence {
        let elements: Vec<Element> = values.iter().flat_map(|e| self.expand(e)).collect();
        Sequence::new(elements)
    }

    fn expand_template(&self, template_name: &str, values: &[&Element]) -> Vec<Element> {
        let template = self
            .templates
            .get(template_name)
            .unwrap_or_else(|| panic!("reference to unknown template: {}", template_name));
        let environment = self.bind_arguments(template, values);
        let body = template.body();
        self.expand_tdl_element(&environment, body)
    }

    // The template body has already been expanded, so any `(:name ...)`s have been replaced.
    // This method expands Template Definition Language (TDL) expressions. It looks for symbols
    // in value position and, if it finds them in the Environment,
    // TODO: quote, unquote
    fn expand_tdl_element(&self, environment: &Environment, element: &Element) -> Vec<Element> {
        if element.is_null() {
            return vec![element.to_owned()];
        }
        match element.ion_type() {
            IonType::SExpression => self.expand_tdl_operation(environment, element),
            IonType::List => self.expand_tdl_list(environment, element),
            IonType::Struct => self.expand_tdl_struct(environment, element),
            IonType::Symbol => self.expand_tdl_symbol(environment, element),
            _ => vec![element.to_owned()],
        }
    }
    fn expand_tdl_symbol(&self, environment: &Environment, element: &Element) -> Vec<Element> {
        let variable_name = element.as_str().unwrap();
        if let Some(stream) = environment.get(variable_name) {
            stream.clone()
        } else {
            vec![element.to_owned()]
        }
    }

    fn expand_tdl_operation(&self, environment: &Environment, element: &Element) -> Vec<Element> {
        let child_elements: Vec<&Element> = element.as_sequence().unwrap().iter().collect();
        if child_elements.is_empty() {
            panic!("empty operation");
        }
        // TODO: Other forms of template identifier (int, fully qualified)
        let name = child_elements[0]
            .as_str()
            .expect("template name was not text");
        let arguments = &child_elements[1..];
        match name {
            "empty" => vec![],
            "quote" => self.tdl_op_quote(environment, arguments),
            // TODO: `unquote`?
            "stream" => self.tdl_op_stream(environment, arguments),
            "repeat" => self.tdl_op_repeat(environment, arguments),
            "inline" => self.tdl_op_inline(environment, arguments),
            "default" => self.tdl_op_default(environment, arguments),
            "annotated" => self.tdl_op_annotated(environment, arguments),
            "string" => self.tdl_op_string(environment, arguments),
            "symbol" => self.tdl_op_symbol(environment, arguments),
            "list" => self.tdl_op_list(environment, arguments),
            "sexp" => self.tdl_op_sexp(environment, arguments),
            "struct" => self.tdl_make_struct(environment, arguments), // `struct` is a keyword,
            "each" => self.tdl_op_each(environment, arguments),
            template_name => self.expand_template(template_name, arguments),
        }
    }

    fn expand_tdl_list(&self, environment: &Environment, element: &Element) -> Vec<Element> {
        let child_elements: Vec<Element> = element
            .as_sequence()
            .unwrap()
            .iter()
            .flat_map(|e| self.expand_tdl_element(environment, e))
            .collect();
        // Make a Vec containing a new list with the expanded child elements
        vec![Value::List(Sequence::new(child_elements)).into()]
    }

    fn expand_tdl_struct(&self, environment: &Environment, element: &Element) -> Vec<Element> {
        let fields = element
            .as_struct()
            .unwrap()
            .fields()
            .map(|(name, value)| (name, self.expand_tdl_element(environment, value)))
            .flat_map(|(name, values)| values.into_iter().map(|v| (name.to_owned(), v)));
        let new_struct = Struct::from_iter(fields);
        vec![Value::Struct(new_struct).into()]
    }

    // Looks at the template's parameter list and validates/binds elements to the parameter names
    fn bind_arguments(&self, template: &Template, values: &[&Element]) -> Environment {
        let parameters = template.parameters();
        let arguments = values.iter();
        let mut scope = HashMap::new();

        for (param, argument_expr) in parameters.iter().zip(arguments) {
            self.bind_argument(&mut scope, template, param, argument_expr);
        }

        if values.len() > parameters.len() {
            // There are trailing arguments that have not yet been bound to a parameter name
            let last_parameter = parameters.last().unwrap_or_else(|| {
                panic!(
                    "found arguments in invocation of template '{}', which takes no parameters",
                    template.name()
                )
            });

            if last_parameter.cardinality() == Cardinality::Many {
                // We can assume all remaining arguments belong to the trailing `many` parameter
                for argument_expr in &values[parameters.len()..] {
                    self.bind_argument(&mut scope, template, last_parameter, argument_expr);
                }
            } else {
                panic!(
                    "found unbound arguments in invocation of template '{}'",
                    template.name()
                );
            }
        } else if values.len() < parameters.len() {
            // There are parameters that didn't get an argument
            for parameter in &parameters[values.len()..] {
                if parameter.cardinality() == Cardinality::Required {
                    panic!(
                        "No argument was passed for template '{}', parameter '{}', which has cardinality '{:?}'",
                        template.name(),
                        parameter.name(),
                        parameter.cardinality(),
                    );
                }
                scope.insert(parameter.name().to_owned(), Vec::new());
            }
        }

        scope
    }

    fn bind_argument(
        &self,
        scope: &mut HashMap<String, Vec<Element>>,
        template: &Template,
        param: &Parameter,
        argument_expr: &Element,
    ) {
        // If the parameter's encoding is a template, we look for an s-expression representing
        // its arguments. The template name is implied.
        let expanded_argument = if let Encoding::Template(template_name) = param.encoding() {
            if argument_expr.ion_type() != IonType::SExpression {
                panic!(
                    "Template '{}' param '{}' has encoding template::{} but argument was: {}",
                    template.name(),
                    param.name(),
                    template_name,
                    argument_expr
                )
            }
            if argument_expr.has_annotation("$ion_invoke") {
                panic!(
                    "Template invocations can only be passed as an argument if the parameter encoding is `any`"
                )
            }
            let template_arguments = Self::sequence_elements(argument_expr);
            self.expand_template(template_name.as_ref(), template_arguments.as_slice())
        } else {
            self.expand(argument_expr)
        };

        match param.cardinality() {
            Cardinality::Required => {
                if expanded_argument.len() != 1 {
                    panic!(
                        "parameter '{}' is required (exactly 1), but found {}",
                        param.name(),
                        expanded_argument.len()
                    )
                }
            }
            Cardinality::Optional => {
                if expanded_argument.len() > 1 {
                    panic!(
                        "parameter '{}' is optional (0-1), but found {}",
                        param.name(),
                        expanded_argument.len()
                    )
                }
            }
            Cardinality::Many => {}
        }

        if let Some(values) = scope.get_mut(param.name()) {
            values.extend(expanded_argument);
        } else {
            scope.insert(param.name().to_owned(), expanded_argument);
        }
    }

    fn sequence_elements(sequence_element: &Element) -> Vec<&Element> {
        sequence_element
            .as_sequence()
            .expect("tried to get sequence children for a non-sequence element")
            .iter()
            .collect()
    }

    fn sequence_elements_cloned(sequence_element: &Element) -> Vec<Element> {
        sequence_element
            .as_sequence()
            .expect("tried to get sequence children for a non-sequence element")
            .iter()
            .map(|child| child.to_owned())
            .collect()
    }

    fn tdl_op_empty(&self, _environment: &Environment, _element: &Element) -> Vec<Element> {
        Vec::new()
    }

    fn tdl_op_quote(&self, _environment: &Environment, arguments: &[&Element]) -> Vec<Element> {
        arguments.iter().map(|e| (*e).to_owned()).collect()
    }

    fn tdl_op_stream(&self, environment: &Environment, arguments: &[&Element]) -> Vec<Element> {
        arguments
            .iter()
            .flat_map(|e| self.expand_tdl_element(environment, e))
            .collect()
    }

    fn tdl_op_inline(&self, environment: &Environment, arguments: &[&Element]) -> Vec<Element> {
        let expanded_arguments: Vec<Element> = arguments
            .iter()
            .flat_map(|e| self.expand_tdl_element(environment, e))
            .collect();

        expanded_arguments
            .iter()
            .flat_map(|e| {
                e.as_sequence()
                    .expect("`inline` arguments must be sequences")
                    .iter()
            })
            .map(|e| e.to_owned())
            .collect()
    }

    fn tdl_op_repeat(&self, environment: &Environment, arguments: &[&Element]) -> Vec<Element> {
        let count = arguments
            .get(0)
            .and_then(|c| c.as_i64())
            .unwrap_or_else(|| panic!("`repeat` called without integer count"));
        let elements: Vec<Element> = arguments
            .get(1)
            .map(|e| self.expand_tdl_element(environment, e))
            .expect("`repeat` called without trailing arguments");

        let mut output = Vec::new();
        for _ in 0..count {
            output.extend(elements.clone());
        }
        output
    }

    fn tdl_op_default(&self, environment: &Environment, arguments: &[&Element]) -> Vec<Element> {
        let variable_stream = arguments
            .get(0)
            .map(|e| self.expand_tdl_element(environment, e))
            .expect("`default` usage: (:default variable_name default_value)");

        if !variable_stream.is_empty() {
            // Use the non-empty variable value
            variable_stream
        } else {
            // Use the default value
            arguments
                .get(1)
                .map(|e| self.expand_tdl_element(environment, e))
                .expect("`default` usage: (:default variable_name default_value)")
        }
    }

    // (:each name_of_current stream expression)
    //
    fn tdl_op_each(&self, environment: &Environment, arguments: &[&Element]) -> Vec<Element> {
        let name_of_current = arguments
            .get(0)
            .filter(|name| name.ion_type() == IonType::Symbol)
            .and_then(|name| name.as_str())
            .expect("`each`'s first argument must be a symbol");
        let mut scope = environment.clone();
        let stream = arguments
            .get(1)
            .map(|e| self.expand_tdl_element(environment, e))
            .expect("`each`'s second argument must be a stream");
        let body = arguments
            .get(2)
            .expect("`each`'s third argument must be a TDL expression");
        stream
            .into_iter()
            .flat_map(|stream_element| {
                scope.insert(name_of_current.to_owned(), vec![stream_element]);
                self.expand_tdl_element(&scope, body)
            })
            .collect()
    }

    // Params: (many::annotations required::value)
    fn tdl_op_annotated(&self, environment: &Environment, arguments: &[&Element]) -> Vec<Element> {
        if arguments.len() != 2 {
            panic!("USAGE: (:annotated [...annotation expressions] value)");
        }
        let annotations: Vec<Symbol> = self
            .expand_tdl_element(environment, arguments[0])
            .iter()
            .map(|e| Symbol::owned(e.as_str().expect("found non-text annotation")))
            .collect();
        let mut values = self.expand_tdl_element(environment, arguments[1]);
        if values.len() > 1 {
            panic!("`annotated` takes a single value.");
        }
        let value = values.pop().unwrap();
        // TODO: Expose a better API for cloning an element while changing its annotations
        vec![Element::new(annotations, value.value)]
    }

    fn tdl_join_text(&self, environment: &Environment, arguments: &[&Element]) -> String {
        let mut new_string = String::new();
        arguments
            .iter()
            .flat_map(|e| self.expand_tdl_element(environment, e))
            .for_each(|e| {
                let text = e.as_str().expect("found non-text argument to `string`");
                new_string.push_str(text);
            });
        new_string
    }

    fn tdl_op_string(&self, environment: &Environment, arguments: &[&Element]) -> Vec<Element> {
        let new_string = self.tdl_join_text(environment, arguments);
        vec![Value::String(new_string).into()]
    }

    fn tdl_op_symbol(&self, environment: &Environment, arguments: &[&Element]) -> Vec<Element> {
        let new_string = self.tdl_join_text(environment, arguments);
        vec![Value::Symbol(Symbol::owned(new_string)).into()]
    }

    fn tdl_join_into_sequence(&self, environment: &Environment, values: &[&Element]) -> Sequence {
        let elements: Vec<Element> = values
            .iter()
            .flat_map(|e| self.expand_tdl_element(environment, e))
            .collect();
        Sequence::new(elements)
    }

    fn tdl_op_list(&self, environment: &Environment, arguments: &[&Element]) -> Vec<Element> {
        let new_sequence = self.tdl_join_into_sequence(environment, arguments);
        vec![Value::List(new_sequence).into()]
    }

    fn tdl_op_sexp(&self, environment: &Environment, arguments: &[&Element]) -> Vec<Element> {
        let new_sequence = self.tdl_join_into_sequence(environment, arguments);
        vec![Value::SExpression(new_sequence).into()]
    }

    // `struct` is a keyword and cannot be a function name
    fn tdl_make_struct(&self, environment: &Environment, arguments: &[&Element]) -> Vec<Element> {
        let elements: Vec<Element> = arguments
            .iter()
            .flat_map(|e| self.expand_tdl_element(environment, e))
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
                let value = elements
                    .get(index)
                    .expect("Found `struct` field name with no corresponding value.");
                new_fields.push((
                    Symbol::owned(field_name_element.as_str().unwrap()),
                    value.to_owned(),
                ))
            }
            index += 1;
        }
        vec![Value::Struct(Struct::from_iter(new_fields)).into()]
    }
}

#[cfg(test)]
mod tests {
    use crate::ion_eq::IonEq;
    use crate::templates::expander::Expander;
    use crate::value::native_reader::NativeElementReader;
    use crate::value::owned::Element;
    use crate::value::reader::ElementReader;
    use std::default::Default;

    fn template_test(template_ion: &str, input_ion: &str, expected_ion: &str) {
        let expander = Expander::from_template_source(template_ion);
        expansion_test(expander, input_ion, expected_ion);
    }

    #[test]
    fn xyz_struct() {
        template_test(
            r#"
            (:define xyz_struct
                ((required any x)
                 (required any y)
                 (required any z))
                 {x: x, y: y, z: z})
            "#,
            r#"
            (:xyz_struct foo bar baz)
            (:xyz_struct cat dog mouse)
            (:xyz_struct 1 2 3)
            "#,
            r#"
            {x: foo, y: bar, z: baz}
            {x: cat, y: dog, z: mouse}
            {x: 1, y: 2, z: 3}
            "#,
        );
    }

    #[test]
    fn tdl_repeat() {
        template_test(
            r#"
            (:define foo
                ()
                (repeat 5 (quote hello)))
            "#,
            "(:foo)",
            "hello hello hello hello hello",
        );
    }

    #[test]
    fn tdl_quote() {
        template_test(
            r#"
            (:define foo
                ()
                (quote (repeat 5 (stream hello))))
            "#,
            "(:foo)",
            "(repeat 5 (stream hello))",
        );
    }

    #[test]
    fn string_concatenation() {
        template_test(
            r#"
            (:define product_url
                (
                    (required any department)
                    (required any product)
                )
                (string "https://example.com/department/" department "/product/" product)
            )
            "#,
            r#"
                (:product_url shoes "abc123")
                (:product_url accessories "def456")
            "#,
            r#"
                "https://example.com/department/shoes/product/abc123"
                "https://example.com/department/accessories/product/def456"
            "#,
        );
    }

    #[test]
    fn tdl_annotated() {
        template_test(
            r#"
            (:define foo
                ((required any x))
                (annotated (stream foo bar baz) x))
            "#,
            "(:foo 71) (:foo quux) (:foo 2022T)",
            "foo::bar::baz::71 foo::bar::baz::quux foo::bar::baz::2022T",
        );
    }

    #[test]
    fn default_values() {
        template_test(
            r#"
            (:define greet
                ((optional any name))
                (string "hello, " (default name "world!")))
            "#,
            r#"
                (:greet)
                (:greet 'Zack!')
            "#,
            r#"
                "hello, world!"
                "hello, Zack!"
            "#,
        );
    }

    #[test]
    fn tdl_each() {
        template_test(
            r#"
            {
                name: object_list,
                parameters: [
                    {name: sequence, encoding: any, cardinality: many},
                ],
                body: [(each x sequence {value: x})]
            }
            "#,
            r#"
                (:object_list (:stream 1 2 3 4 5 6))
            "#,
            r#"
            [
                {value: 1},
                {value: 2},
                {value: 3},
                {value: 4},
                {value: 5},
                {value: 6},
            ]
            "#,
        );
    }

    fn template_expansion_test(expander: Expander, input_ion: &str, expected_ion: &str) {
        let reader = NativeElementReader;
        let input_elements = reader
            .read_all(input_ion.as_bytes())
            .expect("Invalid input Ion");
        let expected_elements = reader
            .read_all(expected_ion.as_bytes())
            .expect("Invalid expected Ion");
        let mut output_elements = vec![];
        for input_element in &input_elements {
            let expanded = expander.expand(&input_element);
            output_elements.extend(expanded);
        }
        if !output_elements.ion_eq(&expected_elements) {
            println!("Expanded output did not match expected output.");
            println!("=== Actual ===");
            show_ion(&output_elements);
            println!("=== Expected ===");
            show_ion(&expected_elements);
        }
        assert_eq!(output_elements, expected_elements);
    }

    fn operator_test(input_ion: &str, expected_ion: &str) {
        expansion_test(Default::default(), input_ion, expected_ion)
    }

    fn expansion_test(expander: Expander, input_ion: &str, expected_ion: &str) {
        let reader = NativeElementReader;
        let input_elements = reader
            .read_all(input_ion.as_bytes())
            .expect("Invalid input Ion");
        let expected_elements = reader
            .read_all(expected_ion.as_bytes())
            .expect("Invalid expected Ion");
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
    fn default() {
        operator_test("foo (:default (:empty) baz) quux", "foo baz quux");
        operator_test("foo (:default bar baz) quux", "foo bar quux");
        operator_test("foo (:default (:inline [1, 2]) baz) quux", "foo 1 2 quux");
        operator_test("foo (:default (:inline []) baz) quux", "foo baz quux");
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
        operator_test(
            "(:string (:inline ('foo' \"bar\" \"\"\"baz\"\"\")))",
            "\"foobarbaz\"",
        );
    }

    #[test]
    fn symbol() {
        operator_test("(:symbol foo bar baz)", "foobarbaz");
        operator_test("(:symbol 'foo' \"bar\" \"\"\"baz\"\"\")", "foobarbaz");
        operator_test(
            "(:symbol (:inline ('foo' \"bar\" \"\"\"baz\"\"\")))",
            "foobarbaz",
        );
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
        operator_test(
            "(:struct foo bar {baz: quux, quuz: gary})",
            "{foo: bar, baz: quux, quuz: gary}",
        );
    }
}
