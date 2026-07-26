import { TbFillTrash as TbTrash } from "solid-icons/tb";
import { Component, For, Show } from "solid-js";

import { DialPlacement, GridLayout, Panel, PanelDial, RgbaColor } from "../api/inventory";
import { toHex } from "../utils/rendered";
import { DialIndicator, litRingSegments, totalRingSegments } from "./DialIndicator";
import { ColorField } from "./fields";

export const newDialColor: RgbaColor = { red: 30, green: 41, blue: 59, alpha: 255 };

const freeDialIndex = (panel: Panel, dials: DialPlacement[]): number | undefined =>
  dials.find(placement => panel.dials.every(dial => dial.index !== placement.index))?.index;

/** Names the knob by where it sits relative to the keys, which is how it reads on the hardware. */
export const dialSide = (placement: DialPlacement, layout: GridLayout): string => {
  if (placement.column < 0) return "left";

  if (placement.column >= layout.columns) return "right";

  if (placement.row < 0) return "above";

  if (placement.row >= layout.rows) return "below";

  return `column ${placement.column + 1}`;
};

export const DialsField: Component<{
  panel: Panel;
  dials: DialPlacement[];
  onMutate: (mutate: (panel: Panel) => void) => void;
}> = properties => (
  <fieldset class="grid gap-1.5">
    <legend class="field-label">Dials</legend>
    <For each={properties.panel.dials}>
      {dial => (
        <div class="flex items-center gap-1.5">
          <span class="check-tile min-w-0 flex-1">
            Dial
            {" "}
            {dial.index + 1}
            <span class="mono ml-auto">{toHex(dial.color, "unset")}</span>
          </span>
          <button
            type="button"
            class="danger-button"
            aria-label={`Remove dial ${dial.index + 1}`}
            onClick={() =>
              properties.onMutate((panel) => {
                panel.dials = panel.dials.filter(entry => entry.index !== dial.index);
              })}
          >
            <TbTrash class="h-3.5 w-3.5" />
          </button>
        </div>
      )}
    </For>
    <button
      type="button"
      class="secondary-button"
      disabled={freeDialIndex(properties.panel, properties.dials) === undefined}
      onClick={() =>
        properties.onMutate((panel) => {
          const index = freeDialIndex(panel, properties.dials);

          if (index !== undefined) panel.dials.push({ index, level: 100, color: newDialColor });
        })}
    >
      Add dial
    </button>
  </fieldset>
);

export const DialEditor: Component<{
  dial: PanelDial;
  placement: DialPlacement | undefined;
  layout: GridLayout;
  onColorChange: (index: number, color: RgbaColor) => void;
  onLevelChange: (index: number, level: number) => void;
}> = properties => (
  <>
    <div class="card-head">
      <p class="card-title">
        Dial
        {" "}
        {properties.dial.index + 1}
      </p>
      <Show when={properties.placement}>
        {placement => <span class="chip chip-muted">{dialSide(placement(), properties.layout)}</span>}
      </Show>
    </div>
    <div class="card-body">
      <div class="flex items-center gap-4 bg-neutral-950 p-3">
        <div class="w-14 shrink-0">
          <DialIndicator
            index={properties.dial.index}
            color={properties.dial.color}
            level={properties.dial.level}
          />
        </div>
        <p class="mono">
          {toHex(properties.dial.color, "unset")}
          {" - "}
          {properties.dial.level}
          % -
          {" "}
          {litRingSegments(properties.dial.level)}
          /
          {totalRingSegments}
          {" "}
          segments
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
        Ring level -
        {" "}
        {properties.dial.level}
        %
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
