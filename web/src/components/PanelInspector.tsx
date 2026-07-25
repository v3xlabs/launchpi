import { TbFillClipboard as TbCopy, TbFillTrash as TbTrash } from "solid-icons/tb";
import { Component, For, Match, Show, Switch } from "solid-js";

import { Anchor9, capabilityLabels, Control, Panel, panelDial, RgbaColor } from "../api/inventory";
import { PresetPickerDialog } from "../dialogs/PresetPickerDialog";
import { newState, toHex } from "../utils/rendered";
import { BindingsEditor } from "./BindingsEditor";
import { DialIndicator, litRingSegments, totalRingSegments } from "./DialIndicator";
import { ColorField, NumberField, SelectField, TextField } from "./fields";
import { ValueField } from "./ValueField";

export type PanelSelection = { kind: "control"; controlId: string; } | { kind: "dial"; index: number; };

const textAnchors: Anchor9[] = [
  "top_start", "top_center", "top_end", "center_start", "center", "center_end", "bottom_start", "bottom_center", "bottom_end",
];

const anchorOptions = [
  { value: "top_start", label: "Left top" }, { value: "top_center", label: "Middle top" }, { value: "top_end", label: "Right top" },
  { value: "center_start", label: "Left middle" }, { value: "center", label: "Middle middle" }, { value: "center_end", label: "Right middle" },
  { value: "bottom_start", label: "Left bottom" }, { value: "bottom_center", label: "Middle bottom" }, { value: "bottom_end", label: "Right bottom" },
];

const toAnchor = (value: string): Anchor9 | undefined => textAnchors.find(anchor => anchor === value);

const toCount = (value: string): number | undefined => {
  const parsed = Number(value);

  return value.trim() === "" || Number.isNaN(parsed) ? undefined : parsed;
};

const PanelSettings: Component<{ panel: Panel; onMutate: (mutate: (panel: Panel) => void) => void; }> = properties => (
  <>
    <div class="card-head">
      <p class="card-title">Panel</p>
    </div>
    <div class="card-body">
      <TextField
        label="Name"
        value={properties.panel.name}
        onChange={value =>
          properties.onMutate((panel) => {
            panel.name = value;
          })}
      />
      <fieldset class="grid gap-1">
        <legend class="field-label">Required capabilities</legend>
        <div class="mt-1 grid grid-cols-2 gap-1.5">
          <For each={capabilityLabels}>
            {({ key, label }) => (
              <label class="check-tile">
                <input
                  type="checkbox"
                  checked={properties.panel.capabilities[key]}
                  onInput={event =>
                    properties.onMutate((panel) => {
                      panel.capabilities[key] = event.currentTarget.checked;
                    })}
                />
                {label}
              </label>
            )}
          </For>
        </div>
      </fieldset>
    </div>
  </>
);

const ControlEditor: Component<{
  control: Control;
  onMutate: (mutate: (control: Control) => void) => void;
  onCopy: () => void;
  onRemove: () => void;
}> = properties => (
  <>
    <div class="card-head">
      <p class="card-title">
        Key
        {" "}
        {properties.control.position.row + 1}
        :
        {properties.control.position.column + 1}
      </p>
      <div class="flex gap-1.5">
        <PresetPickerDialog
          trigger={<button type="button" class="link-button">preset</button>}
          onChoose={template =>
            properties.onMutate((control) => {
              // Everything about the button, nothing about where it sits.
              control.name = template.name;
              control.default_state = structuredClone(template.default_state);
              control.pressed_state = structuredClone(template.pressed_state);
              control.action_bindings = structuredClone(template.action_bindings);
            })}
        />
        <button
          type="button"
          class="icon-button"
          onClick={properties.onCopy}
          aria-label="Copy control"
          title="Copy (Ctrl/Cmd+C)"
        >
          <TbCopy class="h-3.5 w-3.5" />
        </button>
        <button
          type="button"
          class="danger-button"
          onClick={properties.onRemove}
          aria-label="Remove control"
          title="Remove control"
        >
          <TbTrash class="h-3.5 w-3.5" />
        </button>
      </div>
    </div>
    <div class="card-body">
      <TextField
        label="Name"
        value={properties.control.name}
        onChange={value =>
          properties.onMutate((control) => {
            control.name = value;
          })}
      />
      <ValueField
        label="Label"
        value={properties.control.default_state.text ?? ""}
        placeholder="Shown on the key"
        onChange={value =>
          properties.onMutate((control) => {
            control.default_state.text = value || null;
          })}
      />
      <SelectField
        label="Label position"
        value={properties.control.default_state.content_layout.text_anchor}
        options={anchorOptions}
        onChange={value => properties.onMutate((control) => {
          const anchor = toAnchor(value);

          if (anchor !== undefined) control.default_state.content_layout.text_anchor = anchor;
        })}
      />
      <ValueField
        label="Image"
        value={properties.control.default_state.image ?? ""}
        placeholder="$(mpris.default:art_url) or a URL"
        onChange={value =>
          properties.onMutate((control) => {
            control.default_state.image = value || null;
          })}
      />
      <p class="hint">
        A value reference, an image URL, or
        {" "}
        <span class="mono">builtin:play</span>
        . Downloaded once and cached.
      </p>
      <div class="grid grid-cols-2 gap-2">
        <ColorField
          label="Text"
          value={properties.control.default_state.foreground_color}
          fallback="#ffffff"
          onChange={color =>
            properties.onMutate((control) => {
              control.default_state.foreground_color = color;
            })}
        />
        <ColorField
          label="Fill"
          value={properties.control.default_state.background_color}
          fallback="#1e293b"
          onChange={color =>
            properties.onMutate((control) => {
              control.default_state.background_color = color;
            })}
        />
      </div>

      <label class="check-tile">
        <input
          type="checkbox"
          checked={properties.control.default_state.border !== null}
          onInput={(event) => {
            const { checked } = event.currentTarget;

            properties.onMutate((control) => {
              control.default_state.border = checked
                ? { color: { red: 255, green: 255, blue: 255, alpha: 255 }, width: 5 }
                : null;
            });
          }}
        />
        Border
      </label>

      <Show when={properties.control.default_state.border}>
        {border => (
          <div class="grid grid-cols-2 gap-2">
            <ColorField
              label="Colour"
              value={border().color}
              fallback="#ffffff"
              onChange={color =>
                properties.onMutate((control) => {
                  if (control.default_state.border) control.default_state.border.color = color;
                })}
            />
            <NumberField
              label="Width"
              value={border().width}
              onChange={value =>
                properties.onMutate((control) => {
                  const width = toCount(value);

                  if (width !== undefined && control.default_state.border) {
                    control.default_state.border.width = width;
                  }
                })}
            />
          </div>
        )}
      </Show>

      <label class="check-tile">
        <input
          type="checkbox"
          checked={properties.control.default_state.overlay_image !== null}
          onInput={(event) => {
            const { checked } = event.currentTarget;

            properties.onMutate((control) => {
              control.default_state.overlay_image = checked
                ? { image: "", anchor: "bottom_end", scale_percent: 32 }
                : null;
            });
          }}
        />
        Badge
      </label>

      <Show when={properties.control.default_state.overlay_image}>
        {overlay => (
          <div class="pressed-fields">
            <ValueField
              label="Badge image"
              value={overlay().image}
              placeholder="$(discord.default:channel_members_0_status_icon)"
              onChange={value =>
                properties.onMutate((control) => {
                  if (control.default_state.overlay_image) control.default_state.overlay_image.image = value;
                })}
            />
            <div class="grid grid-cols-2 gap-2">
              <SelectField
                label="Corner"
                value={overlay().anchor}
                options={anchorOptions}
                onChange={value =>
                  properties.onMutate((control) => {
                    const anchor = toAnchor(value);

                    if (anchor !== undefined && control.default_state.overlay_image) {
                      control.default_state.overlay_image.anchor = anchor;
                    }
                  })}
              />
              <NumberField
                label="Size %"
                value={overlay().scale_percent}
                onChange={value =>
                  properties.onMutate((control) => {
                    const scale = toCount(value);

                    if (scale !== undefined && control.default_state.overlay_image) {
                      control.default_state.overlay_image.scale_percent = scale;
                    }
                  })}
              />
            </div>
          </div>
        )}
      </Show>

      <label class="check-tile">
        <input
          type="checkbox"
          checked={properties.control.pressed_state !== null}
          onInput={event =>
            properties.onMutate((control) => {
              control.pressed_state = event.currentTarget.checked ? newState(true) : null;
            })}
        />
        Pressed feedback
      </label>

      <Show when={properties.control.pressed_state}>
        {pressed => (
          <div class="pressed-fields">
            <ValueField
              label="Pressed label"
              value={pressed().text ?? ""}
              placeholder="Optional"
              onChange={value =>
                properties.onMutate((control) => {
                  if (control.pressed_state) control.pressed_state.text = value || null;
                })}
            />
            <div class="grid grid-cols-2 gap-2">
              <ColorField
                label="Text"
                value={pressed().foreground_color}
                fallback="#ffffff"
                onChange={color =>
                  properties.onMutate((control) => {
                    if (control.pressed_state) control.pressed_state.foreground_color = color;
                  })}
              />
              <ColorField
                label="Fill"
                value={pressed().background_color}
                fallback="#0f172a"
                onChange={color =>
                  properties.onMutate((control) => {
                    if (control.pressed_state) control.pressed_state.background_color = color;
                  })}
              />
            </div>
          </div>
        )}
      </Show>

      <BindingsEditor control={properties.control} onMutate={properties.onMutate} />
    </div>
  </>
);

const DialEditor: Component<{
  panel: Panel;
  index: number;
  onColorChange: (index: number, color: RgbaColor) => void;
  onLevelChange: (index: number, level: number) => void;
}> = (properties) => {
  const dial = () => panelDial(properties.panel, properties.index);

  return (
    <>
      <div class="card-head">
        <p class="card-title">
          Dial
          {" "}
          {properties.index + 1}
        </p>
        <span class="chip chip-muted">{properties.index === 0 ? "left" : "right"}</span>
      </div>
      <div class="card-body">
        <div class="flex items-center gap-4 bg-neutral-950 p-3">
          <div class="w-14 shrink-0">
            <DialIndicator index={properties.index} color={dial().color} level={dial().level} />
          </div>
          <p class="mono">
            {toHex(dial().color, "unset")}
            {" - "}
            {dial().level}
            % -
            {" "}
            {litRingSegments(dial().level)}
            /
            {totalRingSegments}
            {" "}
            segments
          </p>
        </div>
        <ColorField
          label="Colour"
          value={dial().color}
          fallback="#1e293b"
          bindable={false}
          onChange={(color) => {
            if (typeof color !== "string") properties.onColorChange(properties.index, color);
          }}
        />
        <label class="field-label">
          Ring level -
          {" "}
          {dial().level}
          %
          <input
            class="range-input"
            type="range"
            min="0"
            max="100"
            value={dial().level}
            onInput={event => properties.onLevelChange(properties.index, Number(event.currentTarget.value))}
          />
        </label>
      </div>
    </>
  );
};

type PanelInspectorProperties = {
  panel: Panel;
  selection: PanelSelection | null;
  control: Control | null;
  onPanelMutate: (mutate: (panel: Panel) => void) => void;
  onControlMutate: (mutate: (control: Control) => void) => void;
  onCopyControl: () => void;
  onRemoveControl: () => void;
  onDialColorChange: (index: number, color: RgbaColor) => void;
  onDialLevelChange: (index: number, level: number) => void;
};

export const PanelInspector: Component<PanelInspectorProperties> = (properties) => {
  // Guard on the selection object, not the index - dial 0 is a falsy value.
  const dialSelection = () => (properties.selection?.kind === "dial" ? properties.selection : null);

  return (
    <div class="card">
      <Switch fallback={<PanelSettings panel={properties.panel} onMutate={properties.onPanelMutate} />}>
        <Match when={dialSelection()}>
          {selection => (
            <DialEditor
              panel={properties.panel}
              index={selection().index}
              onColorChange={properties.onDialColorChange}
              onLevelChange={properties.onDialLevelChange}
            />
          )}
        </Match>
        <Match when={properties.control}>
          {control => (
            <ControlEditor
              control={control()}
              onMutate={properties.onControlMutate}
              onCopy={properties.onCopyControl}
              onRemove={properties.onRemoveControl}
            />
          )}
        </Match>
      </Switch>
    </div>
  );
};
