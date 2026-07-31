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
 * every second. The cache holds blobs rather than object URLs, so evicting an old render cannot
 * invalidate an image another mounted component is still showing.
 */
const CACHE_LIMIT = 256;

const cache = new Map<string, Promise<Blob>>();

/**
 * Drops anything that might have been drawn with this asset. The browser no longer resolves
 * bindings, so it cannot know which entries used it; clearing on a substring of the raw binding
 * would be wrong, and clearing everything is both correct and rare - once per newly-seen image.
 */
export const forgetRendersUsing = (): void => {
  cache.clear();
};

export const renderedKeyImage = (request: RenderRequest, cacheKey: string): Promise<Blob> => {
  const body = JSON.stringify(request);
  const cached = cache.get(cacheKey);

  if (cached !== undefined) {
    // Refresh its place in insertion order so what is on screen is not what gets evicted.
    cache.delete(cacheKey);
    cache.set(cacheKey, cached);

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
    });

  cache.set(cacheKey, pending);

  while (cache.size > CACHE_LIMIT) {
    const oldest = cache.keys().next();

    if (oldest.done === true) break;

    cache.delete(oldest.value);
  }

  return pending;
};
