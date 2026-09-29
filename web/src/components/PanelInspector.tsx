import * as Tabs from "@kobalte/core/tabs";
import { FiCheck, FiCopy, FiPlus, FiTrash2 } from "solid-icons/fi";
import { Component, createSignal, For, Match, Show, Switch } from "solid-js";

import {
  Capabilities,
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
import { FontFamilyField, TextField } from "./fields";
import { InfoTip } from "./InfoTip";
import { AddLayerMenu, LayersField } from "./LayerEditor";

export type PanelSelection = { kind: "control"; controlId: string; } | { kind: "dial"; index: number; };

/** What a device must support to show a panel, as toggles. Shared with the new panel dialog. */
export const NeedsField: Component<{
  value: Capabilities;
  isDisabled?: boolean;
  onToggle: (key: keyof Capabilities, isNeeded: boolean) => void;
}> = properties => (
  <div class="grid gap-1.5">
    <span class="field-label flex items-center gap-1.5">
      Needs
      <InfoTip>A device shows this panel only if it supports every item selected here.</InfoTip>
    </span>
    <div class="flex flex-wrap gap-1" role="group" aria-label="Needs">
      <For each={capabilityLabels}>
        {({ key, label }) => (
          <button
            type="button"
            class="toggle-chip"
            aria-pressed={properties.value[key]}
            disabled={properties.isDisabled}
            onClick={() => properties.onToggle(key, properties.value[key] === false)}
          >
            <Show when={properties.value[key]} fallback={<FiPlus class="size-4" />}>
              <FiCheck class="size-4" />
            </Show>
            {label}
          </button>
        )}
      </For>
    </div>
  </div>
);

const PanelSettings: Component<{
  panel: Panel;
  dials: DialPlacement[];
  onMutate: (mutate: (panel: Panel) => void) => void;
}> = properties => (
  <>
    <p class="border-b border-hairline px-4 py-3 font-semibold">Panel settings</p>
    <div class="grid max-w-xl gap-4 px-4 py-4">
      <TextField
        label="Name"
        value={properties.panel.name}
        onChange={value =>
          properties.onMutate((panel) => {
            panel.name = value;
          })}
      />
      <FontFamilyField
        label="Font family"
        value={properties.panel.font_family ?? ""}
        placeholder="System sans-serif"
        onChange={value =>
          properties.onMutate((panel) => {
            panel.font_family = value.trim() || undefined;
          })}
      />
      <NeedsField
        value={properties.panel.capabilities}
        onToggle={(key, isNeeded) =>
          properties.onMutate((panel) => {
            panel.capabilities[key] = isNeeded;
          })}
      />
      <DialsField
        panel={properties.panel}
        dials={properties.dials}
        onMutate={properties.onMutate}
      />
    </div>
  </>
);

const LayerList: Component<{
  layers: Layer[];
  onMutate: (mutate: (layers: Layer[]) => void) => void;
}> = properties => (
  <>
    <Show when={properties.layers.length > 0}>
      <p class="px-4 pt-3 pb-1 text-muted">Top layer first.</p>
    </Show>
    <LayersField layers={properties.layers} onMutate={properties.onMutate} />
  </>
);

const ControlEditor: Component<{
  control: Control;
  onMutate: (mutate: (control: Control) => void) => void;
  onCopy: () => void;
  onRemove: () => void;
}> = (properties) => {
  const [tab, setTab] = createSignal("look");
  const mutateLook = (mutate: (layers: Layer[]) => void) =>
    properties.onMutate(control => mutate(control.default_state.layers));
  const mutatePressed = (mutate: (layers: Layer[]) => void) =>
    properties.onMutate((control) => {
      if (control.pressed_state) mutate(control.pressed_state.layers);
    });

  return (
    <>
      <div class="flex items-center gap-3 border-b border-hairline px-4 py-3">
        <span class="shrink-0 font-semibold">
          Key
          {" "}
          <span class="tabular-nums">
            {properties.control.position.row + 1}
            :
            {properties.control.position.column + 1}
          </span>
        </span>
        <input
          class="field-input w-64"
          aria-label="Name"
          value={properties.control.name}
          onInput={(event) => {
            const name = event.currentTarget.value;

            properties.onMutate((control) => {
              control.name = name;
            });
          }}
        />
        <div class="ml-auto flex items-center gap-1">
          <PresetPickerDialog
            triggerClass="secondary-button"
            onChoose={template =>
              properties.onMutate((control) => {
                // Everything about the button, nothing about where it sits.
                control.name = template.name;
                control.default_state = structuredClone(template.default_state);
                control.pressed_state = structuredClone(template.pressed_state);
                control.action_bindings = structuredClone(template.action_bindings);
              })}
          >
            Presets
          </PresetPickerDialog>
          <button
            type="button"
            class="icon-button"
            onClick={properties.onCopy}
            aria-label="Copy key"
            title="Copy key (Ctrl+C)"
          >
            <FiCopy class="size-4" />
          </button>
          <button
            type="button"
            class="danger-button"
            onClick={properties.onRemove}
            aria-label="Remove key"
            title="Remove key"
          >
            <FiTrash2 class="size-4" />
          </button>
        </div>
      </div>

      <div class="grid grid-cols-[minmax(0,1fr)_18rem]">
        <Tabs.Root value={tab()} onChange={setTab} class="min-w-0 pb-2">
          <div class="flex items-center gap-4 px-4 pt-1">
            <Tabs.List class="flex gap-4">
              <Tabs.Trigger class="tab" value="look">Look</Tabs.Trigger>
              <Tabs.Trigger class="tab" value="pressed">Pressed look</Tabs.Trigger>
            </Tabs.List>
            <Show when={tab() === "look" || properties.control.pressed_state !== null}>
              <span class="ml-auto">
                <AddLayerMenu onMutate={tab() === "look" ? mutateLook : mutatePressed} />
              </span>
            </Show>
          </div>
          <Tabs.Content value="look">
            <LayerList layers={properties.control.default_state.layers} onMutate={mutateLook} />
          </Tabs.Content>
          <Tabs.Content value="pressed">
            <Show
              when={properties.control.pressed_state}
              fallback={(
                <div class="grid justify-items-start gap-2 px-4 py-3">
                  <p class="text-muted">Uses the look.</p>
                  <button
                    type="button"
                    class="secondary-button"
                    onClick={() =>
                      properties.onMutate((control) => {
                        control.pressed_state = newState(true);
                      })}
                  >
                    Customise
                  </button>
                </div>
              )}
            >
              {pressed => (
                <>
                  <LayerList layers={pressed().layers} onMutate={mutatePressed} />
                  <div class="px-4 py-2">
                    <button
                      type="button"
                      class="link-button"
                      onClick={() =>
                        properties.onMutate((control) => {
                          control.pressed_state = null;
                        })}
                    >
                      Use the look
                    </button>
                  </div>
                </>
              )}
            </Show>
          </Tabs.Content>
        </Tabs.Root>
        <div class="border-l border-hairline px-4 py-3">
          <BindingsEditor control={properties.control} onMutate={properties.onMutate} />
        </div>
      </div>
    </>
  );
};

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
    <div class="surface">
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
