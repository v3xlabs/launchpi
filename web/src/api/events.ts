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
