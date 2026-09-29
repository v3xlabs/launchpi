use std::{collections::BTreeMap, fmt::Write};

use anyhow::{Context, Result};
use serde::Serialize;

use crate::{
    config::{
        devices::PersistedDevice,
        nix::{NixAttribute, NixExpression, NixString},
        plugins::exported_document,
        values::UserValue,
    },
    panels::Panel,
    plugins::{
        engine::manifest_for,
        instance::{InstanceDocument, InstanceIdentity},
    },
};

/// One persisted unit of configuration, in the form it is written to disk.
pub enum Entry {
    Device(PersistedDevice),
    Panel(Panel),
    Plugin(InstanceIdentity, InstanceDocument),
    Value(UserValue),
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    Device,
    Panel,
    Plugin,
    Value,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Change {
    Added,
    Changed,
    Removed,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
pub struct ConfigurationChange {
    pub kind: EntryKind,
    #[serde(rename = "entry_id")]
    pub id: String,
    pub name: String,
    pub change: Change,
}

#[derive(Serialize)]
pub struct ConfigurationChanges {
    pub changes: Vec<ConfigurationChange>,
    pub toml: String,
    pub nix: String,
}

/// Configuration reduced to what is persisted, keyed so two snapshots compare entry by entry.
pub struct Snapshot(BTreeMap<(EntryKind, String), Recorded>);

struct Recorded {
    name: String,
    document: toml::Value,
}

impl Entry {
    fn kind(&self) -> EntryKind {
        match self {
            Self::Device(_) => EntryKind::Device,
            Self::Panel(_) => EntryKind::Panel,
            Self::Plugin(..) => EntryKind::Plugin,
            Self::Value(_) => EntryKind::Value,
        }
    }

    fn id(&self) -> String {
        match self {
            Self::Device(device) => device.surface_id.0.clone(),
            Self::Panel(panel) => panel.panel_id.0.clone(),
            Self::Plugin(identity, _) => identity.integration_id().0,
            Self::Value(value) => value.name.clone(),
        }
    }

    fn name(&self) -> String {
        match self {
            Self::Device(device) => device.name.clone(),
            Self::Panel(panel) => panel.name.clone(),
            Self::Plugin(identity, document) => document
                .display_name
                .clone()
                .unwrap_or_else(|| identity.integration_id().0),
            Self::Value(value) => value.name.clone(),
        }
    }

    fn document(&self) -> Result<toml::Value> {
        Ok(match self {
            Self::Device(device) => toml::Value::try_from(device)?,
            Self::Panel(panel) => toml::Value::try_from(panel)?,
            Self::Plugin(_, document) => toml::Value::try_from(document)?,
            Self::Value(value) => toml::Value::try_from(value)?,
        })
    }

    /// The entry as TOML: a `[[devices]]`, `[[panels]]` or `[[values]]` table to append to its
    /// file, or a whole plugin instance file.
    pub fn toml(&self) -> Result<String> {
        Ok(match self {
            Self::Device(device) => {
                toml::to_string_pretty(&BTreeMap::from([("devices", [device])]))?
            }
            Self::Panel(panel) => toml::to_string_pretty(&BTreeMap::from([("panels", [panel])]))?,
            Self::Plugin(identity, document) => {
                toml::to_string_pretty(&exported_plugin(identity, document)?)?
            }
            Self::Value(value) => toml::to_string_pretty(&BTreeMap::from([("values", [value])]))?,
        })
    }

    /// The entry as the attribute set `services.launchpi.settings` takes for it.
    pub fn nix(&self) -> Result<String> {
        let value = match self {
            Self::Device(device) => toml::Value::Table(device.nix_settings()?),
            Self::Panel(panel) => toml::Value::try_from(panel)?,
            Self::Plugin(identity, document) => {
                let mut table = toml::Table::try_from(exported_plugin(identity, document)?)?;
                table.remove("version");
                toml::Value::Table(table)
            }
            Self::Value(value) => toml::Value::try_from(value)?,
        };
        Ok(NixExpression(&value).to_string())
    }
}

impl EntryKind {
    fn settings_option(self) -> &'static str {
        match self {
            Self::Device => "devices",
            Self::Panel => "panels",
            Self::Plugin => "plugins",
            Self::Value => "values",
        }
    }

    fn key_field(self) -> Option<&'static str> {
        match self {
            Self::Device => Some("surface_id"),
            Self::Panel => Some("panel_id"),
            Self::Plugin => None,
            Self::Value => Some("name"),
        }
    }
}

impl Change {
    fn as_str(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Changed => "changed",
            Self::Removed => "removed",
        }
    }
}

impl ConfigurationChange {
    fn toml_header(&self) -> String {
        let change = self.change.as_str();
        match self.kind.key_field() {
            Some(field) => format!(
                "# {}.toml, {change} {field} = {}\n",
                self.kind.settings_option(),
                toml::Value::String(self.id.clone())
            ),
            None => format!("# plugins/{}.toml, {change}\n", self.id),
        }
    }

    fn nix_header(&self) -> String {
        let option = self.kind.settings_option();
        let target = match self.kind.key_field() {
            Some(field) => format!("the entry with {field} = {}", NixString(&self.id)),
            None => format!("plugins.{}", NixAttribute(&self.id)),
        };
        match self.change {
            Change::Added => format!("# services.launchpi.settings.{option}, added\n"),
            Change::Changed => {
                format!("# services.launchpi.settings.{option}, changed: replace {target}\n")
            }
            Change::Removed => {
                format!("# services.launchpi.settings.{option}, removed: delete {target}\n")
            }
        }
    }
}

impl ConfigurationChanges {
    pub fn new(baseline: &Snapshot, entries: &[Entry]) -> Result<Self> {
        let changes = Snapshot::new(entries)?.changes_since(baseline);
        let current: BTreeMap<_, _> = entries
            .iter()
            .map(|entry| ((entry.kind(), entry.id()), entry))
            .collect();
        let mut toml = String::new();
        let mut nix = String::new();
        for change in &changes {
            if !toml.is_empty() {
                toml.push('\n');
                nix.push('\n');
            }
            toml.push_str(&change.toml_header());
            nix.push_str(&change.nix_header());
            let Some(entry) = current.get(&(change.kind, change.id.clone())) else {
                continue;
            };
            match entry.toml() {
                Ok(fragment) => toml.push_str(&fragment),
                Err(error) => writeln!(toml, "# cannot export: {error:#}")?,
            }
            match entry.nix() {
                Ok(expression) if change.kind == EntryKind::Plugin => {
                    writeln!(nix, "plugins.{} = {expression};", NixAttribute(&change.id))?
                }
                Ok(expression) => writeln!(nix, "{expression}")?,
                Err(error) => writeln!(nix, "# cannot export: {error:#}")?,
            }
        }
        Ok(Self { changes, toml, nix })
    }
}

impl Snapshot {
    pub fn new(entries: &[Entry]) -> Result<Self> {
        entries
            .iter()
            .map(|entry| {
                let recorded = Recorded {
                    name: entry.name(),
                    document: entry.document()?,
                };
                Ok(((entry.kind(), entry.id()), recorded))
            })
            .collect::<Result<_>>()
            .map(Self)
    }

    pub fn changes_since(&self, baseline: &Self) -> Vec<ConfigurationChange> {
        let mut changes: Vec<_> = self
            .0
            .iter()
            .filter_map(|(key, current)| {
                let change = match baseline.0.get(key) {
                    None => Change::Added,
                    Some(before) if before.document != current.document => Change::Changed,
                    Some(_) => return None,
                };
                Some((key, &current.name, change))
            })
            .chain(
                baseline
                    .0
                    .iter()
                    .filter(|(key, _)| !self.0.contains_key(key))
                    .map(|(key, before)| (key, &before.name, Change::Removed)),
            )
            .map(|((kind, id), name, change)| ConfigurationChange {
                kind: *kind,
                id: id.clone(),
                name: name.clone(),
                change,
            })
            .collect();
        changes.sort_by(|left, right| (left.kind, &left.id).cmp(&(right.kind, &right.id)));
        changes
    }
}

/// Unknown plugin types have no manifest to say which fields are secret, so they are not exported
/// at all rather than risk a credential in clear text.
fn exported_plugin(
    identity: &InstanceIdentity,
    document: &InstanceDocument,
) -> Result<InstanceDocument> {
    let manifest = manifest_for(&identity.plugin_type).with_context(|| {
        format!(
            "{} is not a plugin type this daemon knows",
            identity.plugin_type
        )
    })?;
    exported_document(&identity.integration_id(), document, &manifest).map_err(anyhow::Error::msg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surfaces::{
        defaults::default_panel, managed::NetworkSurfaceStatus, registry::SurfaceRegistry,
    };

    fn value(name: &str, value: &str) -> Entry {
        Entry::Value(UserValue {
            name: name.to_string(),
            value: toml::Value::String(value.to_string()),
            description: None,
        })
    }

    #[test]
    fn entries_are_added_changed_or_removed_relative_to_the_baseline() {
        let mut renamed = default_panel();
        renamed.name = "Renamed".to_string();
        let baseline = Snapshot::new(&[
            Entry::Panel(default_panel()),
            value("kept", "same"),
            value("dropped", "gone"),
        ])
        .expect("snapshots");
        let current = Snapshot::new(&[
            Entry::Panel(renamed),
            value("kept", "same"),
            value("fresh", "new"),
        ])
        .expect("snapshots");

        let changes: Vec<_> = current
            .changes_since(&baseline)
            .into_iter()
            .map(|change| (change.kind, change.id, change.name, change.change))
            .collect();
        assert_eq!(
            changes,
            vec![
                (
                    EntryKind::Panel,
                    default_panel().panel_id.0,
                    "Renamed".to_string(),
                    Change::Changed
                ),
                (
                    EntryKind::Value,
                    "dropped".to_string(),
                    "dropped".to_string(),
                    Change::Removed
                ),
                (
                    EntryKind::Value,
                    "fresh".to_string(),
                    "fresh".to_string(),
                    Change::Added
                ),
            ]
        );
    }

    #[test]
    fn runtime_state_of_a_device_is_not_a_change() {
        let registry = SurfaceRegistry::from_configuration(Vec::new(), vec![default_panel()]);
        let device = registry.managed_surfaces().remove(0);
        let mut running = device.clone();
        running.status = NetworkSurfaceStatus::Unavailable;
        running.last_error = Some("refused".to_string());
        running.is_display_off = true;

        let baseline =
            Snapshot::new(&[Entry::Device(PersistedDevice::from(device))]).expect("snapshots");
        let current =
            Snapshot::new(&[Entry::Device(PersistedDevice::from(running))]).expect("snapshots");
        assert!(current.changes_since(&baseline).is_empty());
    }
}
