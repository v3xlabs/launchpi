import { Component, createResource, createSignal, For, Show } from "solid-js";

import { fetchIcons } from "../api/icons";
import { TextField } from "./fields";

/**
 * Browses the icon pack the daemon holds.
 *
 * The daemon serves each glyph as SVG rather than the browser carrying a second copy of 7447
 * icons, and the glyph uses `currentColor`, so the swatch here is the same drawing the key gets.
 */
export const IconPicker: Component<{ onChoose: (icon: string) => void; }> = (properties) => {
  const [search, setSearch] = createSignal("");
  const [icons] = createResource(search, term => fetchIcons(term));

  const found = () => icons.latest ?? [];

  return (
    <div class="icon-picker">
      <TextField
        label="Search icons"
        value={search()}
        placeholder="lightbulb, volume, play..."
        onChange={setSearch}
      />
      <Show
        when={found().length > 0}
        fallback={<p class="hint">{icons.loading ? "Looking..." : "No icon matches that."}</p>}
      >
        <div class="icon-grid">
          <For each={found()}>
            {icon => (
              <button
                type="button"
                class="icon-tile"
                title={icon}
                onClick={() => properties.onChoose(icon)}
              >
                <img src={`/api/icons/${encodeURIComponent(icon)}`} alt="" />
              </button>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
};
