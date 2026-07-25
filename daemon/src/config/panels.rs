use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{
    config::write_toml,
    identifiers::PanelId,
    panels::{
        dial::{full_ring, PanelDial},
        rendered_state::RgbaColor,
        Panel,
    },
};

#[derive(Deserialize, Serialize)]
struct PanelsDocument {
    version: u8,
    panels: Vec<Panel>,
}

/// Up to version 3 a panel carried two parallel arrays indexed by dial number instead of the
/// dials it declares. Serde ignores the fields it no longer knows, so those files are read a
/// second time through this view and the dials rebuilt from it.
#[derive(Deserialize)]
struct LegacyDialsDocument {
    panels: Vec<LegacyPanelDials>,
}

#[derive(Deserialize)]
struct LegacyPanelDials {
    panel_id: PanelId,
    #[serde(default)]
    dial_colors: Vec<RgbaColor>,
    #[serde(default)]
    dial_ring_levels: Vec<u8>,
}

/// Version 2 tags `Action` and `ActionTrigger` for readable TOML. Version 1 files parse
/// identically because no `action_bindings` written under it were ever non-empty. Version 4
/// replaces the parallel dial arrays with `[[panels.dials]]`.
const PANEL_DOCUMENT_VERSION: u8 = 4;
const SUPPORTED_PANEL_VERSIONS: [u8; 4] = [1, 2, 3, PANEL_DOCUMENT_VERSION];

pub fn load(path: &Path) -> Result<Vec<Panel>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let contents =
        fs::read_to_string(path).with_context(|| format!("unable to read {}", path.display()))?;
    parse(&contents).with_context(|| format!("unable to parse {}", path.display()))
}

fn parse(contents: &str) -> Result<Vec<Panel>> {
    let config: PanelsDocument = toml::from_str(contents)?;
    if !SUPPORTED_PANEL_VERSIONS.contains(&config.version) {
        anyhow::bail!("unsupported panel configuration version {}", config.version);
    }
    let mut panels = config.panels;
    if config.version < PANEL_DOCUMENT_VERSION {
        let legacy: LegacyDialsDocument = toml::from_str(contents)?;
        adopt_legacy_dials(&mut panels, legacy.panels);
    }
    Ok(panels)
}

fn adopt_legacy_dials(panels: &mut [Panel], legacy: Vec<LegacyPanelDials>) {
    for entry in legacy {
        let Some(panel) = panels
            .iter_mut()
            .find(|panel| panel.panel_id == entry.panel_id)
            .filter(|panel| panel.dials.is_empty())
        else {
            continue;
        };
        let levels = entry.dial_ring_levels;
        panel.dials = entry
            .dial_colors
            .into_iter()
            .enumerate()
            .filter_map(|(index, color)| {
                Some(PanelDial {
                    index: u8::try_from(index).ok()?,
                    level: levels.get(index).copied().unwrap_or_else(full_ring),
                    color,
                })
            })
            .collect();
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surfaces::defaults::default_panel;

    const LEGACY_DOCUMENT: &str = r#"
version = 3

[[panels]]
panel_id = "studio-panel-1"
name = "Hello"
controls = []
dial_ring_levels = [90]

[panels.layout]
columns = 16
rows = 2

[[panels.dial_colors]]
red = 35
green = 88
blue = 165
alpha = 255

[[panels.dial_colors]]
red = 7
green = 37
blue = 85
alpha = 255
"#;

    #[test]
    fn reads_the_dials_of_a_pre_version_4_panel_from_its_parallel_arrays() {
        let panels = parse(LEGACY_DOCUMENT).expect("a version 3 document should still load");
        let dials = &panels[0].dials;

        assert_eq!(dials.len(), 2);
        assert_eq!((dials[0].index, dials[0].level), (0, 90));
        assert_eq!(dials[0].color.red, 35);
        // A colour with no matching level starts from a full ring.
        assert_eq!((dials[1].index, dials[1].level), (1, 100));
    }

    #[test]
    fn declared_dials_survive_a_round_trip_through_the_document() {
        let rendered = render(vec![default_panel()]).expect("panels should serialise");
        let panels = parse(&rendered).expect("what we write should parse back");

        assert_eq!(panels[0].dials, default_panel().dials);
    }
}
