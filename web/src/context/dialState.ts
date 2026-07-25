import { Device, DialPress, dialSlotCount, DialState } from "../api/inventory";

/**
 * Live dial levels keyed by surface, then by dial index. A missing entry means the dial still sits
 * wherever its panel configured it.
 */
export type DialLevels = Record<string, Record<string, number>>;

export const groupDialLevels = (dialStates: DialState[]): DialLevels => {
  const grouped: DialLevels = {};

  for (const dial of dialStates) {
    (grouped[dial.surface_id] ??= {})[String(dial.dial_index)] = dial.level;
  }

  return grouped;
};

export const groupPressedDials = (dialPresses: DialPress[]): Record<string, number[]> => {
  const grouped: Record<string, number[]> = {};

  for (const dial of dialPresses) {
    if (!dial.is_pressed) continue;

    (grouped[dial.surface_id] ??= []).push(dial.dial_index);
  }

  return grouped;
};

/**
 * The levels of the given devices' dials, indexed by dial index, `null` where nothing has turned
 * since the panel loaded. The devices' own models decide how many dials there are to report.
 */
export const dialLevelsOf = (levels: DialLevels, devices: Device[]): Array<number | null> => {
  const slots = devices.reduce((count, device) => Math.max(count, dialSlotCount(device.dials)), 0);

  return Array.from({ length: slots }, (_, index) => {
    for (const device of devices) {
      const level = levels[device.surface_id]?.[String(index)];

      if (level !== undefined) return level;
    }

    return null;
  });
};
