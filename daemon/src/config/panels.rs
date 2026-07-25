use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{config::write_toml, panels::Panel};

#[derive(Deserialize, Serialize)]
struct PanelsDocument {
    version: u8,
    panels: Vec<Panel>,
}

/// Version 2 tags `Action` and `ActionTrigger` for readable TOML. Version 1 files parse
/// identically because no `action_bindings` written under it were ever non-empty.
const PANEL_DOCUMENT_VERSION: u8 = 3;
const SUPPORTED_PANEL_VERSIONS: [u8; 3] = [1, 2, PANEL_DOCUMENT_VERSION];

pub fn load(path: &Path) -> Result<Vec<Panel>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let contents =
        fs::read_to_string(path).with_context(|| format!("unable to read {}", path.display()))?;
    let config: PanelsDocument =
        toml::from_str(&contents).with_context(|| format!("unable to parse {}", path.display()))?;
    if !SUPPORTED_PANEL_VERSIONS.contains(&config.version) {
        anyhow::bail!("unsupported panel configuration version {}", config.version);
    }
    Ok(config.panels)
}

pub fn save(path: &Path, panels: Vec<Panel>) -> Result<()> {
    write_toml(path, &document(panels))
}

pub fn render(panels: Vec<Panel>) -> Result<String> {
    Ok(toml::to_string_pretty(&document(panels))?)
}

fn document(panels: Vec<Panel>) -> PanelsDocument {
    PanelsDocument {
        version: PANEL_DOCUMENT_VERSION,
        panels,
    }
}
