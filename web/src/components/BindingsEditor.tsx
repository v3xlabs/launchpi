import { FiPlus, FiTrash2 } from "solid-icons/fi";
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
const gestureOptions: Array<{ name: "press" | "hold" | "release"; label: string; }> = [
  { name: "press", label: "On press" },
  { name: "hold", label: "On hold" },
  { name: "release", label: "On release" },
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
    const action = properties.action;

    if (action.type !== "invoke_integration") return null;

    const instance = properties.instances.find(entry => entry.integration_id === action.integration_id);

    if (instance === undefined) return null;

    const manifest = store.plugins().types.find(type => type.plugin_type === instance.plugin_type);

    return manifest?.actions.find(entry => entry.name === action.action_name) ?? null;
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
    <div class="action-row">
      <div class="action-row-head">
        <span class="action-row-title">{actionTitle(properties.action, store.plugins())}</span>
        <button
          type="button"
          class="danger-button"
          aria-label="Remove action"
          title="Remove action"
          onClick={() =>
            properties.onMutate((control) => {
              control.action_bindings[properties.bindingIndex]?.actions.splice(
                properties.actionIndex,
                1,
              );
            })}
        >
          <FiTrash2 class="size-4" />
        </button>
      </div>

      <div class="grid gap-2">
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

/** A gesture's heading is its kind: changing the select rewrites the trigger and keeps the actions. */
const GestureSection: Component<{
  binding: ActionBinding;
  index: number;
  instances: PluginInstance[];
  onMutate: Mutate;
}> = (properties) => {
  const name = () => gestureName(properties.binding.gesture);
  const setGesture = (next: ActionTrigger) =>
    properties.onMutate((control) => {
      const binding = control.action_bindings[properties.index];

      if (binding !== undefined) binding.gesture = next;
    });

  return (
    <section class="grid gap-2">
      <div class="flex items-center gap-2">
        <select
          class="-ml-1 rounded-control bg-transparent px-1 py-1 font-medium text-soft outline-none hover:bg-raised focus-visible:ring-2 focus-visible:ring-slate-400/60"
          aria-label="Gesture"
          value={name()}
          onInput={(event) => {
            const { value } = event.currentTarget;
            const option = gestureOptions.find(entry => entry.name === value);

            if (option === undefined) return;

            setGesture(option.name === "hold"
              ? { hold: { duration_ms: holdDuration(properties.binding.gesture) } }
              : option.name);
          }}
        >
          <For each={gestureOptions}>{option => <option value={option.name}>{option.label}</option>}</For>
          <Show when={gestureOptions.every(option => option.name !== name())}>
            <option value={name()}>{`On ${name().replaceAll("_", " ")}`}</option>
          </Show>
        </select>
        <Show when={name() === "hold"}>
          <label class="hold-field text-muted">
            <input
              class="field-input tabular-nums"
              type="number"
              min="0"
              step="100"
              aria-label="Hold duration in milliseconds"
              value={holdDuration(properties.binding.gesture)}
              onInput={event => setGesture({ hold: { duration_ms: Number(event.currentTarget.value) } })}
            />
            ms
          </label>
        </Show>
        <button
          type="button"
          class="danger-button ml-auto"
          aria-label="Remove gesture"
          title="Remove gesture"
          onClick={() =>
            properties.onMutate((control) => {
              control.action_bindings.splice(properties.index, 1);
            })}
        >
          <FiTrash2 class="size-4" />
        </button>
      </div>

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
        triggerClass="secondary-button justify-self-start"
        onChoose={action =>
          properties.onMutate((control) => {
            control.action_bindings[properties.index]?.actions.push(action);
          })}
      >
        <FiPlus class="size-4" />
        Add action
      </ActionPickerDialog>
    </section>
  );
};

/**
 * What a key does. How it *looks* is not edited here: a style field binds to a value directly, so
 * there is nothing to configure beyond the reference itself.
 */
export const BindingsEditor: Component<{ control: Control; onMutate: Mutate; }> = (properties) => {
  const store = useInventory();
  const instances = createMemo(() =>
    store.plugins().instances.filter(instance => instance.status.state === "running"),
  );

  return (
    <div class="grid gap-5">
      <Show
        when={properties.control.action_bindings.length > 0}
        fallback={(
          <section class="grid gap-2">
            <p class="py-1 font-medium text-soft">On press</p>
            <p class="text-muted">Nothing happens yet.</p>
            <ActionPickerDialog
              triggerClass="secondary-button justify-self-start"
              onChoose={action =>
                properties.onMutate((control) => {
                  control.action_bindings.push({ gesture: "press", actions: [action] });
                })}
            >
              <FiPlus class="size-4" />
              Add action
            </ActionPickerDialog>
          </section>
        )}
      >
        <For each={properties.control.action_bindings}>
          {(binding, index) => (
            <GestureSection
              binding={binding}
              index={index()}
              instances={instances()}
              onMutate={properties.onMutate}
            />
          )}
        </For>
      </Show>
      <button
        type="button"
        class="link-button justify-self-start"
        onClick={() =>
          properties.onMutate((control) => {
            const hasHold = control.action_bindings.some(binding => gestureName(binding.gesture) === "hold");

            control.action_bindings.push({
              gesture: hasHold ? "release" : { hold: { duration_ms: DEFAULT_HOLD_MS } },
              actions: [],
            });
          })}
      >
        <FiPlus class="size-4" />
        Add gesture
      </button>
    </div>
  );
};
