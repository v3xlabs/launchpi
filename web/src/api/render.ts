import { RenderedState } from "./inventory";

/**
 * What the daemon needs to draw a key: the control's state with its bindings still intact. The
 * daemon resolves them, through the same code that feeds the hardware, so the preview cannot drift
 * from the device -- and an unsaved draft still renders, because the browser sends what it has
 * rather than naming something the daemon would have to look up.
 */
export type RenderRequest = {
  default_state: RenderedState;
  pressed_state: RenderedState | null;
  is_pressed: boolean;
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
 * Drops anything that might have been drawn with this asset. The browser no longer resolves
 * bindings, so it cannot know which entries used it; clearing on a substring of the raw binding
 * would be wrong, and clearing everything is both correct and rare - once per newly-seen image.
 */
export const forgetRendersUsing = (): void => {
  for (const body of cache.keys()) forget(body);
};

export const renderedKeyImageUrl = (request: RenderRequest): Promise<string> => {
  const body = JSON.stringify(request);
  const cached = cache.get(body);

  if (cached !== undefined) {
    // Refresh its place in insertion order so what is on screen is not what gets evicted.
    cache.delete(body);
    cache.set(body, cached);

    return cached;
  }

  const pending = fetch("/api/render-key", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body,
  })
    .then((response) => {
      if (!response.ok) throw new Error("Unable to render key image.");

      return response.blob();
    })
    .then(blob => URL.createObjectURL(blob));

  cache.set(body, pending);

  while (cache.size > CACHE_LIMIT) {
    const oldest = cache.keys().next();

    if (oldest.done === true) break;

    forget(oldest.value);
  }

  return pending;
};
