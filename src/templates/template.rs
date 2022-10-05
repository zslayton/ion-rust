use crate::result::{decoding_error, decoding_error_raw};
use crate::value::owned::Element;
use crate::value::{IonElement, IonSequence, IonStruct};
use crate::IonResult;

#[derive(Debug, Clone)]
pub struct Template {
    pub(crate) name: Option<String>,
    pub(crate) parameters: Vec<Parameter>,
    pub(crate) body: Element,
}

impl Template {
    pub(crate) fn from_ion(element: &Element) -> IonResult<Template> {
        let template_struct = element
            .as_struct()
            .ok_or_else(|| decoding_error_raw("template definition must be an Ion struct"))?;

        let name_element = template_struct
            .get("name")
            .ok_or_else(|| decoding_error_raw("template definition must have a 'name' field"))?;

        let name = if name_element.is_null() {
            None
        } else {
            let text = name_element
                .as_str()
                .ok_or_else(|| decoding_error_raw("template 'name' must be text or null"))?;
            Some(text.to_owned())
        };

        let parameters = template_struct
            .get("parameters")
            .and_then(|parameters| parameters.as_sequence())
            .ok_or_else(|| {
                decoding_error_raw("template definition must have a 'parameters' sequence")
            })?
            .iter()
            .map(Parameter::from_ion)
            .collect::<IonResult<Vec<Parameter>>>()?;

        let body = template_struct
            .get("body")
            .ok_or_else(|| decoding_error_raw("template definition must have a 'body' expression"))?
            .to_owned();

        Ok(Template {
            name,
            parameters,
            body,
        })
    }
    pub fn name(&self) -> Option<&str> {
        self.name.as_ref().map(|name| name.as_str())
    }
    pub fn parameters(&self) -> &[Parameter] {
        self.parameters.as_slice()
    }
    pub fn get_parameter<A: AsRef<str>>(&self, name: A) -> Option<&Parameter> {
        self.parameters.iter().find(|p| p.name() == name.as_ref())
    }

    pub fn body(&self) -> &Element {
        &self.body
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Encoding {
    Any,
    Template(String), // TODO: other kinds of template refs
}

impl Encoding {
    fn from_ion(element: &Element) -> IonResult<Encoding> {
        let text = element
            .as_str()
            .ok_or_else(|| decoding_error_raw("encoding must be a symbol"))?;
        let encoding = match text {
            "any" => Encoding::Any,
            template_name if element.has_annotation("template") => {
                Encoding::Template(template_name.to_owned())
            }
            _ => return decoding_error("unrecognized encoding"),
        };
        Ok(encoding)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Cardinality {
    Required,
    Optional,
    Many,
}

impl Cardinality {
    fn from_ion(element: &Element) -> IonResult<Cardinality> {
        let text = element
            .as_str()
            .ok_or_else(|| decoding_error_raw("cardinality must be a symbol"))?;
        let cardinality = match text {
            "required" => Cardinality::Required,
            "optional" => Cardinality::Optional,
            "many" => Cardinality::Many,
            _ => return decoding_error("cardinality must be required, optional, or many"),
        };
        Ok(cardinality)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    name: String,
    encoding: Encoding,
    cardinality: Cardinality,
}

impl Parameter {
    fn from_ion(element: &Element) -> IonResult<Parameter> {
        let parameter_struct = element.as_struct().ok_or_else(|| {
            decoding_error_raw(format!(
                "parameter definition must be an Ion struct, found: {}",
                element
            ))
        })?;

        let name = parameter_struct
            .get("name")
            .and_then(|name| name.as_str())
            .ok_or_else(|| {
                decoding_error_raw("parameter definition must have a text 'name' field")
            })?
            .to_owned();

        let encoding = parameter_struct.get("encoding").ok_or_else(|| {
            decoding_error_raw("parameter definition must have an 'encoding' field")
        })?;
        let encoding = Encoding::from_ion(encoding)?;

        let cardinality = parameter_struct.get("cardinality").ok_or_else(|| {
            decoding_error_raw("parameter definition must have a 'cardinality' field")
        })?;
        let cardinality = Cardinality::from_ion(cardinality)?;

        Ok(Parameter {
            name,
            encoding,
            cardinality,
        })
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn encoding(&self) -> &Encoding {
        &self.encoding
    }
    pub fn cardinality(&self) -> Cardinality {
        self.cardinality
    }
}
