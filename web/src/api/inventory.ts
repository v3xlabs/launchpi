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
export type RenderedState = {
  text: string | null;
  image: string | null;
  foreground_color: ColorBinding | null;
  background_color: ColorBinding | null;
  progress: unknown | null;
  content_layout: ContentLayout;
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
export type Panel = {
  panel_id: string;
  name: string;
  layout: GridLayout;
  capabilities: Capabilities;
  controls: Control[];
  dial_colors: RgbaColor[];
  dial_ring_levels: number[];
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
const isColorBinding = (value: unknown): value is ColorBinding | null =>
  value === null || isString(value) || isColor(value);
const isGridLayout = (value: unknown): value is GridLayout =>
  isRecord(value) && isNumber(value.columns) && isNumber(value.rows);
const isRenderedState = (value: unknown): value is RenderedState =>
  isRecord(value)
  && isOptionalString(value.text)
  && isOptionalString(value.image)
  && isColorBinding(value.foreground_color)
  && isColorBinding(value.background_color)
  && "progress" in value
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
const isPanel = (value: unknown): value is Panel =>
  isRecord(value)
  && isString(value.panel_id)
  && isString(value.name)
  && isGridLayout(value.layout)
  && isCapabilities(value.capabilities)
  && Array.isArray(value.controls)
  && value.controls.every(isControl)
  && (value.dial_colors === undefined
    || (Array.isArray(value.dial_colors) && value.dial_colors.every(isColor)))
  && (value.dial_ring_levels === undefined
    || (Array.isArray(value.dial_ring_levels) && value.dial_ring_levels.every(isNumber)));
const isDevice = (value: unknown): value is Device =>
  isRecord(value)
  && isString(value.surface_id)
  && isString(value.name)
  && isString(value.host)
  && isNumber(value.port)
  && isOptionalString(value.serial_number)
  && isString(value.model)
  && isCapabilities(value.capabilities)
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

// The Studio is the only surface with rotary dials, and the daemon identifies it by this geometry.
export const studioLayout: GridLayout = { columns: 16, rows: 2 };
export const studioDialCount = 2;
export const isStudioLayout = (layout: GridLayout | null): boolean =>
  layout !== null && layout.columns === studioLayout.columns && layout.rows === studioLayout.rows;
export const panelDialCount = (panel: Panel): number => (isStudioLayout(panel.layout) ? studioDialCount : 0);
export const panelDial = (panel: Panel, index: number): { color: RgbaColor | null; level: number; } => ({
  color: panel.dial_colors[index] ?? null,
  level: panel.dial_ring_levels[index] ?? 100,
});

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
  capabilities: Capabilities;
  controls: Control[];
  dial_colors: RgbaColor[];
  dial_ring_levels: number[];
};
export type PanelPayload = Pick<
  Panel,
    "name" | "layout" | "capabilities" | "controls" | "dial_colors" | "dial_ring_levels"
>;

export const panelPayload = (panel: Panel): PanelPayload => ({
  name: panel.name,
  layout: panel.layout,
  capabilities: panel.capabilities,
  controls: panel.controls,
  dial_colors: panel.dial_colors,
  dial_ring_levels: panel.dial_ring_levels,
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
    })),
    panels: data.panels.map(panel => ({
      ...panel,
      dial_colors: panel.dial_colors ?? [],
      dial_ring_levels: panel.dial_ring_levels ?? [],
    })),
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
