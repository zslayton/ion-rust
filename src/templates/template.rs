use crate::IonResult;
use crate::result::{decoding_error, decoding_error_raw};
use crate::value::{IonElement, IonSequence, IonStruct};
use crate::value::owned::Element;

pub struct Template {
    name: String,
    parameters: Vec<Parameter>,
    body: Element,
}

impl Template {
    fn from_ion(element: &Element) -> IonResult<Template> {
        let template_struct = element
            .as_struct()
            .ok_or_else(|| decoding_error_raw("template definition must be an Ion struct"))?;

        let name = template_struct
            .get("name")
            .and_then(|name| name.as_str())
            .ok_or_else(|| decoding_error_raw("template definition must have a text 'name'"))?
            .to_owned();

        let parameters = template_struct
            .get("parameters")
            .and_then(|parameters| parameters.as_sequence())
            .ok_or_else(|| decoding_error_raw("template definition must have a 'parameters' sequence"))?
            .iter()
            .map(Parameter::from_ion)
            .collect::<IonResult<Vec<Parameter>>>()?;

        let body = template_struct
            .get("body")
            .ok_or_else(|| decoding_error_raw("template definition must have a 'body' expression"))?
            .to_owned();

        // let parameters: Vec<Parameter> =
        Ok(Template {
            name,
            parameters,
            body
        })
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn parameters(&self) -> &Vec<Parameter> {
        &self.parameters
    }
    pub fn body(&self) -> &Element {
        &self.body
    }
}

pub enum Encoding {
    Any
}

impl Encoding {
    fn from_ion(element: &Element) -> IonResult<Encoding> {
        let text = element
            .as_str()
            .ok_or_else(|| decoding_error_raw("encoding must be a symbol"))?;
        let encoding = match text {
            "any" => Encoding::Any,
            _ => return decoding_error("unrecognized encoding")
        };
        Ok(encoding)
    }
}

pub enum Cardinality {
    Required,
    Optional,
    Many
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
            _ => return decoding_error("cardinality must be required, optional, or many")
        };
        Ok(cardinality)
    }
}

pub struct Parameter {
    name: String,
    encoding: Encoding,
    cardinality: Cardinality, // required, optional, many
}

impl Parameter {
    fn from_ion(element: &Element) -> IonResult<Parameter> {
        let parameter_struct = element
            .as_struct()
            .ok_or_else(|| decoding_error_raw("parameter definition must be an Ion struct"))?;

        let name = parameter_struct
            .get("name")
            .and_then(|name| name.as_str())
            .ok_or_else(|| decoding_error_raw("parameter definition must have a text 'name' field"))?
            .to_owned();

        let encoding = parameter_struct
            .get("encoding")
            .ok_or_else(|| decoding_error_raw("parameter definition must have an 'encoding' field"))?;
        let encoding = Encoding::from_ion(encoding)?;

        let cardinality = parameter_struct
            .get("cardinality")
            .ok_or_else(|| decoding_error_raw("parameter definition must have a 'cardinality' field"))?;
        let cardinality = Cardinality::from_ion(cardinality)?;

        Ok(Parameter {
            name,
            encoding,
            cardinality
        })
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn encoding(&self) -> &Encoding {
        &self.encoding
    }
    pub fn cardinality(&self) -> &Cardinality {
        &self.cardinality
    }
}
