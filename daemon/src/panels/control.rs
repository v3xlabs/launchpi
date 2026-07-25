use serde::{Deserialize, Serialize};

use crate::{
    bindings::action::ActionBinding, identifiers::ControlId, panels::rendered_state::RenderedState,
    surfaces::layout::SurfacePosition,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Control {
    pub control_id: ControlId,
    pub name: String,
    pub position: SurfacePosition,
    pub default_state: RenderedState,
    pub pressed_state: Option<RenderedState>,
    pub action_bindings: Vec<ActionBinding>,
}

/// A control without its placement. `control_id` and `position` are facts about where a control
/// sits, not about what it is, and something recommending a button cannot know either.
///
/// Serialised, this is exactly a `[[panels.controls]]` table with those two keys removed, which is
/// what makes a recommendation something that can be pasted into a panel file by hand.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ControlTemplate {
    pub name: String,
    pub default_state: RenderedState,
    #[serde(default)]
    pub pressed_state: Option<RenderedState>,
    #[serde(default)]
    pub action_bindings: Vec<ActionBinding>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The portability claim, checked rather than asserted in a comment: a template plus the two
    /// placement keys is a panel-file control.
    #[test]
    fn a_template_plus_a_placement_is_a_panel_control() {
        let template = ControlTemplate {
            name: "Member 1".to_string(),
            default_state: RenderedState {
                text: Some("$(discord.home:channel_members_0)".to_string()),
                ..RenderedState::default()
            },
            pressed_state: None,
            action_bindings: Vec::new(),
        };
        let placed = Control {
            control_id: ControlId("key".to_string()),
            name: template.name.clone(),
            position: SurfacePosition { column: 1, row: 2 },
            default_state: template.default_state.clone(),
            pressed_state: template.pressed_state.clone(),
            action_bindings: template.action_bindings.clone(),
        };

        let mut table = toml::Table::try_from(&template).expect("a template serialises");
        table.insert("control_id".to_string(), "key".into());
        table.insert(
            "position".to_string(),
            toml::Value::try_from(SurfacePosition { column: 1, row: 2 }).expect("a position"),
        );
        assert_eq!(
            table
                .try_into::<Control>()
                .expect("and reads back as a control"),
            placed
        );
    }
}
