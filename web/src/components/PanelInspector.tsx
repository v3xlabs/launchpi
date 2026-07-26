import { TbFillClipboard as TbCopy, TbFillTrash as TbTrash } from "solid-icons/tb";
import { Component, For, Match, Show, Switch } from "solid-js";

import {
  capabilityLabels,
  Control,
  DialPlacement,
  Layer,
  Panel,
  panelDial,
  RgbaColor,
} from "../api/inventory";
import { PresetPickerDialog } from "../dialogs/PresetPickerDialog";
import { newState } from "../utils/rendered";
import { BindingsEditor } from "./BindingsEditor";
import { DialEditor, DialsField } from "./DialEditor";
import { TextField } from "./fields";
import { LayersField } from "./LayerEditor";

export type PanelSelection = { kind: "control"; controlId: string; } | { kind: "dial"; index: number; };

const PanelSettings: Component<{
  panel: Panel;
  dials: DialPlacement[];
  onMutate: (mutate: (panel: Panel) => void) => void;
}> = properties => (
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
      <DialsField
        panel={properties.panel}
        dials={properties.dials}
        onMutate={properties.onMutate}
      />
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
      <LayersField
        layers={properties.control.default_state.layers}
        onMutate={(mutate: (layers: Layer[]) => void) =>
          properties.onMutate(control => mutate(control.default_state.layers))}
      />

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
            <LayersField
              layers={pressed().layers}
              onMutate={(mutate: (layers: Layer[]) => void) =>
                properties.onMutate((control) => {
                  if (control.pressed_state) mutate(control.pressed_state.layers);
                })}
            />
          </div>
        )}
      </Show>

      <BindingsEditor control={properties.control} onMutate={properties.onMutate} />
    </div>
  </>
);

type PanelInspectorProperties = {
  panel: Panel;
  dials: DialPlacement[];
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
  const selectedDial = () => {
    const selection = properties.selection;

    return selection?.kind === "dial" ? panelDial(properties.panel, selection.index) : null;
  };

  return (
    <div class="card">
      <Switch
        fallback={(
          <PanelSettings
            panel={properties.panel}
            dials={properties.dials}
            onMutate={properties.onPanelMutate}
          />
        )}
      >
        <Match when={selectedDial()}>
          {dial => (
            <DialEditor
              dial={dial()}
              placement={properties.dials.find(placement => placement.index === dial().index)}
              layout={properties.panel.layout}
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
