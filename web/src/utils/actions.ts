import { Action } from "../api/inventory";
import { PluginCatalogue } from "../api/plugins";

/**
 * What an action does, in the words the picker offered it under. A row headed `set surface
 * display` forces the reader to open it to find out whether it sleeps or wakes; this says so.
 */
export const actionTitle = (action: Action, plugins: PluginCatalogue): string => {
  switch (action.type) {
    case "invoke_integration": {
      const instance = plugins.instances.find(
        entry => entry.integration_id === action.integration_id,
      );
      const label = plugins.types
        .find(type => type.plugin_type === instance?.plugin_type)
        ?.actions.find(definition => definition.name === action.action_name)?.label;

      return [instance?.display_name, label ?? (action.action_name || "Choose an action")]
        .filter(part => part !== undefined)
        .join(" - ");
    }
    case "set_variable": {
      return action.variable_name === "" ? "Set value" : `Set ${action.variable_name}`;
    }
    case "change_panel": {
      return "Change panel";
    }
    case "open_subpanel": {
      return "Open subpanel";
    }
    case "close_subpanel": {
      return "Close subpanel";
    }
    case "set_surface_display": {
      return action.is_display_off ? "Sleep display" : "Wake display";
    }
    case "set_surface_brightness": {
      return `Brightness ${action.brightness}%`;
    }
    case "wait": {
      return `Wait ${action.duration_ms} ms`;
    }
  }
};
