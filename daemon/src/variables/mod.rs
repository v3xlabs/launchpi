pub mod template;

use std::{collections::HashMap, fmt, sync::RwLock};

use serde::Serialize;

use crate::identifiers::{AssetId, IntegrationId};

/// The namespace `Action::SetVariable` writes into, so a button can hold state without a plugin.
pub const USER_NAMESPACE: &str = "user";

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
pub struct VariableRef {
    pub integration_id: IntegrationId,
    pub name: String,
}

impl VariableRef {
    pub fn new(integration_id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            integration_id: IntegrationId(integration_id.into()),
            name: name.into(),
        }
    }

    pub fn user(name: impl Into<String>) -> Self {
        Self::new(USER_NAMESPACE, name)
    }
}

impl fmt::Display for VariableRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "$({}:{})", self.integration_id.0, self.name)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum VariableValue {
    Text(String),
    Number(f64),
    Boolean(bool),
    Image(AssetId),
}

impl fmt::Display for VariableValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(value) => formatter.write_str(value),
            Self::Number(value) if value.fract() == 0.0 && value.is_finite() => {
                write!(formatter, "{value:.0}")
            }
            Self::Number(value) => write!(formatter, "{value}"),
            Self::Boolean(value) => write!(formatter, "{value}"),
            Self::Image(asset) => formatter.write_str(&asset.0),
        }
    }
}

#[derive(Default)]
pub struct VariableStore {
    values: RwLock<HashMap<VariableRef, VariableValue>>,
}

impl VariableStore {
    /// Returns whether the value actually changed. A poll loop that republishes the same reading
    /// every second should mark nothing dirty and re-render nothing.
    pub fn set(&self, reference: VariableRef, value: VariableValue) -> bool {
        let mut values = self.values.write().unwrap();
        match values.get(&reference) {
            Some(existing) if *existing == value => false,
            _ => {
                values.insert(reference, value);
                true
            }
        }
    }

    pub fn get(&self, reference: &VariableRef) -> Option<VariableValue> {
        self.values.read().unwrap().get(reference).cloned()
    }

    pub fn text(&self, reference: &VariableRef) -> Option<String> {
        self.get(reference).map(|value| value.to_string())
    }

    pub fn clear_one(&self, reference: &VariableRef) {
        self.values.write().unwrap().remove(reference);
    }

    pub fn clear_instance(&self, integration_id: &IntegrationId) -> Vec<VariableRef> {
        let mut values = self.values.write().unwrap();
        let cleared: Vec<_> = values
            .keys()
            .filter(|reference| reference.integration_id == *integration_id)
            .cloned()
            .collect();
        for reference in &cleared {
            values.remove(reference);
        }
        cleared
    }

    pub fn snapshot(&self) -> Vec<(VariableRef, VariableValue)> {
        let mut entries: Vec<_> = self
            .values
            .read()
            .unwrap()
            .iter()
            .map(|(reference, value)| (reference.clone(), value.clone()))
            .collect();
        entries.sort_by(|left, right| {
            left.0
                .integration_id
                .0
                .cmp(&right.0.integration_id.0)
                .then_with(|| left.0.name.cmp(&right.0.name))
        });
        entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setting_an_unchanged_value_reports_no_change() {
        let store = VariableStore::default();
        let reference = VariableRef::new("http.local", "value");
        assert!(store.set(reference.clone(), VariableValue::Number(1.0)));
        assert!(!store.set(reference.clone(), VariableValue::Number(1.0)));
        assert!(store.set(reference, VariableValue::Number(2.0)));
    }

    #[test]
    fn whole_numbers_render_without_a_decimal_point() {
        assert_eq!(VariableValue::Number(21.0).to_string(), "21");
        assert_eq!(VariableValue::Number(21.5).to_string(), "21.5");
    }
}
