import { Component, createResource, Show } from "solid-js";

import { RenderedState } from "../api/inventory";
import { renderedKeyImageUrl } from "../api/render";
import { useInventory } from "../context/InventoryContext";
import { interpolateVariables } from "../utils/variables";

export const KeyImage: Component<{ state: RenderedState; }> = (properties) => {
  const store = useInventory();
  // Resolve against live variables before asking the daemon to draw, so the preview shows the same
  // text the hardware does rather than the binding that produced it.
  const resolved = (): RenderedState => ({
    ...properties.state,
    text: properties.state.text === null
      ? null
      : interpolateVariables(properties.state.text, reference => store.variables[reference]) || null,
  });
  const renderKey = () => {
    const state = resolved();

    return JSON.stringify([state.text, state.foreground_color, state.background_color]);
  };
  const [url] = createResource(renderKey, () => renderedKeyImageUrl(resolved()));

  return (
    <Show when={url()}>
      {source => (
        <img src={source()} alt="" class="pointer-events-none absolute inset-0 h-full w-full object-cover" />
      )}
    </Show>
  );
};
