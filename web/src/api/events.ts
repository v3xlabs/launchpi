/** Parsers for the frames the daemon pushes over `/api/events`. */

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null;

export type KeyStateEvent = {
  type: "key_state";
  surface_id: string;
  key_index: number;
  is_pressed: boolean;
};
export type DialStateEvent = {
  type: "dial_state";
  surface_id: string;
  dial_index: number;
  level: number;
};
export type DialPressEvent = {
  type: "dial_press";
  surface_id: string;
  dial_index: number;
  is_pressed: boolean;
};

export const asEventFrame = (raw: string): Record<string, unknown> | null => {
  try {
    const parsed: unknown = JSON.parse(raw);

    return isRecord(parsed) ? parsed : null;
  }
  catch {
    return null;
  }
};

export const asKeyStateEvent = (value: Record<string, unknown>): KeyStateEvent | null =>
  (value.type === "key_state"
    && typeof value.surface_id === "string"
    && typeof value.key_index === "number"
    && typeof value.is_pressed === "boolean"
    ? {
        type: "key_state",
        surface_id: value.surface_id,
        key_index: value.key_index,
        is_pressed: value.is_pressed,
      }
    : null);

export const asDialStateEvent = (value: Record<string, unknown>): DialStateEvent | null =>
  (value.type === "dial_state"
    && typeof value.surface_id === "string"
    && typeof value.dial_index === "number"
    && typeof value.level === "number"
    ? {
        type: "dial_state",
        surface_id: value.surface_id,
        dial_index: value.dial_index,
        level: value.level,
      }
    : null);

export type DeviceStatusEvent = {
  type: "device_status";
  surface_id: string;
  status: string;
  last_error: string | null;
};

/** Patched into the existing device row rather than triggering a refetch. */
export const asDeviceStatusEvent = (value: Record<string, unknown>): DeviceStatusEvent | null =>
  (value.type === "device_status"
    && typeof value.surface_id === "string"
    && typeof value.status === "string"
    ? {
        type: "device_status",
        surface_id: value.surface_id,
        status: value.status,
        last_error: typeof value.last_error === "string" ? value.last_error : null,
      }
    : null);

export type AssetReadyEvent = { type: "asset_ready"; asset: string; };

export const asAssetReadyEvent = (value: Record<string, unknown>): AssetReadyEvent | null =>
  (value.type === "asset_ready" && typeof value.asset === "string"
    ? { type: "asset_ready", asset: value.asset }
    : null);

export type VariableChangedEvent = {
  type: "variable_changed";
  integration_id: string;
  name: string;
  rendered: string;
};

export const asVariableChangedEvent = (
  value: Record<string, unknown>,
): VariableChangedEvent | null =>
  (value.type === "variable_changed"
    && typeof value.integration_id === "string"
    && typeof value.name === "string"
    && typeof value.rendered === "string"
    ? {
        type: "variable_changed",
        integration_id: value.integration_id,
        name: value.name,
        rendered: value.rendered,
      }
    : null);

export const asDialPressEvent = (value: Record<string, unknown>): DialPressEvent | null =>
  (value.type === "dial_press"
    && typeof value.surface_id === "string"
    && typeof value.dial_index === "number"
    && typeof value.is_pressed === "boolean"
    ? {
        type: "dial_press",
        surface_id: value.surface_id,
        dial_index: value.dial_index,
        is_pressed: value.is_pressed,
      }
    : null);
