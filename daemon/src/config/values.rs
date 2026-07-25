use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{config::write_toml, variables::VariableValue};

const VALUES_DOCUMENT_VERSION: u8 = 1;

/// A value the user defined rather than a plugin published. These live in the `user` namespace and
/// are the only values worth persisting: everything else is re-derived from its source on start.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct UserValue {
    pub name: String,
    pub value: toml::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl UserValue {
    pub fn as_variable(&self) -> VariableValue {
        match &self.value {
            toml::Value::Boolean(value) => VariableValue::Boolean(*value),
            toml::Value::Integer(value) => VariableValue::Number(*value as f64),
            toml::Value::Float(value) => VariableValue::Number(*value),
            toml::Value::String(value) => VariableValue::Text(value.clone()),
            other => VariableValue::Text(other.to_string()),
        }
    }
}

#[derive(Deserialize, Serialize)]
struct ValuesDocument {
    version: u8,
    #[serde(default)]
    values: Vec<UserValue>,
}

pub fn load(path: &Path) -> Result<Vec<UserValue>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let contents =
        fs::read_to_string(path).with_context(|| format!("unable to read {}", path.display()))?;
    let document: ValuesDocument =
        toml::from_str(&contents).with_context(|| format!("unable to parse {}", path.display()))?;
    if document.version != VALUES_DOCUMENT_VERSION {
        anyhow::bail!(
            "unsupported value configuration version {}",
            document.version
        );
    }
    Ok(document.values)
}

pub fn save(path: &Path, values: Vec<UserValue>) -> Result<()> {
    write_toml(path, &document(values))
}

pub fn render(values: Vec<UserValue>) -> Result<String> {
    Ok(toml::to_string_pretty(&document(values))?)
}

fn document(values: Vec<UserValue>) -> ValuesDocument {
    ValuesDocument {
        version: VALUES_DOCUMENT_VERSION,
        values,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_maps_onto_the_variable_kind_its_toml_type_implies() {
        let cases = [
            ("\"day\"", VariableValue::Text("day".to_string())),
            ("12", VariableValue::Number(12.0)),
            ("1.5", VariableValue::Number(1.5)),
            ("true", VariableValue::Boolean(true)),
        ];
        for (literal, expected) in cases {
            let parsed: UserValue =
                toml::from_str(&format!("name = \"m\"\nvalue = {literal}")).expect("valid toml");
            assert_eq!(parsed.as_variable(), expected);
        }
    }

    #[test]
    fn values_survive_a_round_trip() {
        let values = vec![UserValue {
            name: "mode".to_string(),
            value: toml::Value::String("day".to_string()),
            description: Some("Which scene the panels show".to_string()),
        }];
        let rendered = render(values.clone()).expect("renders");
        let document: ValuesDocument = toml::from_str(&rendered).expect("round-trips");
        assert_eq!(document.values, values);
    }

    #[test]
    fn an_absent_file_is_an_empty_list_rather_than_an_error() {
        assert_eq!(
            load(Path::new("/nonexistent/launchpi/values.toml")).expect("absent is fine"),
            Vec::new()
        );
    }
}
