import { Component, createResource, Show } from "solid-js";

import { ColorBinding, RenderedState, RgbaColor } from "../api/inventory";
import { renderedKeyImageUrl, ResolvedState } from "../api/render";
import { useInventory } from "../context/InventoryContext";
import { isReference, parseHex } from "../utils/rendered";
import { interpolateVariables } from "../utils/variables";

export const KeyImage: Component<{ state: RenderedState; }> = (properties) => {
  const store = useInventory();
  // Reading one key of the values store subscribes to that key alone, so a track change repaints
  // the keys showing the title and leaves the other thirty-one untouched.
  const lookup = (reference: string) => store.variables[reference];
  const resolveColor = (binding: ColorBinding | null): RgbaColor | null => {
    if (binding === null) return null;

    if (!isReference(binding)) return binding;

    return parseHex(interpolateVariables(binding, lookup));
  };
  const resolved = (): ResolvedState => ({
    text: properties.state.text === null
      ? null
      : interpolateVariables(properties.state.text, lookup) || null,
    image: properties.state.image === null
      ? null
      : interpolateVariables(properties.state.image, lookup) || null,
    foreground_color: resolveColor(properties.state.foreground_color),
    background_color: resolveColor(properties.state.background_color),
  });
  const renderKey = () => {
    const state = resolved();

    // Depends on the arrival of *this* key's image only. A global counter here would redraw every
    // key on screen each time any picture anywhere finished downloading.
    return JSON.stringify([state, state.image === null ? 0 : store.assetArrivals[state.image] ?? 0]);
  };
  const [url] = createResource(renderKey, () => renderedKeyImageUrl(resolved()));

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
