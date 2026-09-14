import { TbFillTrash as TbTrash } from "solid-icons/tb";
import { Component, createMemo, For, Show } from "solid-js";

import {
  Action,
  ActionBinding,
  ActionTrigger,
  Control,
  SubpanelPlacement,
} from "../api/inventory";
import { ActionDefinition, coerceConfigValue, PluginInstance } from "../api/plugins";
import { useInventory } from "../context/InventoryContext";
import { ActionPickerDialog } from "../dialogs/ActionPickerDialog";
import { actionTitle } from "../utils/actions";
import { ConfigFieldInput, SelectField } from "./fields";
import { SurfaceActionEditor } from "./SurfaceActionEditor";

type Mutate = (mutate: (control: Control) => void) => void;

const DEFAULT_HOLD_MS = 800;
const gestureOptions = [
  { name: "press", label: "Press" },
  { name: "hold", label: "Hold" },
  { name: "release", label: "Release" },
];
const placementOptions: Array<{ value: SubpanelPlacement; label: string; }> = [
  { value: "top_start", label: "Top left" },
  { value: "top_center", label: "Top center" },
  { value: "top_end", label: "Top right" },
  { value: "start_center", label: "Left middle" },
  { value: "end_center", label: "Right middle" },
  { value: "bottom_start", label: "Bottom left" },
  { value: "bottom_center", label: "Bottom center" },
  { value: "bottom_end", label: "Bottom right" },
];
const placementValues = placementOptions.map(option => option.value);

const gestureName = (gesture: ActionTrigger): string =>
  (typeof gesture === "string" ? gesture : "hold");
const holdDuration = (gesture: ActionTrigger): number =>
  (typeof gesture === "string" ? DEFAULT_HOLD_MS : gesture.hold.duration_ms);
const asGesture = (name: string, durationMs: number): ActionTrigger =>
  (name === "hold" ? { hold: { duration_ms: durationMs } } : (name as ActionTrigger));

const parameterValue = (action: Action, key: string): unknown =>
  (action.type === "invoke_integration" ? action.parameters[key] : undefined);

const ActionRow: Component<{
  bindingIndex: number;
  action: Action;
  actionIndex: number;
  instances: PluginInstance[];
  onMutate: Mutate;
}> = (properties) => {
  const store = useInventory();
  const definition = createMemo((): ActionDefinition | null => {
    if (properties.action.type !== "invoke_integration") return null;

    const instance = properties.instances.find(
      entry => entry.integration_id === (properties.action as { integration_id: string; }).integration_id,
    );

    if (instance === undefined) return null;

    const manifest = store.plugins().types.find(type => type.plugin_type === instance.plugin_type);

    return manifest?.actions.find(
      action => action.name === (properties.action as { action_name: string; }).action_name,
    ) ?? null;
  });
  const surfaceAction = createMemo(() => {
    const action = properties.action;

    return action.type === "set_surface_display" || action.type === "set_surface_brightness"
      ? action
      : null;
  });

  const editAction = (edit: (action: Action) => void): void =>
    properties.onMutate((control) => {
      const target = control.action_bindings[properties.bindingIndex]?.actions[properties.actionIndex];

      if (target !== undefined) edit(target);
    });

  return (
    <div class="action-card">
      <div class="action-card-head">
        <span class="action-card-title">{actionTitle(properties.action, store.plugins())}</span>
        <button
          type="button"
          class="danger-button"
          aria-label="Remove action"
          onClick={() =>
            properties.onMutate((control) => {
              control.action_bindings[properties.bindingIndex]?.actions.splice(
                properties.actionIndex,
                1,
              );
            })}
        >
          <TbTrash class="h-3.5 w-3.5" />
        </button>
      </div>

      <div class="action-card-body">
        <Show when={properties.action.type === "invoke_integration" && properties.action}>
          {invoke => (
            <SelectField
              label="Instance"
              value={invoke().type === "invoke_integration" ? invoke().integration_id : ""}
              options={properties.instances.map(instance => ({
                value: instance.integration_id,
                label: instance.display_name,
              }))}
              onChange={value =>
                editAction((action) => {
                  if (action.type === "invoke_integration") action.integration_id = value;
                })}
            />
          )}
        </Show>

        <For each={definition()?.parameters ?? []}>
          {field => (
            <ConfigFieldInput
              field={field}
              supportsReferences
              integrationId={
                properties.action.type === "invoke_integration"
                  ? properties.action.integration_id
                  : undefined
              }
              value={parameterValue(properties.action, field.key)}
              onChange={raw =>
                editAction((action) => {
                  if (action.type === "invoke_integration") {
                    action.parameters[field.key] = coerceConfigValue(field, raw);
                  }
                })}
            />
          )}
        </For>

        <Show when={properties.action.type === "wait" && properties.action}>
          {wait => (
            <label class="field-label">
              Duration (ms)
              <input
                class="field-input"
                type="number"
                value={wait().type === "wait" ? wait().duration_ms : 0}
                onInput={(event) => {
                  const durationMs = Number(event.currentTarget.value);

                  editAction((action) => {
                    if (action.type === "wait") action.duration_ms = durationMs;
                  });
                }}
              />
            </label>
          )}
        </Show>

        <Show when={properties.action.type === "set_variable" && properties.action}>
          {variable => (
            <>
              <label class="field-label">
                Name
                <input
                  class="field-input"
                  value={variable().type === "set_variable" ? variable().variable_name : ""}
                  onInput={(event) => {
                    const name = event.currentTarget.value;

                    editAction((action) => {
                      if (action.type === "set_variable") action.variable_name = name;
                    });
                  }}
                />
              </label>
              <label class="field-label">
                Value
                <input
                  class="field-input"
                  value={variable().type === "set_variable" ? String(variable().value ?? "") : ""}
                  onInput={(event) => {
                    const value = event.currentTarget.value;

                    editAction((action) => {
                      if (action.type === "set_variable") action.value = value;
                    });
                  }}
                />
              </label>
            </>
          )}
        </Show>

        <Show when={properties.action.type === "change_panel" && properties.action}>
          {change => (
            <SelectField
              label="Panel"
              value={change().type === "change_panel" ? change().panel_id : ""}
              options={store.inventory().panels.map(panel => ({
                value: panel.panel_id,
                label: panel.name,
              }))}
              onChange={value =>
                editAction((action) => {
                  if (action.type === "change_panel") action.panel_id = value;
                })}
            />
          )}
        </Show>

        <Show when={properties.action.type === "open_subpanel" && properties.action}>
          {open => (
            <>
              <SelectField
                label="Subpanel"
                value={open().type === "open_subpanel" ? open().panel_id : ""}
                options={store.inventory().panels.map(panel => ({ value: panel.panel_id, label: panel.name }))}
                onChange={value => editAction((action) => {
                  if (action.type === "open_subpanel") action.panel_id = value;
                })}
              />
              <SelectField
                label="Open from"
                value={open().type === "open_subpanel" ? open().placement : "bottom_end"}
                options={placementOptions}
                onChange={value => editAction((action) => {
                  const placement = placementValues.find(option => option === value);

                  if (placement !== undefined && action.type === "open_subpanel") action.placement = placement;
                })}
              />
              <div class="grid grid-cols-2 gap-2">
                <label class="field-label">
                  Column offset
                  <input
                    class="field-input"
                    type="number"
                    value={open().type === "open_subpanel" ? open().offset_columns : 0}
                    onInput={(event) => {
                      const offsetColumns = Number(event.currentTarget.value);

                      editAction((action) => {
                        if (action.type === "open_subpanel") action.offset_columns = offsetColumns;
                      });
                    }}
                  />
                </label>
                <label class="field-label">
                  Row offset
                  <input
                    class="field-input"
                    type="number"
                    value={open().type === "open_subpanel" ? open().offset_rows : 0}
                    onInput={(event) => {
                      const offsetRows = Number(event.currentTarget.value);

                      editAction((action) => {
                        if (action.type === "open_subpanel") action.offset_rows = offsetRows;
                      });
                    }}
                  />
                </label>
              </div>
            </>
          )}
        </Show>

        <Show when={surfaceAction()}>
          {surface => (
            <SurfaceActionEditor
              action={surface()}
              onMutate={mutate => editAction((action) => {
                if (action.type === "set_surface_display" || action.type === "set_surface_brightness") {
                  mutate(action);
                }
              })}
            />
          )}
        </Show>
      </div>
    </div>
  );
};

const GestureField: Component<{
  binding: ActionBinding;
  index: number;
  onMutate: Mutate;
}> = properties => (
  <div class="gesture-row">
    <div class="segmented" role="group" aria-label="Gesture">
      <For each={gestureOptions}>
        {option => (
          <button
            type="button"
            class="segment"
            data-selected={gestureName(properties.binding.gesture) === option.name}
            onClick={() =>
              properties.onMutate((control) => {
                const binding = control.action_bindings[properties.index];

                if (binding !== undefined) {
                  binding.gesture = asGesture(option.name, holdDuration(binding.gesture));
                }
              })}
          >
            {option.label}
          </button>
        )}
      </For>
    </div>
    <Show when={gestureName(properties.binding.gesture) === "hold"}>
      <label class="hold-field">
        <input
          class="field-input"
          type="number"
          min="0"
          step="100"
          aria-label="Hold duration in milliseconds"
          value={holdDuration(properties.binding.gesture)}
          onInput={(event) => {
            const durationMs = Number(event.currentTarget.value);

            properties.onMutate((control) => {
              const binding = control.action_bindings[properties.index];

              if (binding !== undefined) binding.gesture = { hold: { duration_ms: durationMs } };
            });
          }}
        />
        <span class="chip chip-muted">ms</span>
      </label>
    </Show>
    <button
      type="button"
      class="danger-button ml-auto"
      aria-label="Remove gesture"
      onClick={() =>
        properties.onMutate((control) => {
          control.action_bindings.splice(properties.index, 1);
        })}
    >
      <TbTrash class="h-3.5 w-3.5" />
    </button>
  </div>
);

const ActionBindingCard: Component<{
  binding: ActionBinding;
  index: number;
  instances: PluginInstance[];
  onMutate: Mutate;
}> = properties => (
  <div class="binding-card">
    <GestureField
      binding={properties.binding}
      index={properties.index}
      onMutate={properties.onMutate}
    />

    <For each={properties.binding.actions}>
      {(action, actionIndex) => (
        <ActionRow
          bindingIndex={properties.index}
          action={action}
          actionIndex={actionIndex()}
          instances={properties.instances}
          onMutate={properties.onMutate}
        />
      )}
    </For>

    <ActionPickerDialog
      trigger={<button type="button" class="secondary-button">+ Action</button>}
      onChoose={action =>
        properties.onMutate((control) => {
          control.action_bindings[properties.index]?.actions.push(action);
        })}
    />
  </div>
);

/**
 * What a key does when pressed. How it *looks* is no longer edited here: a style field binds to a
 * value directly, so there is nothing to configure beyond the reference itself.
 *
 * The header button is the whole flow for a one-action key: it appends to the press gesture and
 * creates it if the key has none, so nothing has to be set up before an action can be chosen.
 */
export const BindingsEditor: Component<{ control: Control; onMutate: Mutate; }> = (properties) => {
  const store = useInventory();
  const instances = createMemo(() =>
    store.plugins().instances.filter(instance => instance.status.state === "running"),
  );

  const addOnPress = (action: Action) =>
    properties.onMutate((control) => {
      const binding = control.action_bindings.find(entry => entry.gesture === "press");

      if (binding === undefined) {
        control.action_bindings.push({ gesture: "press", actions: [action] });

        return;
      }

      binding.actions.push(action);
    });

  return (
    <>
      <div class="mt-2 flex items-center justify-between">
        <p class="field-label">Actions</p>
        <ActionPickerDialog
          trigger={<button type="button" class="primary-button">+ Action</button>}
          onChoose={addOnPress}
        />
      </div>
      <Show when={properties.control.action_bindings.length > 0}>
        <For each={properties.control.action_bindings}>
          {(binding, index) => (
            <ActionBindingCard
              binding={binding}
              index={index()}
              instances={instances()}
              onMutate={properties.onMutate}
            />
          )}
        </For>
        <button
          type="button"
          class="secondary-button"
          onClick={() =>
            properties.onMutate((control) => {
              control.action_bindings.push({
                gesture: { hold: { duration_ms: DEFAULT_HOLD_MS } },
                actions: [],
              });
            })}
        >
          + Gesture
        </button>
      </Show>
    </>
  );
};
