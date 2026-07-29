import {
  TbFillArrowBigDown as TbDown,
  TbFillArrowBigUp as TbUp,
  TbFillTrash as TbTrash,
} from "solid-icons/tb";
import { Component, createSignal, For, Match, Show, Switch } from "solid-js";

import { Anchor9, ColorBinding, Edge, Fit, Layer, LayerKind, ValueBinding } from "../api/inventory";
import { isReference, newLayer } from "../utils/rendered";
import { ColorField, FontFamilyField, NumberField, SelectField } from "./fields";
import { IconPicker } from "./IconPicker";
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
  <div class="grid gap-1">
    <span class="field-label">{properties.label}</span>
    <div class="anchor-grid">
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

const fitOptions = [
  { value: "cover", label: "Cover the key" },
  { value: "contain", label: "Fit inside" },
];

const edgeOptions = [
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
          <div class="grid grid-cols-2 gap-2">
            <ColorField
              label="Colour"
              value={fill().color}
              fallback="#1e293b"
              onChange={color =>
                properties.onMutate((layer) => {
                  if (layer.kind === "fill") layer.color = color;
                })}
            />
            <NumberField
              label="Opacity %"
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
          </div>
        )}
      </Match>

      <Match when={properties.layer.kind === "border" ? properties.layer : null}>
        {border => (
          <div class="grid grid-cols-2 gap-2">
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
          </div>
        )}
      </Match>

      <Match when={properties.layer.kind === "text" ? properties.layer : null}>
        {text => (
          <>
            <ValueField
              label="Text"
              value={text().text}
              placeholder="Shown on the key"
              onChange={value =>
                properties.onMutate((layer) => {
                  if (layer.kind === "text") layer.text = value;
                })}
            />
            <FontFamilyField
              label="Font family"
              value={text().font_family ?? ""}
              placeholder="Panel default"
              onChange={value =>
                properties.onMutate((layer) => {
                  if (layer.kind === "text") layer.font_family = value.trim() || undefined;
                })}
            />
            <div class="grid grid-cols-2 gap-2">
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
            </div>
          </>
        )}
      </Match>

      <Match when={properties.layer.kind === "image" ? properties.layer : null}>
        {image => (
          <>
            <ValueField
              label="Image"
              value={image().image}
              placeholder="mdi:lightbulb, a URL, or $(mpris.default:art_url)"
              onChange={value =>
                properties.onMutate((layer) => {
                  if (layer.kind === "image") layer.image = value;
                })}
            />
            <Show
              when={isBrowsing()}
              fallback={(
                <button type="button" class="link-button justify-self-start" onClick={() => setIsBrowsing(true)}>
                  browse icons
                </button>
              )}
            >
              <IconPicker
                onChoose={(icon) => {
                  setIsBrowsing(false);
                  properties.onMutate((layer) => {
                    if (layer.kind === "image") layer.image = icon;
                  });
                }}
              />
            </Show>
            <div class="grid grid-cols-2 gap-2">
              <SelectField
                label="Fit"
                value={image().fit}
                options={fitOptions}
                onChange={value =>
                  properties.onMutate((layer) => {
                    if (layer.kind === "image") layer.fit = value as Fit;
                  })}
              />
              <NumberField
                label="Size %"
                value={image().scale_percent}
                onChange={value =>
                  properties.onMutate((layer) => {
                    const scale = toCount(value);

                    if (scale !== undefined && layer.kind === "image") layer.scale_percent = scale;
                  })}
              />
            </div>
            <div class="grid grid-cols-2 gap-2">
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
            </div>
          </>
        )}
      </Match>

      <Match when={properties.layer.kind === "bar" ? properties.layer : null}>
        {bar => (
          <>
            <div class="grid grid-cols-2 gap-2">
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
            </div>
            <div class="grid grid-cols-2 gap-2">
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
                    if (layer.kind === "bar") layer.edge = value as Edge;
                  })}
              />
            </div>
          </>
        )}
      </Match>
    </Switch>
  );
};

/**
 * A key's face as an ordered stack. The list reads bottom-up the way the key is drawn, so moving a
 * layer up in the editor moves it nearer the viewer.
 */
export const LayersField: Component<{
  layers: Layer[];
  onMutate: (mutate: (layers: Layer[]) => void) => void;
}> = properties => (
  <div class="grid gap-2">
    <span class="field-label">Layers</span>
    <For each={properties.layers}>
      {(layer, index) => (
        <div class="layer-card">
          <div class="layer-head">
            <span class="layer-kind">{labelFor(layer.kind)}</span>
            <div class="flex gap-1">
              <button
                type="button"
                class="icon-button"
                aria-label="Move layer up"
                disabled={index() === properties.layers.length - 1}
                onClick={() => properties.onMutate(layers => swap(layers, index(), index() + 1))}
              >
                <TbUp class="h-3 w-3" />
              </button>
              <button
                type="button"
                class="icon-button"
                aria-label="Move layer down"
                disabled={index() === 0}
                onClick={() => properties.onMutate(layers => swap(layers, index(), index() - 1))}
              >
                <TbDown class="h-3 w-3" />
              </button>
              <button
                type="button"
                class="danger-button"
                aria-label="Remove layer"
                onClick={() => properties.onMutate(layers => layers.splice(index(), 1))}
              >
                <TbTrash class="h-3 w-3" />
              </button>
            </div>
          </div>
          <div class="layer-body">
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
        </div>
      )}
    </For>
    <Show when={properties.layers.length === 0}>
      <p class="hint">Nothing is drawn on this key yet.</p>
    </Show>
    <div class="layer-add">
      <For each={kinds}>
        {({ kind, label }) => (
          <button
            type="button"
            class="secondary-button"
            onClick={() =>
              properties.onMutate((layers) => {
                layers.push(newLayer(kind));
              })}
          >
            {`+ ${label}`}
          </button>
        )}
      </For>
    </div>
  </div>
);

const swap = (layers: Layer[], from: number, to: number): void => {
  const moved = layers[from];
  const displaced = layers[to];

  if (moved === undefined || displaced === undefined) return;

  layers[from] = displaced;
  layers[to] = moved;
};
