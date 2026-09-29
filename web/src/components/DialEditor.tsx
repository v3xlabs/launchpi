import { FiPlus, FiTrash2 } from "solid-icons/fi";
import { Component, For, Show } from "solid-js";

import { DialPlacement, GridLayout, Panel, PanelDial, RgbaColor } from "../api/inventory";
import { toHex } from "../utils/rendered";
import { DialIndicator } from "./DialIndicator";
import { ColorField } from "./fields";
import { MetaSeparator } from "./PageHeader";

export const newDialColor: RgbaColor = { red: 30, green: 41, blue: 59, alpha: 255 };

const freeDialIndex = (panel: Panel, dials: DialPlacement[]): number | undefined =>
  dials.find(placement => panel.dials.every(dial => dial.index !== placement.index))?.index;

/** Names the knob by where it sits relative to the keys, which is how it reads on the hardware. */
export const dialSide = (placement: DialPlacement, layout: GridLayout): string => {
  if (placement.column < 0) return "Left";

  if (placement.column >= layout.columns) return "Right";

  if (placement.row < 0) return "Above";

  if (placement.row >= layout.rows) return "Below";

  return `Column ${placement.column + 1}`;
};

export const DialsField: Component<{
  panel: Panel;
  dials: DialPlacement[];
  onMutate: (mutate: (panel: Panel) => void) => void;
}> = properties => (
  <div class="grid gap-1.5">
    <span class="field-label">Dials</span>
    <Show when={properties.panel.dials.length > 0}>
      <div class="rows">
        <For each={properties.panel.dials}>
          {dial => (
            <div class="flex items-center gap-3 py-1">
              <span class="min-w-0 flex-1">
                Dial
                {" "}
                <span class="tabular-nums">{dial.index + 1}</span>
              </span>
              <span class="mono">{toHex(dial.color, "unset")}</span>
              <button
                type="button"
                class="danger-button"
                aria-label={`Remove dial ${dial.index + 1}`}
                title="Remove dial"
                onClick={() =>
                  properties.onMutate((panel) => {
                    panel.dials = panel.dials.filter(entry => entry.index !== dial.index);
                  })}
              >
                <FiTrash2 class="size-4" />
              </button>
            </div>
          )}
        </For>
      </div>
    </Show>
    <button
      type="button"
      class="secondary-button justify-self-start"
      disabled={freeDialIndex(properties.panel, properties.dials) === undefined}
      onClick={() =>
        properties.onMutate((panel) => {
          const index = freeDialIndex(panel, properties.dials);

          if (index !== undefined) panel.dials.push({ index, level: 100, color: newDialColor });
        })}
    >
      <FiPlus class="size-4" />
      Add dial
    </button>
  </div>
);

export const DialEditor: Component<{
  dial: PanelDial;
  placement: DialPlacement | undefined;
  layout: GridLayout;
  onColorChange: (index: number, color: RgbaColor) => void;
  onLevelChange: (index: number, level: number) => void;
}> = properties => (
  <>
    <div class="flex items-center gap-3 border-b border-hairline px-4 py-3">
      <span class="font-semibold">
        Dial
        {" "}
        <span class="tabular-nums">{properties.dial.index + 1}</span>
      </span>
      <Show when={properties.placement}>
        {placement => <span class="text-muted">{dialSide(placement(), properties.layout)}</span>}
      </Show>
    </div>
    <div class="grid max-w-md gap-4 px-4 py-4">
      <div class="flex items-center gap-4">
        <div class="w-14 shrink-0">
          <DialIndicator
            index={properties.dial.index}
            color={properties.dial.color}
            level={properties.dial.level}
          />
        </div>
        <p class="mono flex items-center gap-2">
          {toHex(properties.dial.color, "unset")}
          <MetaSeparator />
          <span>
            {properties.dial.level}
            %
          </span>
        </p>
      </div>
      <ColorField
        label="Colour"
        value={properties.dial.color}
        fallback="#1e293b"
        bindable={false}
        onChange={(color) => {
          if (typeof color !== "string") properties.onColorChange(properties.dial.index, color);
        }}
      />
      <label class="field-label">
        <span class="flex items-center justify-between gap-2">
          Ring level
          <span class="font-normal text-muted tabular-nums">
            {properties.dial.level}
            %
          </span>
        </span>
        <input
          class="range-input"
          type="range"
          min="0"
          max="100"
          value={properties.dial.level}
          onInput={event =>
            properties.onLevelChange(properties.dial.index, Number(event.currentTarget.value))}
        />
      </label>
    </div>
  </>
);
