import { isRecord, isString } from "./guards";
import { Control } from "./inventory";

/** A control without its placement: exactly what a preset can know about a button. */
export type ControlTemplate = Pick<
  Control,
  "name" | "default_state" | "pressed_state" | "action_bindings"
>;
export type Preset = {
  preset_id: string;
  category: string;
  name: string;
  description: string | null;
  control: ControlTemplate;
};
export type InstancePresets = {
  integration_id: string;
  display_name: string;
  plugin_type: string;
  presets: Preset[];
};

const isPreset = (value: unknown): value is Preset =>
  isRecord(value)
  && isString(value.preset_id)
  && isString(value.category)
  && isString(value.name)
  && isRecord(value.control)
  && isString(value.control.name)
  && isRecord(value.control.default_state);

const isInstancePresets = (value: unknown): value is InstancePresets =>
  isRecord(value)
  && isString(value.integration_id)
  && isString(value.display_name)
  && isString(value.plugin_type)
  && Array.isArray(value.presets)
  && value.presets.every(isPreset);

export const fetchPresets = async (): Promise<InstancePresets[]> => {
  const response = await fetch("/api/presets");

  if (!response.ok) throw new Error(`Request failed with status ${response.status}`);

  const data: unknown = await response.json();

  if (!Array.isArray(data) || !data.every(isInstancePresets)) {
    throw new Error("Unexpected preset payload.");
  }

  return data;
};
