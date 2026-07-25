import { Component, createResource, Show } from "solid-js";

import { RenderedState } from "../api/inventory";
import { renderedKeyImageUrl } from "../api/render";

export const KeyImage: Component<{ state: RenderedState; }> = (properties) => {
  const renderKey = () =>
    JSON.stringify([properties.state.text, properties.state.foreground_color, properties.state.background_color]);
  const [url] = createResource(renderKey, () => renderedKeyImageUrl(properties.state));

  return (
    <Show when={url()}>
      {source => (
        <img src={source()} alt="" class="pointer-events-none absolute inset-0 h-full w-full object-cover" />
      )}
    </Show>
  );
};
