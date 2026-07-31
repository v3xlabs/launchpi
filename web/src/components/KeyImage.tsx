import { Component, createEffect, createResource, createSignal, onCleanup, Show } from "solid-js";

import { Control } from "../api/inventory";
import { renderedKeyImage } from "../api/render";
import { useInventory } from "../context/InventoryContext";
import { referencesInLayer } from "../utils/variables";

/**
 * Draws a control by asking the daemon to resolve and render it.
 *
 * The browser deliberately does not interpret bindings -- that lives in one place, in the daemon,
 * so the preview and the hardware cannot disagree. All this component works out is *when* to ask
 * again, which needs only the names a state mentions, not what they mean.
 */
export const KeyImage: Component<{ control: Control; isPressed: boolean; }> = (properties) => {
  const store = useInventory();
  const request = () => ({
    default_state: properties.control.default_state,
    pressed_state: properties.control.pressed_state,
    is_pressed: properties.isPressed,
  });
  // Reading each referenced value subscribes to that key alone, so a track change redraws the keys
  // showing the title and leaves the rest untouched. The values themselves are only a change
  // signal here; the daemon is what turns them into pixels.
  const dependencies = () => {
    const state = properties.isPressed
      ? properties.control.pressed_state ?? properties.control.default_state
      : properties.control.default_state;
    const names = state.layers.flatMap(referencesInLayer);

    return names.map(name => [store.variables[name], store.assetArrivals[store.variables[name] ?? ""]]);
  };
  const renderKey = () => JSON.stringify([request(), dependencies()]);
  const [image] = createResource(renderKey, cacheKey => renderedKeyImage(request(), cacheKey));
  const [url, setUrl] = createSignal<string>();
  let activeUrl: string | undefined;

  createEffect(() => {
    const blob = image.latest;

    if (blob === undefined) return;

    const nextUrl = URL.createObjectURL(blob);
    const previousUrl = activeUrl;

    activeUrl = nextUrl;
    setUrl(nextUrl);
    if (previousUrl !== undefined) URL.revokeObjectURL(previousUrl);
  });

  onCleanup(() => {
    if (activeUrl !== undefined) URL.revokeObjectURL(activeUrl);
  });

  return (
    // The previous URL stays mounted until the new blob is ready, so a live value never blanks it.
    <Show when={url()}>
      {source => (
        <img src={source()} alt="" class="pointer-events-none absolute inset-0 h-full w-full object-cover" />
      )}
    </Show>
  );
};
