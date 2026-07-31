import {
  fetchText,
  getErrorMessage,
  isNumber,
  isOptionalString,
  isRecord,
  isString,
  request,
} from "./guards";
import { isPluginInstance, PluginInstance } from "./plugins";

export type DeviceStatus = "connecting" | "connected" | "unavailable" | "disabled";
export type RgbaColor = { red: number; green: number; blue: number; alpha: number; };
export type Capabilities = {
  supports_color: boolean;
  supports_images: boolean;
  supports_text: boolean;
  supports_brightness: boolean;
  supports_haptics: boolean;
};
export type GridLayout = { columns: number; rows: number; };
/** A colour is either written out (a table) or read from a value (a `$(...)` string). */
export type ColorBinding = RgbaColor | string;
export type Anchor9
  = | "top_start" | "top_center" | "top_end"
    | "center_start" | "center" | "center_end"
    | "bottom_start" | "bottom_center" | "bottom_end";
export type ContentLayout = { text_anchor: Anchor9; };
export type SubpanelPlacement
  = | "top_start" | "top_center" | "top_end" | "start_center"
    | "bottom_start" | "bottom_center" | "bottom_end" | "end_center";
/** `cover` crops a picture to fill its square; `contain` fits the whole picture inside it. */
export type Fit = "cover" | "contain";
export type Edge = "top" | "bottom" | "start" | "end";
/** A number written out, or a `$(...)` string that reads one from a value. */
export type ValueBinding = number | string;
/** A key's face, drawn in array order: index 0 first, later entries on top. */
export type Layer
  = | { kind: "fill"; color: ColorBinding; }
    | {
      kind: "image";
      image: string;
      fit: Fit;
      anchor: Anchor9;
      scale_percent: number;
      tint: ColorBinding | null;
    }
    | { kind: "text"; text: string; color: ColorBinding; anchor: Anchor9; font_family?: string; font_size?: number; }
    | {
      kind: "bar";
      value: ValueBinding;
      maximum: ValueBinding;
      color: ColorBinding;
      edge: Edge;
      thickness: number;
    }
    | { kind: "border"; color: ColorBinding; width: number; };
export type LayerKind = Layer["kind"];
export type RenderedState = {
  layers: Layer[];
  is_pressed: boolean;
};
export type ActionTrigger
  = | "press"
    | "release"
    | "rotate_clockwise"
    | "rotate_counter_clockwise"
    | "value_changed"
    | { hold: { duration_ms: number; }; };
export type Action
  = | {
    type: "invoke_integration";
    integration_id: string;
    action_name: string;
    parameters: Record<string, unknown>;
  }
  | { type: "set_variable"; variable_name: string; value: unknown; }
  | { type: "change_panel"; panel_id: string; }
  | {
    type: "open_subpanel";
    panel_id: string;
    placement: SubpanelPlacement;
    offset_columns: number;
    offset_rows: number;
  }
  | { type: "close_subpanel"; }
  | { type: "wait"; duration_ms: number; };
export type ActionBinding = { gesture: ActionTrigger; actions: Action[]; };
export type Control = {
  control_id: string;
  name: string;
  position: { column: number; row: number; };
  default_state: RenderedState;
  pressed_state: RenderedState | null;
  action_bindings: ActionBinding[];
};
/** One rotary dial a panel declares. `level` is the percentage of the ring lit when it loads. */
export type PanelDial = { index: number; level: number; color: RgbaColor; };
/**
 * Where a knob sits on a device, in that device's key-grid cell coordinates: `(0, 0)` is the
 * top-left key, so a negative column or a column past the last one puts the knob beside the keys.
 * The daemon's model table is the only place this is written down.
 */
export type DialPlacement = { index: number; column: number; row: number; row_span: number; };
export type Panel = {
  panel_id: string;
  name: string;
  layout: GridLayout;
  font_family?: string;
  capabilities: Capabilities;
  controls: Control[];
  dials: PanelDial[];
};
export type Device = {
  surface_id: string;
  name: string;
  host: string;
  port: number;
  serial_number: string | null;
  model: string;
  layout: unknown;
  capabilities: Capabilities;
  dials: DialPlacement[];
  active_panel_id: string | null;
  open_subpanels: Array<{ panel_id: string; column: number; row: number; }>;
  is_enabled: boolean;
  parent_surface_id: string | null;
  status: DeviceStatus;
  last_error: string | null;
};
export type SurfacePresentation = {
  columns: number;
  rows: number;
  controls: Array<{ control: Control; key_index: number; is_dimmed: boolean; }>;
};
export type DiscoveredDevice = {
  discovery_id: string;
  name: string;
  host: string;
  port: number;
  serial_number: string | null;
  model: string;
};
export type KeyEvent = { surface_id: string; key_index: number; is_pressed: boolean; };
/** Where a dial currently stands on a connected device, as a percentage of its ring. */
export type DialState = { surface_id: string; dial_index: number; level: number; };
export type DialPress = { surface_id: string; dial_index: number; is_pressed: boolean; };
export type LogLevel = "input" | "info" | "warning";
/** One line of a device's activity log. `sequence` is monotonic per daemon run. */
export type LogEntry = {
  surface_id: string;
  sequence: number;
  at_ms: number;
  level: LogLevel;
  message: string;
};
export type Inventory = {
  discovered: DiscoveredDevice[];
  devices: Device[];
  panels: Panel[];
  plugin_instances: PluginInstance[];
  recent_key_events: KeyEvent[];
  key_states: KeyEvent[];
  dial_states: DialState[];
  dial_presses: DialPress[];
  logs: LogEntry[];
};

export const capabilityKeys: Array<keyof Capabilities> = [
  "supports_color",
  "supports_images",
  "supports_text",
  "supports_brightness",
  "supports_haptics",
];
export const capabilityLabels: Array<{ key: keyof Capabilities; label: string; }> = [
  { key: "supports_color", label: "Color" },
  { key: "supports_images", label: "Images" },
  { key: "supports_text", label: "Text" },
  { key: "supports_brightness", label: "Brightness" },
  { key: "supports_haptics", label: "Haptics" },
];

export const emptyCapabilities: Capabilities = {
  supports_color: false,
  supports_images: false,
  supports_text: true,
  supports_brightness: false,
  supports_haptics: false,
};
export const defaultContentLayout: ContentLayout = { text_anchor: "center" };
export const emptyInventory: Inventory = {
  discovered: [],
  devices: [],
  panels: [],
  plugin_instances: [],
  recent_key_events: [],
  key_states: [],
  dial_states: [],
  dial_presses: [],
  logs: [],
};

const logLevels = new Set<string>(["input", "info", "warning"] satisfies LogLevel[]);

const isCapabilities = (value: unknown): value is Capabilities =>
  isRecord(value) && capabilityKeys.every(key => typeof value[key] === "boolean");
const isColor = (value: unknown): value is RgbaColor =>
  isRecord(value)
  && isNumber(value.red)
  && isNumber(value.green)
  && isNumber(value.blue)
  && isNumber(value.alpha);
const isGridLayout = (value: unknown): value is GridLayout =>
  isRecord(value) && isNumber(value.columns) && isNumber(value.rows);
const isBoundColor = (value: unknown): boolean => isString(value) || isColor(value);
const isLayer = (value: unknown): value is Layer => {
  if (!isRecord(value)) return false;

  switch (value.kind) {
    case "fill":
    case "border": {
      return isBoundColor(value.color);
    }
    case "image": {
      return isString(value.image) && isString(value.anchor) && isNumber(value.scale_percent);
    }
    case "text": {
      return isString(value.text) && isBoundColor(value.color) && isString(value.anchor) && (value.font_size === undefined || isNumber(value.font_size));
    }
    case "bar": {
      return isBoundColor(value.color) && isNumber(value.thickness);
    }
    default: {
      return false;
    }
  }
};
const isRenderedState = (value: unknown): value is RenderedState =>
  isRecord(value)
  && Array.isArray(value.layers)
  && value.layers.every(isLayer)
  && typeof value.is_pressed === "boolean";
const isControl = (value: unknown): value is Control =>
  isRecord(value)
  && isString(value.control_id)
  && isString(value.name)
  && isRecord(value.position)
  && isNumber(value.position.column)
  && isNumber(value.position.row)
  && isRenderedState(value.default_state)
  && (value.pressed_state === null || isRenderedState(value.pressed_state))
  && Array.isArray(value.action_bindings);
const isPanelDial = (value: unknown): value is PanelDial =>
  isRecord(value) && isNumber(value.index) && isNumber(value.level) && isColor(value.color);
const isDialPlacement = (value: unknown): value is DialPlacement =>
  isRecord(value)
  && isNumber(value.index)
  && isNumber(value.column)
  && isNumber(value.row)
  && isNumber(value.row_span);
const isPanel = (value: unknown): value is Panel =>
  isRecord(value)
  && isString(value.panel_id)
  && isString(value.name)
  && isGridLayout(value.layout)
  && isCapabilities(value.capabilities)
  && Array.isArray(value.controls)
  && value.controls.every(isControl)
  && (value.dials === undefined || (Array.isArray(value.dials) && value.dials.every(isPanelDial)));
const isDevice = (value: unknown): value is Device =>
  isRecord(value)
  && isString(value.surface_id)
  && isString(value.name)
  && isString(value.host)
  && isNumber(value.port)
  && isOptionalString(value.serial_number)
  && isString(value.model)
  && isCapabilities(value.capabilities)
  && (value.dials === undefined || (Array.isArray(value.dials) && value.dials.every(isDialPlacement)))
  && isOptionalString(value.active_panel_id)
  && (value.open_subpanels === undefined || Array.isArray(value.open_subpanels))
  && typeof value.is_enabled === "boolean"
  && (value.parent_surface_id === undefined || isOptionalString(value.parent_surface_id))
  && ["connecting", "connected", "unavailable", "disabled"].includes(String(value.status))
  && isOptionalString(value.last_error);
const isDiscoveredDevice = (value: unknown): value is DiscoveredDevice =>
  isRecord(value)
  && isString(value.discovery_id)
  && isString(value.name)
  && isString(value.host)
  && isNumber(value.port)
  && isOptionalString(value.serial_number)
  && isString(value.model);
const isKeyEvent = (value: unknown): value is KeyEvent =>
  isRecord(value)
  && isString(value.surface_id)
  && isNumber(value.key_index)
  && typeof value.is_pressed === "boolean";
const isDialState = (value: unknown): value is DialState =>
  isRecord(value) && isString(value.surface_id) && isNumber(value.dial_index) && isNumber(value.level);
const isDialPress = (value: unknown): value is DialPress =>
  isRecord(value)
  && isString(value.surface_id)
  && isNumber(value.dial_index)
  && typeof value.is_pressed === "boolean";
const isLogLevel = (value: unknown): value is LogLevel =>
  isString(value) && logLevels.has(value);

export const isLogEntry = (value: unknown): value is LogEntry =>
  isRecord(value)
  && isString(value.surface_id)
  && isNumber(value.sequence)
  && isNumber(value.at_ms)
  && isLogLevel(value.level)
  && isString(value.message);
const isInventory = (value: unknown): value is Inventory =>
  isRecord(value)
  && Array.isArray(value.discovered)
  && value.discovered.every(isDiscoveredDevice)
  && Array.isArray(value.devices)
  && value.devices.every(isDevice)
  && Array.isArray(value.panels)
  && value.panels.every(isPanel)
  && (value.plugin_instances === undefined
    || (Array.isArray(value.plugin_instances) && value.plugin_instances.every(isPluginInstance)))
  && Array.isArray(value.recent_key_events)
  && value.recent_key_events.every(isKeyEvent)
  && (value.key_states === undefined
    || (Array.isArray(value.key_states) && value.key_states.every(isKeyEvent)))
  && (value.dial_states === undefined
    || (Array.isArray(value.dial_states) && value.dial_states.every(isDialState)))
  && (value.dial_presses === undefined
    || (Array.isArray(value.dial_presses) && value.dial_presses.every(isDialPress)))
  && (value.logs === undefined || (Array.isArray(value.logs) && value.logs.every(isLogEntry)));

export const deviceGridLayout = (value: unknown): GridLayout | null => {
  if (isGridLayout(value)) return value;

  if (!isRecord(value)) return null;

  if (isGridLayout(value.Grid)) return value.Grid;

  if (isGridLayout(value.grid)) return value.grid;

  return null;
};

export const panelDial = (panel: Panel, index: number): PanelDial | null =>
  panel.dials.find(dial => dial.index === index) ?? null;
/** Live dial levels arrive as one entry per dial index, so an array has to cover the highest one. */
export const dialSlotCount = (dials: DialPlacement[]): number =>
  dials.reduce((count, dial) => Math.max(count, dial.index + 1), 0);

export const layoutLabel = (layout: GridLayout | null): string =>
  (layout === null ? "Freeform" : `${layout.columns}x${layout.rows}`);
// Discovery names arrive as raw mDNS instance names; the service suffix is noise in the UI.
export const displayName = (name: string): string => name.replace(/\._elg\._tcp\.local\.?/i, "").trim();

export const isPanelCompatible = (device: Device, panel: Panel): boolean => {
  const layout = deviceGridLayout(device.layout);

  return (
    layout !== null
    && layout.columns === panel.layout.columns
    && layout.rows === panel.layout.rows
    && capabilityKeys.every(key => !panel.capabilities[key] || device.capabilities[key])
  );
};

/**
 * The knobs a panel could be turned by. A panel is not bound to a device, so the dials it may
 * declare are those of every device whose grid it fits; capabilities are left out because a
 * capability mismatch does not remove a knob from the hardware.
 */
export const dialsForPanel = (devices: Device[], layout: GridLayout): DialPlacement[] => {
  const placements: DialPlacement[] = [];

  for (const device of devices) {
    const deviceLayout = deviceGridLayout(device.layout);

    if (deviceLayout === null) continue;

    if (deviceLayout.columns !== layout.columns || deviceLayout.rows !== layout.rows) continue;

    for (const dial of device.dials) {
      if (placements.every(existing => existing.index !== dial.index)) placements.push(dial);
    }
  }

  return placements;
};

export type DeviceKind = "studio" | "network_dock";
export const deviceKindLabels: Array<{ value: DeviceKind; label: string; hint: string; }> = [
  { value: "studio", label: "Stream Deck Studio", hint: "16 x 2 keys, connects directly over the network." },
  {
    value: "network_dock",
    label: "Stream Deck Network Dock",
    hint: "Keyless dock - the attached Stream Deck appears as a child device once connected.",
  },
];
export type AddDeviceInput = {
  name: string;
  host: string;
  port?: number;
  serial_number: string | null;
  kind: DeviceKind;
};
export type CreatePanelInput = {
  name: string;
  layout: GridLayout;
  font_family?: string;
  capabilities: Capabilities;
  controls: Control[];
  dials: PanelDial[];
};
export type PanelPayload = Pick<Panel, "name" | "layout" | "font_family" | "capabilities" | "controls" | "dials">;

export const panelPayload = (panel: Panel): PanelPayload => ({
  name: panel.name,
  layout: panel.layout,
  font_family: panel.font_family,
  capabilities: panel.capabilities,
  controls: panel.controls,
  dials: panel.dials,
});

export const fetchInventory = async (): Promise<Inventory> => {
  const response = await fetch("/api/devices");

  if (!response.ok) throw new Error(await getErrorMessage(response));

  const data: unknown = await response.json();

  if (!isInventory(data)) throw new Error("The daemon returned an invalid device inventory.");

  return {
    ...data,
    devices: data.devices.map(device => ({
      ...device,
      parent_surface_id: device.parent_surface_id ?? null,
      open_subpanels: device.open_subpanels ?? [],
      dials: device.dials ?? [],
    })),
    panels: data.panels.map(panel => ({ ...panel, dials: panel.dials ?? [] })),
    plugin_instances: data.plugin_instances ?? [],
    key_states: data.key_states ?? [],
    dial_states: data.dial_states ?? [],
    dial_presses: data.dial_presses ?? [],
    logs: data.logs ?? [],
  };
};

export const addDevice = (input: AddDeviceInput): Promise<Response> => request("/api/devices", "POST", input);
export const addDiscoveredDevice = (discoveryId: string): Promise<Response> =>
  request(`/api/discovered/${encodeURIComponent(discoveryId)}/devices`, "POST");
export const setDeviceEnabled = (surfaceId: string, isEnabled: boolean): Promise<Response> =>
  request(`/api/devices/${encodeURIComponent(surfaceId)}`, "PATCH", { is_enabled: isEnabled });
export const removeDevice = (surfaceId: string): Promise<Response> =>
  request(`/api/devices/${encodeURIComponent(surfaceId)}`, "DELETE");
export const assignActivePanel = (surfaceId: string, panelId: string): Promise<Response> =>
  request(`/api/devices/${encodeURIComponent(surfaceId)}/active-panel`, "PUT", { panel_id: panelId });
export const fetchDevicePresentation = async (surfaceId: string): Promise<SurfacePresentation> => {
  const response = await fetch(`/api/devices/${encodeURIComponent(surfaceId)}/presentation`);

  if (!response.ok) throw new Error(await getErrorMessage(response));

  const data: unknown = await response.json();

  if (!isRecord(data) || !isNumber(data.columns) || !isNumber(data.rows) || !Array.isArray(data.controls)) {
    throw new Error("The daemon returned an invalid device presentation.");
  }

  const controls = data.controls.flatMap((entry) => {
    if (!isRecord(entry) || !isControl(entry.control) || !isNumber(entry.key_index) || typeof entry.is_dimmed !== "boolean") return [];

    return [{ control: entry.control, key_index: entry.key_index, is_dimmed: entry.is_dimmed }];
  });

  if (controls.length !== data.controls.length) throw new Error("The daemon returned an invalid device presentation.");

  return { columns: data.columns, rows: data.rows, controls };
};
export const createPanel = (input: CreatePanelInput): Promise<Response> => request("/api/panels", "POST", input);
export const updatePanel = (panelId: string, payload: PanelPayload): Promise<Response> =>
  request(`/api/panels/${encodeURIComponent(panelId)}`, "PATCH", payload);
export const deletePanel = (panelId: string): Promise<Response> =>
  request(`/api/panels/${encodeURIComponent(panelId)}`, "DELETE");
export const saveConfig = (): Promise<Response> => request("/api/config", "POST");

export const fetchPanelConfig = (panelId: string): Promise<string> =>
  fetchText(`/api/panels/${encodeURIComponent(panelId)}/config`);
export const fetchDeviceConfig = (surfaceId: string): Promise<string> =>
  fetchText(`/api/devices/${encodeURIComponent(surfaceId)}/config`);
export const fetchFullConfig = (): Promise<string> => fetchText("/api/config/export");

export { getErrorMessage, isNumber, isOptionalString, isRecord, isString, request } from "./guards";
