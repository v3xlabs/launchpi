import { Component, createResource, Show } from "solid-js";

import { Control } from "../api/inventory";
import { renderedKeyImageUrl } from "../api/render";
import { useInventory } from "../context/InventoryContext";
import { referencesIn } from "../utils/variables";

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
    const names = [
      ...referencesIn(state.text),
      ...referencesIn(state.image),
      ...referencesIn(state.overlay_image?.image ?? null),
      ...referencesIn(typeof state.foreground_color === "string" ? state.foreground_color : null),
      ...referencesIn(typeof state.background_color === "string" ? state.background_color : null),
      ...referencesIn(typeof state.border?.color === "string" ? state.border.color : null),
    ];

    return names.map(name => [store.variables[name], store.assetArrivals[store.variables[name] ?? ""]]);
  };
  const renderKey = () => JSON.stringify([request(), dependencies()]);
  const [url] = createResource(renderKey, () => renderedKeyImageUrl(request()));

  return (
    // `latest` keeps the previous frame on screen while the next one renders. Reading `url()`
    // instead would blank the key on every change, which is the flicker.
    <Show when={url.latest}>
      {source => (
        <img src={source()} alt="" class="pointer-events-none absolute inset-0 h-full w-full object-cover" />
      )}
    </Show>
  );
};
