export type DeviceStatus = 'connecting' | 'connected' | 'unavailable' | 'disabled';
export type RgbaColor = { red: number; green: number; blue: number; alpha: number };
export type Capabilities = {
    supports_color: boolean;
    supports_images: boolean;
    supports_text: boolean;
    supports_brightness: boolean;
    supports_haptics: boolean;
};
export type GridLayout = { columns: number; rows: number };
export type RenderedState = {
    text: string | null;
    image: string | null;
    foreground_color: RgbaColor | null;
    background_color: RgbaColor | null;
    progress: unknown | null;
    is_pressed: boolean;
};
export type Control = {
    control_id: string;
    name: string;
    position: { column: number; row: number };
    default_state: RenderedState;
    pressed_state: RenderedState | null;
    action_bindings: unknown[];
    feedback_bindings: unknown[];
};
export type Panel = {
    panel_id: string;
    name: string;
    layout: GridLayout;
    capabilities: Capabilities;
    controls: Control[];
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
    is_enabled: boolean;
    parent_surface_id: string | null;
    status: DeviceStatus;
    last_error: string | null;
};
export type DiscoveredDevice = {
    discovery_id: string;
    name: string;
    host: string;
    port: number;
    serial_number: string | null;
    model: string;
};
export type KeyEvent = { surface_id: string; key_index: number; is_pressed: boolean };
export type Inventory = {
    discovered: DiscoveredDevice[];
    devices: Device[];
    panels: Panel[];
    recent_key_events: KeyEvent[];
    key_states: KeyEvent[];
};

export const capabilityKeys: Array<keyof Capabilities> = [
    'supports_color',
    'supports_images',
    'supports_text',
    'supports_brightness',
    'supports_haptics',
];
export const capabilityLabels: Array<{ key: keyof Capabilities; label: string }> = [
    { key: 'supports_color', label: 'Color' },
    { key: 'supports_images', label: 'Images' },
    { key: 'supports_text', label: 'Text' },
    { key: 'supports_brightness', label: 'Brightness' },
    { key: 'supports_haptics', label: 'Haptics' },
];

export const emptyCapabilities: Capabilities = {
    supports_color: false,
    supports_images: false,
    supports_text: true,
    supports_brightness: false,
    supports_haptics: false,
};
export const emptyInventory: Inventory = {
    discovered: [],
    devices: [],
    panels: [],
    recent_key_events: [],
    key_states: [],
};

const isRecord = (value: unknown): value is Record<string, unknown> =>
    typeof value === 'object' && value !== null;
const isString = (value: unknown): value is string => typeof value === 'string';
const isOptionalString = (value: unknown): value is string | null => value === null || isString(value);
const isNumber = (value: unknown): value is number => typeof value === 'number' && Number.isFinite(value);
const isCapabilities = (value: unknown): value is Capabilities =>
    isRecord(value) && capabilityKeys.every((key) => typeof value[key] === 'boolean');
const isColor = (value: unknown): value is RgbaColor =>
    isRecord(value) &&
    isNumber(value.red) &&
    isNumber(value.green) &&
    isNumber(value.blue) &&
    isNumber(value.alpha);
const isGridLayout = (value: unknown): value is GridLayout =>
    isRecord(value) && isNumber(value.columns) && isNumber(value.rows);
const isRenderedState = (value: unknown): value is RenderedState =>
    isRecord(value) &&
    isOptionalString(value.text) &&
    isOptionalString(value.image) &&
    (value.foreground_color === null || isColor(value.foreground_color)) &&
    (value.background_color === null || isColor(value.background_color)) &&
    'progress' in value &&
    typeof value.is_pressed === 'boolean';
const isControl = (value: unknown): value is Control =>
    isRecord(value) &&
    isString(value.control_id) &&
    isString(value.name) &&
    isRecord(value.position) &&
    isNumber(value.position.column) &&
    isNumber(value.position.row) &&
    isRenderedState(value.default_state) &&
    (value.pressed_state === null || isRenderedState(value.pressed_state)) &&
    Array.isArray(value.action_bindings) &&
    Array.isArray(value.feedback_bindings);
const isPanel = (value: unknown): value is Panel =>
    isRecord(value) &&
    isString(value.panel_id) &&
    isString(value.name) &&
    isGridLayout(value.layout) &&
    isCapabilities(value.capabilities) &&
    Array.isArray(value.controls) &&
    value.controls.every(isControl);
const isDevice = (value: unknown): value is Device =>
    isRecord(value) &&
    isString(value.surface_id) &&
    isString(value.name) &&
    isString(value.host) &&
    isNumber(value.port) &&
    isOptionalString(value.serial_number) &&
    isString(value.model) &&
    isCapabilities(value.capabilities) &&
    isOptionalString(value.active_panel_id) &&
    typeof value.is_enabled === 'boolean' &&
    (value.parent_surface_id === undefined || isOptionalString(value.parent_surface_id)) &&
    ['connecting', 'connected', 'unavailable', 'disabled'].includes(String(value.status)) &&
    isOptionalString(value.last_error);
const isDiscoveredDevice = (value: unknown): value is DiscoveredDevice =>
    isRecord(value) &&
    isString(value.discovery_id) &&
    isString(value.name) &&
    isString(value.host) &&
    isNumber(value.port) &&
    isOptionalString(value.serial_number) &&
    isString(value.model);
const isKeyEvent = (value: unknown): value is KeyEvent =>
    isRecord(value) &&
    isString(value.surface_id) &&
    isNumber(value.key_index) &&
    typeof value.is_pressed === 'boolean';
const isInventory = (value: unknown): value is Inventory =>
    isRecord(value) &&
    Array.isArray(value.discovered) &&
    value.discovered.every(isDiscoveredDevice) &&
    Array.isArray(value.devices) &&
    value.devices.every(isDevice) &&
    Array.isArray(value.panels) &&
    value.panels.every(isPanel) &&
    Array.isArray(value.recent_key_events) &&
    value.recent_key_events.every(isKeyEvent) &&
    (value.key_states === undefined ||
        (Array.isArray(value.key_states) && value.key_states.every(isKeyEvent)));

export const deviceGridLayout = (value: unknown): GridLayout | null => {
    if (isGridLayout(value)) return value;
    if (!isRecord(value)) return null;
    if (isGridLayout(value.Grid)) return value.Grid;
    if (isGridLayout(value.grid)) return value.grid;
    return null;
};

export const isPanelCompatible = (device: Device, panel: Panel): boolean => {
    const layout = deviceGridLayout(device.layout);
    return (
        layout !== null &&
        layout.columns === panel.layout.columns &&
        layout.rows === panel.layout.rows &&
        capabilityKeys.every((key) => !panel.capabilities[key] || device.capabilities[key])
    );
};

const getErrorMessage = async (response: Response): Promise<string> => {
    const body: unknown = await response.json().catch(() => null);
    return isRecord(body) && isString(body.error) ? body.error : `Request failed with status ${response.status}`;
};

const request = async (
    path: string,
    method: 'POST' | 'PATCH' | 'PUT' | 'DELETE',
    body?: unknown,
): Promise<Response> => {
    const response = await fetch(path, {
        method,
        headers: body === undefined ? undefined : { 'content-type': 'application/json' },
        body: body === undefined ? undefined : JSON.stringify(body),
    });
    if (!response.ok) throw new Error(await getErrorMessage(response));
    return response;
};

export type DeviceKind = 'studio' | 'network_dock';
export const deviceKindLabels: Array<{ value: DeviceKind; label: string; hint: string }> = [
    { value: 'studio', label: 'Stream Deck Studio', hint: '16 × 2 keys, connects directly over the network.' },
    {
        value: 'network_dock',
        label: 'Stream Deck Network Dock',
        hint: 'Keyless dock — the attached Stream Deck appears as a child device once connected.',
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
};
export type PanelPayload = Pick<Panel, 'name' | 'layout' | 'capabilities' | 'controls'>;

export const panelPayload = (panel: Panel): PanelPayload => ({
    name: panel.name,
    layout: panel.layout,
    capabilities: panel.capabilities,
    controls: panel.controls,
});

export const fetchInventory = async (): Promise<Inventory> => {
    const response = await fetch('/api/devices');
    if (!response.ok) throw new Error(await getErrorMessage(response));
    const data: unknown = await response.json();
    if (!isInventory(data)) throw new Error('The daemon returned an invalid device inventory.');
    return { ...data, key_states: data.key_states ?? [] };
};

export const addDevice = (input: AddDeviceInput): Promise<Response> => request('/api/devices', 'POST', input);
export const addDiscoveredDevice = (discoveryId: string): Promise<Response> =>
    request(`/api/discovered/${encodeURIComponent(discoveryId)}/devices`, 'POST');
export const setDeviceEnabled = (surfaceId: string, isEnabled: boolean): Promise<Response> =>
    request(`/api/devices/${encodeURIComponent(surfaceId)}`, 'PATCH', { is_enabled: isEnabled });
export const removeDevice = (surfaceId: string): Promise<Response> =>
    request(`/api/devices/${encodeURIComponent(surfaceId)}`, 'DELETE');
export const assignActivePanel = (surfaceId: string, panelId: string): Promise<Response> =>
    request(`/api/devices/${encodeURIComponent(surfaceId)}/active-panel`, 'PUT', { panel_id: panelId });
export const createPanel = (input: CreatePanelInput): Promise<Response> => request('/api/panels', 'POST', input);
export const updatePanel = (panelId: string, payload: PanelPayload): Promise<Response> =>
    request(`/api/panels/${encodeURIComponent(panelId)}`, 'PATCH', payload);
export const saveConfiguration = (): Promise<Response> => request('/api/config', 'POST');
export const fetchPanelConfiguration = async (panelId: string): Promise<string> => {
    const response = await fetch(`/api/panels/${encodeURIComponent(panelId)}/config`);
    if (!response.ok) throw new Error(await getErrorMessage(response));
    return response.text();
};
