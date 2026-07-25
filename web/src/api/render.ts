import { RgbaColor } from "./inventory";

type KeyRendering = {
  key_index: number;
  text: string | null;
  icon: null;
  image: string | null;
  progress: null;
  foreground_color: RgbaColor | null;
  background_color: RgbaColor | null;
};

/**
 * Distinct renderings held at once. A key bound to a once-a-second value produces a new rendering
 * every second, and every one of them is an object URL holding a decoded JPEG; without a bound
 * this map is a memory leak that grows for as long as the tab is open.
 */
const CACHE_LIMIT = 256;

const cache = new Map<string, Promise<string>>();

const forget = (body: string): void => {
  const held = cache.get(body);

  cache.delete(body);
  // Revoking releases the blob. Awaiting first matters: an in-flight request would otherwise leak
  // the URL it is about to produce.
  void held?.then(url => URL.revokeObjectURL(url)).catch(() => undefined);
};

/**
 * Drops anything drawn with this asset, so the next render picks up the bytes that just landed.
 * Matched on the parsed body rather than a substring, so a label that happens to contain the URL
 * does not evict unrelated keys.
 */
export const forgetRendersUsing = (asset: string): void => {
  // Deleting the entry currently being visited is defined behaviour for a Map iterator, so this
  // walks safely even though `forget` removes from the map.
  for (const body of cache.keys()) {
    const rendering: unknown = JSON.parse(body);

    if (
      typeof rendering === "object"
      && rendering !== null
      && (rendering as KeyRendering).image === asset
    ) {
      forget(body);
    }
  }
};

export type ResolvedState = {
  text: string | null;
  image: string | null;
  foreground_color: RgbaColor | null;
  background_color: RgbaColor | null;
};

// Renders a key exactly as the daemon draws it for the device (same font, same raster), returning
// an object URL for the JPEG. Cached by rendering, so the same key shown on the stage and in a
// sidebar thumbnail costs one request rather than two.
export const renderedKeyImageUrl = (state: ResolvedState): Promise<string> => {
  const rendering: KeyRendering = {
    key_index: 0,
    text: state.text,
    icon: null,
    image: state.image,
    progress: null,
    foreground_color: state.foreground_color,
    background_color: state.background_color,
  };
  const body = JSON.stringify(rendering);
  const cached = cache.get(body);

  if (cached !== undefined) {
    // Refresh its place in insertion order so what is on screen is not what gets evicted.
    cache.delete(body);
    cache.set(body, cached);

    return cached;
  }

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

  while (cache.size > CACHE_LIMIT) {
    const oldest = cache.keys().next();

    if (oldest.done === true) break;

    forget(oldest.value);
  }

  return request;
};
