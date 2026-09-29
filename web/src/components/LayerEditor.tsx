import * as DropdownMenu from "@kobalte/core/dropdown-menu";
import { FiArrowDown, FiArrowUp, FiChevronDown, FiPlus, FiTrash2 } from "solid-icons/fi";
import { Component, createSignal, For, Match, Show, Switch } from "solid-js";

import { Anchor9, ColorBinding, Edge, Fit, Layer, LayerKind, ValueBinding } from "../api/inventory";
import { isReference, newLayer } from "../utils/rendered";
import { ColorField, FontFamilyField, NumberField, SelectField } from "./fields";
import { IconPicker } from "./IconPicker";
import { MenuItem } from "./Menu";
import { ReferenceInput } from "./ReferenceInput";
import { ValueField } from "./ValueField";

const anchors: Anchor9[] = [
  "top_start", "top_center", "top_end", "center_start", "center", "center_end", "bottom_start", "bottom_center", "bottom_end",
];

const anchorLabels: Record<Anchor9, string> = {
  top_start: "Left top",
  top_center: "Middle top",
  top_end: "Right top",
  center_start: "Left middle",
  center: "Middle middle",
  center_end: "Right middle",
  bottom_start: "Left bottom",
  bottom_center: "Middle bottom",
  bottom_end: "Right bottom",
};

/**
 * The nine anchors as the nine places they mean, rather than as a list of names. A grid says where
 * a thing will sit without the reader translating "bottom_end" into a corner first.
 */
const AnchorField: Component<{
  label: string;
  value: Anchor9;
  onChange: (anchor: Anchor9) => void;
}> = properties => (
  <div class="grid gap-1.5">
    <span class="field-label">{properties.label}</span>
    <div class="anchor-grid" role="group" aria-label={properties.label}>
      <For each={anchors}>
        {anchor => (
          <button
            type="button"
            class="anchor-cell"
            title={anchorLabels[anchor]}
            aria-label={anchorLabels[anchor]}
            aria-pressed={properties.value === anchor}
            data-selected={properties.value === anchor}
            onClick={() => properties.onChange(anchor)}
          />
        )}
      </For>
    </div>
  </div>
);

const fitOptions: Array<{ value: Fit; label: string; }> = [
  { value: "cover", label: "Cover the key" },
  { value: "contain", label: "Fit inside" },
];

const edgeOptions: Array<{ value: Edge; label: string; }> = [
  { value: "bottom", label: "Bottom" }, { value: "top", label: "Top" },
  { value: "start", label: "Left" }, { value: "end", label: "Right" },
];

const kinds: Array<{ kind: LayerKind; label: string; }> = [
  { kind: "fill", label: "Fill" },
  { kind: "image", label: "Image" },
  { kind: "text", label: "Text" },
  { kind: "bar", label: "Bar" },
  { kind: "border", label: "Border" },
];

const labelFor = (kind: LayerKind): string =>
  kinds.find(entry => entry.kind === kind)?.label ?? kind;

const toCount = (value: string): number | undefined => {
  const parsed = Number(value);

  return value.trim() === "" || Number.isNaN(parsed) ? undefined : parsed;
};

/** A number stays a number; anything else is a reference the daemon resolves. */
const toValueBinding = (value: string): ValueBinding => toCount(value) ?? value;

/** A fill below full opacity is a scrim: it darkens what is under it rather than hiding it. */
const opacityOf = (color: ColorBinding): number | null =>
  (isReference(color) ? null : Math.round((color.alpha / 255) * 100));

/**
 * One layer's own fields. Every layer binds its colour and its content, so each editor is the same
 * two ideas with a different vocabulary.
 */
const LayerFields: Component<{
  layer: Layer;
  onMutate: (mutate: (layer: Layer) => void) => void;
}> = (properties) => {
  const [isBrowsing, setIsBrowsing] = createSignal(false);

  return (
    <Switch>
      <Match when={properties.layer.kind === "fill" ? properties.layer : null}>
        {fill => (
          <>
            <ColorField
              label="Colour"
              value={fill().color}
              fallback="#000000"
              onChange={color =>
                properties.onMutate((layer) => {
                  if (layer.kind === "fill") layer.color = color;
                })}
            />
            <NumberField
              label="Opacity"
              value={opacityOf(fill().color)}
              onChange={value =>
                properties.onMutate((layer) => {
                  const percent = toCount(value);

                  if (percent === undefined || layer.kind !== "fill") return;

                  if (isReference(layer.color)) return;

                  layer.color = {
                    ...layer.color,
                    alpha: Math.round((Math.min(Math.max(percent, 0), 100) * 255) / 100),
                  };
                })}
            />
          </>
        )}
      </Match>

      <Match when={properties.layer.kind === "border" ? properties.layer : null}>
        {border => (
          <>
            <ColorField
              label="Colour"
              value={border().color}
              fallback="#ffffff"
              onChange={color =>
                properties.onMutate((layer) => {
                  if (layer.kind === "border") layer.color = color;
                })}
            />
            <NumberField
              label="Width"
              value={border().width}
              onChange={value =>
                properties.onMutate((layer) => {
                  const width = toCount(value);

                  if (width !== undefined && layer.kind === "border") layer.width = width;
                })}
            />
          </>
        )}
      </Match>

      <Match when={properties.layer.kind === "text" ? properties.layer : null}>
        {text => (
          <>
            <label class="col-span-2 grid min-w-0">
              <span class="sr-only">Text</span>
              <ReferenceInput
                value={text().text}
                placeholder="Shown on the key"
                onChange={value =>
                  properties.onMutate((layer) => {
                    if (layer.kind === "text") layer.text = value;
                  })}
              />
            </label>
            <ColorField
              label="Colour"
              value={text().color}
              fallback="#ffffff"
              onChange={color =>
                properties.onMutate((layer) => {
                  if (layer.kind === "text") layer.color = color;
                })}
            />
            <AnchorField
              label="Position"
              value={text().anchor}
              onChange={anchor =>
                properties.onMutate((layer) => {
                  if (layer.kind === "text") layer.anchor = anchor;
                })}
            />
            <FontFamilyField
              label="Font"
              value={text().font_family ?? ""}
              placeholder="Panel font"
              onChange={value =>
                properties.onMutate((layer) => {
                  if (layer.kind === "text") layer.font_family = value.trim() || undefined;
                })}
            />
            <NumberField
              label="Font size"
              value={text().font_size ?? null}
              placeholder="Auto"
              onChange={value =>
                properties.onMutate((layer) => {
                  if (layer.kind !== "text") return;

                  const fontSize = toCount(value);

                  layer.font_size = fontSize === undefined
                    ? undefined
                    : Math.round(Math.min(Math.max(fontSize, 8), 72));
                })}
            />
          </>
        )}
      </Match>

      <Match when={properties.layer.kind === "image" ? properties.layer : null}>
        {image => (
          <>
            <div class="col-span-2 flex min-w-0 items-start gap-2">
              <label class="grid min-w-0 flex-1">
                <span class="sr-only">Image</span>
                <ReferenceInput
                  value={image().image}
                  placeholder="mdi:lightbulb, a URL, or $(mpris.default:art_url)"
                  onChange={value =>
                    properties.onMutate((layer) => {
                      if (layer.kind === "image") layer.image = value;
                    })}
                />
              </label>
              <button
                type="button"
                class="secondary-button"
                aria-expanded={isBrowsing()}
                onClick={() => setIsBrowsing(current => !current)}
              >
                Browse
              </button>
            </div>
            <Show when={isBrowsing()}>
              <IconPicker
                onChoose={(icon) => {
                  setIsBrowsing(false);
                  properties.onMutate((layer) => {
                    if (layer.kind === "image") layer.image = icon;
                  });
                }}
              />
            </Show>
            <SelectField
              label="Fit"
              value={image().fit}
              options={fitOptions}
              onChange={value =>
                properties.onMutate((layer) => {
                  const fit = fitOptions.find(option => option.value === value)?.value;

                  if (fit !== undefined && layer.kind === "image") layer.fit = fit;
                })}
            />
            <NumberField
              label="Size"
              value={image().scale_percent}
              onChange={value =>
                properties.onMutate((layer) => {
                  const scale = toCount(value);

                  if (scale !== undefined && layer.kind === "image") layer.scale_percent = scale;
                })}
            />
            <AnchorField
              label="Position"
              value={image().anchor}
              onChange={anchor =>
                properties.onMutate((layer) => {
                  if (layer.kind === "image") layer.anchor = anchor;
                })}
            />
            <ColorField
              label="Tint"
              value={image().tint}
              fallback="#ffffff"
              onChange={color =>
                properties.onMutate((layer) => {
                  if (layer.kind === "image") layer.tint = color;
                })}
            />
          </>
        )}
      </Match>

      <Match when={properties.layer.kind === "bar" ? properties.layer : null}>
        {bar => (
          <>
            <ValueField
              label="Value"
              value={String(bar().value)}
              onChange={value =>
                properties.onMutate((layer) => {
                  if (layer.kind === "bar") layer.value = toValueBinding(value);
                })}
            />
            <ValueField
              label="Maximum"
              value={String(bar().maximum)}
              onChange={value =>
                properties.onMutate((layer) => {
                  if (layer.kind === "bar") layer.maximum = toValueBinding(value);
                })}
            />
            <ColorField
              label="Colour"
              value={bar().color}
              fallback="#ffffff"
              onChange={color =>
                properties.onMutate((layer) => {
                  if (layer.kind === "bar") layer.color = color;
                })}
            />
            <SelectField
              label="Edge"
              value={bar().edge}
              options={edgeOptions}
              onChange={value =>
                properties.onMutate((layer) => {
                  const edge = edgeOptions.find(option => option.value === value)?.value;

                  if (edge !== undefined && layer.kind === "bar") layer.edge = edge;
                })}
            />
          </>
        )}
      </Match>
    </Switch>
  );
};

export const AddLayerMenu: Component<{ onMutate: (mutate: (layers: Layer[]) => void) => void; }> = properties => (
  <DropdownMenu.Root>
    <DropdownMenu.Trigger class="secondary-button">
      <FiPlus class="size-4" />
      Add layer
      <FiChevronDown class="size-4 text-muted" />
    </DropdownMenu.Trigger>
    <DropdownMenu.Portal>
      <DropdownMenu.Content class="popover min-w-36">
        <For each={kinds}>
          {({ kind, label }) => (
            <MenuItem
              onSelect={() =>
                properties.onMutate((layers) => {
                  layers.push(newLayer(kind));
                })}
            >
              {label}
            </MenuItem>
          )}
        </For>
      </DropdownMenu.Content>
    </DropdownMenu.Portal>
  </DropdownMenu.Root>
);

/**
 * A key's face as an ordered stack, listed top layer first the way design tools list layers. The
 * stored array stays bottom first, the order the daemon draws, so moving a layer up raises its index.
 */
export const LayersField: Component<{
  layers: Layer[];
  onMutate: (mutate: (layers: Layer[]) => void) => void;
}> = properties => (
  <Show when={properties.layers.length > 0} fallback={<p class="empty">No layers.</p>}>
    <div class="rows">
      <For each={properties.layers.map((_, position, layers) => layers[layers.length - 1 - position])}>
        {(layer, displayIndex) => {
          const index = () => properties.layers.length - 1 - displayIndex();

          return (
            <div class="layer-row">
              <span class="layer-kind">{labelFor(layer.kind)}</span>
              <div class="layer-fields">
                <LayerFields
                  layer={layer}
                  onMutate={(mutate) => {
                    properties.onMutate((layers) => {
                      const target = layers[index()];

                      if (target !== undefined) mutate(target);
                    });
                  }}
                />
              </div>
              <div class="flex shrink-0">
                <button
                  type="button"
                  class="icon-button"
                  aria-label="Move layer up"
                  title="Move up"
                  disabled={index() === properties.layers.length - 1}
                  onClick={() => properties.onMutate(layers => swap(layers, index(), index() + 1))}
                >
                  <FiArrowUp class="size-4" />
                </button>
                <button
                  type="button"
                  class="icon-button"
                  aria-label="Move layer down"
                  title="Move down"
                  disabled={index() === 0}
                  onClick={() => properties.onMutate(layers => swap(layers, index(), index() - 1))}
                >
                  <FiArrowDown class="size-4" />
                </button>
                <button
                  type="button"
                  class="danger-button"
                  aria-label="Remove layer"
                  title="Remove layer"
                  onClick={() => properties.onMutate(layers => layers.splice(index(), 1))}
                >
                  <FiTrash2 class="size-4" />
                </button>
              </div>
            </div>
          );
        }}
      </For>
    </div>
  </Show>
);

const swap = (layers: Layer[], from: number, to: number): void => {
  const moved = layers[from];
  const displaced = layers[to];

  if (moved === undefined || displaced === undefined) return;

  layers[from] = displaced;
  layers[to] = moved;
};
