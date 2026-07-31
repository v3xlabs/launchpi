import { ColorBinding, Layer, LayerKind, RenderedState, RgbaColor } from "../api/inventory";

export const isReference = (color: ColorBinding | null): color is string => typeof color === "string";

export const rgbHex = (color: RgbaColor): string =>
  `#${[color.red, color.green, color.blue]
    .map(channel => channel.toString(16).padStart(2, "0"))
    .join("")}`;

export const toHex = (color: ColorBinding | null, fallback: string): string =>
  (color === null || isReference(color) ? fallback : rgbHex(color));

/** Mirrors the daemon: `#rgb`, `#rrggbb` and `#rrggbbaa`, hash optional. */
export const parseHex = (value: string): RgbaColor | null => {
  const digits = value.trim().replace(/^#/, "");

  if (digits.length === 3) {
    const nibble = (at: number) => Number.parseInt(digits.slice(at, at + 1), 16) * 17;

    if ([0, 1, 2].some(at => Number.isNaN(nibble(at)))) return null;

    return { red: nibble(0), green: nibble(1), blue: nibble(2), alpha: 255 };
  }

  if (digits.length !== 6 && digits.length !== 8) return null;

  const byte = (at: number) => Number.parseInt(digits.slice(at, at + 2), 16);

  if ([0, 2, 4].some(at => Number.isNaN(byte(at)))) return null;

  return {
    red: byte(0),
    green: byte(2),
    blue: byte(4),
    alpha: digits.length === 8 ? byte(6) : 255,
  };
};

export const fromHex = (value: string): RgbaColor => ({
  red: Number.parseInt(value.slice(1, 3), 16),
  green: Number.parseInt(value.slice(3, 5), 16),
  blue: Number.parseInt(value.slice(5, 7), 16),
  alpha: 255,
});

/** Two layers, so the simplest key stays as simple as it was before it had a stack. */
export const newState = (isPressed: boolean): RenderedState => ({
  layers: [
    { kind: "fill", color: { red: 0, green: 0, blue: 0, alpha: 255 } },
    {
      kind: "text",
      text: "",
      color: { red: 255, green: 255, blue: 255, alpha: 255 },
      anchor: "center",
      font_family: undefined,
      font_size: undefined,
    },
  ],
  is_pressed: isPressed,
});

/** A blank layer of each kind, for the inspector's add-layer menu. */
export const newLayer = (kind: LayerKind): Layer => {
  const white: RgbaColor = { red: 255, green: 255, blue: 255, alpha: 255 };

  switch (kind) {
    case "fill": {
      return { kind: "fill", color: { red: 0, green: 0, blue: 0, alpha: 255 } };
    }
    case "image": {
      return { kind: "image", image: "", fit: "cover", anchor: "center", scale_percent: 100, tint: null };
    }
    case "text": {
      return {
        kind: "text",
        text: "",
        color: white,
        anchor: "center",
        font_family: undefined,
        font_size: undefined,
      };
    }
    case "bar": {
      return { kind: "bar", value: 0, maximum: 100, color: white, edge: "bottom", thickness: 6 };
    }
    default: {
      return { kind: "border", color: white, width: 5 };
    }
  }
};
