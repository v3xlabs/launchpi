use serde::Serialize;

/// Everything the daemon and the web UI need to know about a plugin type without an instance of it
/// existing. The same `ConfigField` shape describes instance configuration, action parameters and
/// feedback parameters, so the UI renders all three through one component.
#[derive(Clone, Debug, Serialize)]
pub struct PluginManifest {
    pub plugin_type: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    pub config_schema: Vec<ConfigField>,
    pub actions: Vec<ActionDefinition>,
    pub variables: Vec<VariableDefinition>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ConfigField {
    pub key: String,
    pub label: String,
    pub kind: ConfigFieldKind,
    pub is_required: bool,
    pub placeholder: Option<String>,
    pub help: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConfigFieldKind {
    Text,
    Number,
    Boolean,
    /// Never echoed back to the browser, and rewritten on export. Declaring a field secret is the
    /// only signal those two paths have.
    Secret,
    Select {
        options: Vec<SelectOption>,
    },
    /// Options the instance supplies at runtime, fetched from
    /// `GET /api/plugins/:id/lookup/:source`. Free text stays valid, so a raw id still works when
    /// the instance is offline or the thing you want is not in the list.
    Lookup {
        source: String,
    },
}

#[derive(Clone, Debug, Serialize)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

impl ConfigField {
    fn new(key: impl Into<String>, kind: ConfigFieldKind) -> Self {
        let key = key.into();
        Self {
            label: key.clone(),
            key,
            kind,
            is_required: false,
            placeholder: None,
            help: None,
        }
    }

    pub fn text(key: impl Into<String>) -> Self {
        Self::new(key, ConfigFieldKind::Text)
    }

    pub fn number(key: impl Into<String>) -> Self {
        Self::new(key, ConfigFieldKind::Number)
    }

    pub fn boolean(key: impl Into<String>) -> Self {
        Self::new(key, ConfigFieldKind::Boolean)
    }

    pub fn secret(key: impl Into<String>) -> Self {
        Self::new(key, ConfigFieldKind::Secret)
    }

    pub fn lookup(key: impl Into<String>, source: impl Into<String>) -> Self {
        Self::new(
            key,
            ConfigFieldKind::Lookup {
                source: source.into(),
            },
        )
    }

    pub fn select<I, S>(key: impl Into<String>, options: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let options = options
            .into_iter()
            .map(|option| {
                let value = option.into();
                SelectOption {
                    label: value.clone(),
                    value,
                }
            })
            .collect();
        Self::new(key, ConfigFieldKind::Select { options })
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    pub fn required(mut self) -> Self {
        self.is_required = true;
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn is_secret(&self) -> bool {
        matches!(self.kind, ConfigFieldKind::Secret)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ActionDefinition {
    pub name: String,
    pub label: String,
    pub description: Option<String>,
    pub parameters: Vec<ConfigField>,
}

impl ActionDefinition {
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            label: name.clone(),
            name,
            description: None,
            parameters: Vec::new(),
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn parameters(mut self, parameters: Vec<ConfigField>) -> Self {
        self.parameters = parameters;
        self
    }
}


#[derive(Clone, Debug, Serialize)]
pub struct VariableDefinition {
    pub name: String,
    pub label: String,
    pub description: Option<String>,
    pub kind: VariableKind,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VariableKind {
    Text,
    Number,
    Boolean,
    Image,
}

impl VariableDefinition {
    pub fn new(name: impl Into<String>, kind: VariableKind) -> Self {
        let name = name.into();
        Self {
            label: name.clone(),
            name,
            description: None,
            kind,
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}
