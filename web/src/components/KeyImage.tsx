import { Component, createResource, Show } from "solid-js";

import { ColorBinding, RenderedState, RgbaColor } from "../api/inventory";
import { renderedKeyImageUrl, ResolvedState } from "../api/render";
import { useInventory } from "../context/InventoryContext";
import { isReference, parseHex } from "../utils/rendered";
import { interpolateVariables } from "../utils/variables";

export const KeyImage: Component<{ state: RenderedState; }> = (properties) => {
  const store = useInventory();
  const lookup = (reference: string) => store.variables[reference];
  // Resolve against live values before asking the daemon to draw, so the preview shows the same
  // thing the hardware does rather than the bindings that produced it.
  const resolveColor = (binding: ColorBinding | null): RgbaColor | null => {
    if (binding === null) return null;

    if (!isReference(binding)) return binding;

    return parseHex(interpolateVariables(binding, lookup));
  };
  const resolved = (): ResolvedState => ({
    text: properties.state.text === null
      ? null
      : interpolateVariables(properties.state.text, lookup) || null,
    foreground_color: resolveColor(properties.state.foreground_color),
    background_color: resolveColor(properties.state.background_color),
  });
  const renderKey = () => JSON.stringify(resolved());
  const [url] = createResource(renderKey, () => renderedKeyImageUrl(resolved()));

  return (
    <Show when={url()}>
      {source => (
        <img src={source()} alt="" class="pointer-events-none absolute inset-0 h-full w-full object-cover" />
      )}
    </Show>
  );
};
