import { RenderedState, RgbaColor } from "./inventory";

type KeyRendering = {
  key_index: number;
  text: string | null;
  icon: null;
  foreground_color: RgbaColor | null;
  background_color: RgbaColor | null;
};

const cache = new Map<string, Promise<string>>();

// Renders a key exactly as the daemon draws it for the device (same ab_glyph font, same raster),
// returning an object URL for the JPEG. Cached by rendering so identical keys share one request.
export const renderedKeyImageUrl = (state: RenderedState): Promise<string> => {
  const rendering: KeyRendering = {
    key_index: 0,
    text: state.text,
    icon: null,
    foreground_color: state.foreground_color,
    background_color: state.background_color,
  };
  const body = JSON.stringify(rendering);
  const cached = cache.get(body);

  if (cached !== undefined) return cached;

  const request = fetch("/api/render-key", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body,
  })
    .then((response) => {
      if (!response.ok) throw new Error("Unable to render key image.");

      return response.blob();
    })
    .then(blob => URL.createObjectURL(blob));

  cache.set(body, request);

  return request;
};
