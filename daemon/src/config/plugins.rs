use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::{
    config::{is_read_only, secret::SecretRef},
    identifiers::IntegrationId,
    plugins::{
        instance::{parse_instance_stem, InstanceDocument, InstanceIdentity},
        manifest::PluginManifest,
    },
};

pub const PLUGIN_DIRECTORY_NAME: &str = "plugins";

/// A file in `plugins/`. Unreadable and unparseable files are carried rather than discarded so the
/// UI can show why an instance did not start, instead of the file silently vanishing.
#[derive(Clone, Debug)]
pub enum InstanceFile {
    Loaded {
        identity: InstanceIdentity,
        document: InstanceDocument,
    },
    Invalid {
        file_name: String,
        reason: String,
    },
}

pub struct PluginDirectory {
    root: PathBuf,
}

impl PluginDirectory {
    pub fn open(config_directory: &Path) -> Result<Self> {
        let root = config_directory.join(PLUGIN_DIRECTORY_NAME);
        fs::create_dir_all(&root)
            .with_context(|| format!("unable to create {}", root.display()))?;
        Ok(Self { root })
    }

    pub fn list(&self) -> Result<Vec<InstanceFile>> {
        let mut files = Vec::new();
        for entry in fs::read_dir(&self.root)
            .with_context(|| format!("unable to read {}", self.root.display()))?
        {
            let path = entry?.path();
            if path.extension().is_none_or(|extension| extension != "toml") {
                continue;
            }
            files.push(read_instance_file(&path));
        }
        files.sort_by_key(|file| match file {
            InstanceFile::Loaded { identity, .. } => identity.file_name(),
            InstanceFile::Invalid { file_name, .. } => file_name.clone(),
        });
        Ok(files)
    }

    pub fn save(&self, identity: &InstanceIdentity, document: &InstanceDocument) -> Result<()> {
        if is_read_only() {
            return Ok(());
        }
        let path = self.root.join(identity.file_name());
        let temporary_path = path.with_extension("toml.tmp");
        fs::write(&temporary_path, toml::to_string_pretty(document)?)?;
        restrict_permissions(&temporary_path)?;
        fs::rename(temporary_path, path)?;
        Ok(())
    }

    pub fn delete(&self, identity: &InstanceIdentity) -> Result<()> {
        if is_read_only() {
            return Ok(());
        }
        let path = self.root.join(identity.file_name());
        if path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("unable to remove {}", path.display()))?;
        }
        Ok(())
    }
}

fn read_instance_file(path: &Path) -> InstanceFile {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let invalid = |reason: String| InstanceFile::Invalid {
        file_name: file_name.clone(),
        reason,
    };

    let stem = match path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
    {
        Some(stem) => stem,
        None => return invalid("file has no name".to_string()),
    };
    let identity = match parse_instance_stem(&stem) {
        Ok(identity) => identity,
        Err(reason) => return invalid(reason),
    };
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) => return invalid(error.to_string()),
    };
    match toml::from_str::<InstanceDocument>(&contents) {
        Ok(document) => InstanceFile::Loaded { identity, document },
        Err(error) => invalid(error.to_string()),
    }
}

/// The inline secret form is permitted, so an instance file may hold a credential.
#[cfg(unix)]
fn restrict_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

/// Renders an instance file with every inline secret rewritten into an environment reference, so
/// the result is safe to paste somewhere public and still usable once that variable is set.
pub fn export_document(
    integration_id: &IntegrationId,
    document: &InstanceDocument,
    manifest: &PluginManifest,
) -> Result<String, String> {
    let mut exported = document.clone();
    for field in manifest
        .config_schema
        .iter()
        .filter(|field| field.is_secret())
    {
        let Some(value) = exported.config.get(&field.key) else {
            continue;
        };
        let reference = SecretRef::deserialize(value.clone())
            .map_err(|error| format!("{} is not a valid secret reference: {error}", field.key))?;
        let replacement = toml::Value::try_from(reference.exported(integration_id, &field.key))
            .map_err(|error| error.to_string())?;
        exported.config.insert(field.key.clone(), replacement);
    }
    toml::to_string_pretty(&exported).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::manifest::ConfigField;

    fn manifest() -> PluginManifest {
        PluginManifest {
            plugin_type: "hass",
            display_name: "Home Assistant",
            description: "",
            config_schema: vec![ConfigField::text("url"), ConfigField::secret("token")],
            actions: Vec::new(),
            variables: Vec::new(),
        }
    }

    fn document(config: &str) -> InstanceDocument {
        InstanceDocument {
            config: toml::from_str(config).expect("valid toml"),
            ..InstanceDocument::default()
        }
    }

    #[test]
    fn exporting_rewrites_an_inline_secret_into_an_environment_reference() {
        let exported = export_document(
            &IntegrationId("hass.home".to_string()),
            &document("url = \"http://hass.local\"\ntoken = \"hunter2\""),
            &manifest(),
        )
        .expect("exports");
        assert!(!exported.contains("hunter2"));
        assert!(exported.contains("LAUNCHPI_HASS_HOME_TOKEN"));
        assert!(exported.contains("http://hass.local"));
    }

    #[test]
    fn exporting_leaves_an_indirect_reference_untouched() {
        let exported = export_document(
            &IntegrationId("hass.home".to_string()),
            &document("token = { file = \"/run/agenix/token\" }"),
            &manifest(),
        )
        .expect("exports");
        assert!(exported.contains("/run/agenix/token"));
    }

    #[test]
    fn exporting_does_not_touch_fields_the_manifest_never_declared_secret() {
        let exported = export_document(
            &IntegrationId("hass.home".to_string()),
            &document("url = \"http://hass.local\""),
            &manifest(),
        )
        .expect("exports");
        assert!(exported.contains("http://hass.local"));
    }
}
