import { RenderedState, RgbaColor } from "../api/inventory";

export const toHex = (color: RgbaColor | null, fallback: string): string =>
  (color === null
    ? fallback
    : `#${[color.red, color.green, color.blue]
      .map(channel => channel.toString(16).padStart(2, "0"))
      .join("")}`);

export const fromHex = (value: string): RgbaColor => ({
  red: Number.parseInt(value.slice(1, 3), 16),
  green: Number.parseInt(value.slice(3, 5), 16),
  blue: Number.parseInt(value.slice(5, 7), 16),
  alpha: 255,
});

export const newState = (isPressed: boolean): RenderedState => ({
  text: null,
  image: null,
  foreground_color: { red: 255, green: 255, blue: 255, alpha: 255 },
  background_color: { red: 30, green: 41, blue: 59, alpha: 255 },
  progress: null,
  is_pressed: isPressed,
});
