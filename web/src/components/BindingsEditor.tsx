import { TbFillTrash as TbTrash } from "solid-icons/tb";
import { Component, createMemo, For, Show } from "solid-js";

import { Action, ActionBinding, ActionTrigger, Control, SubpanelPlacement } from "../api/inventory";
import { ActionDefinition, coerceConfigValue, PluginInstance } from "../api/plugins";
import { useInventory } from "../context/InventoryContext";
import { ConfigFieldInput, SelectField } from "./fields";

type Mutate = (mutate: (control: Control) => void) => void;

const triggerOptions = [
  { value: "press", label: "Press" },
  { value: "release", label: "Release" },
  { value: "hold", label: "Hold" },
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

const triggerName = (gesture: ActionTrigger): string =>
  (typeof gesture === "string" ? gesture : "hold");
const holdDuration = (gesture: ActionTrigger): number =>
  (typeof gesture === "string" ? 800 : gesture.hold.duration_ms);
const asTrigger = (name: string, durationMs: number): ActionTrigger =>
  (name === "hold" ? { hold: { duration_ms: durationMs } } : (name as ActionTrigger));

const parameterValue = (action: Action, key: string): unknown =>
  (action.type === "invoke_integration" ? action.parameters[key] : undefined);

const ActionRow: Component<{
  binding: ActionBinding;
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

  const editAction = (edit: (action: Action) => void): void =>
    properties.onMutate((control) => {
      const target = control.action_bindings[properties.bindingIndex]?.actions[properties.actionIndex];

      if (target !== undefined) edit(target);
    });

  return (
    <div class="pressed-fields">
      <div class="flex items-center justify-between">
        <span class="chip chip-muted">{properties.action.type.replaceAll("_", " ")}</span>
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

      <Show when={properties.action.type === "invoke_integration" && properties.action}>
        {invoke => (
          <>
            <SelectField
              label="Instance"
              value={invoke().type === "invoke_integration" ? invoke().integration_id : ""}
              options={properties.instances.map(instance => ({
                value: instance.integration_id,
                label: instance.display_name,
              }))}
              onChange={value =>
                editAction((action) => {
                  if (action.type !== "invoke_integration") {
                    return;
                  }

                  action.integration_id = value;
                  action.action_name = "";
                  action.parameters = {};
                })}
            />
            <SelectField
              label="Action"
              value={invoke().type === "invoke_integration" ? invoke().action_name : ""}
              options={actionsFor(store, properties.instances, invoke().integration_id).map(action => ({
                value: action.name,
                label: action.label,
              }))}
              onChange={value =>
                editAction((action) => {
                  if (action.type !== "invoke_integration") {
                    return;
                  }

                  action.action_name = value;
                  action.parameters = {};
                })}
            />
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
          </>
        )}
      </Show>

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
    </div>
  );
};

const actionsFor = (
  store: ReturnType<typeof useInventory>,
  instances: PluginInstance[],
  integrationId: string,
): ActionDefinition[] => {
  const instance = instances.find(entry => entry.integration_id === integrationId);

  if (instance === undefined) return [];

  return store.plugins().types.find(type => type.plugin_type === instance.plugin_type)?.actions ?? [];
};

const newAction = (kind: Action["type"]): Action => {
  switch (kind) {
    case "wait": {
      return { type: "wait", duration_ms: 200 };
    }
    case "change_panel": {
      return { type: "change_panel", panel_id: "" };
    }
    case "open_subpanel": {
      return { type: "open_subpanel", panel_id: "", placement: "bottom_end", offset_columns: 0, offset_rows: 0 };
    }
    case "close_subpanel": {
      return { type: "close_subpanel" };
    }
    case "set_variable": {
      return { type: "set_variable", variable_name: "", value: "" };
    }
    default: {
      return { type: "invoke_integration", integration_id: "", action_name: "", parameters: {} };
    }
  }
};

const ActionBindingCard: Component<{
  binding: ActionBinding;
  index: number;
  instances: PluginInstance[];
  onMutate: Mutate;
}> = properties => (
  <div class="pressed-fields">
    <div class="flex items-end gap-2">
      <SelectField
        label="Gesture"
        value={triggerName(properties.binding.gesture)}
        options={triggerOptions}
        onChange={value =>
          properties.onMutate((control) => {
            const binding = control.action_bindings[properties.index];

            if (binding !== undefined) {
              binding.gesture = asTrigger(value, holdDuration(binding.gesture));
            }
          })}
      />
      <Show when={triggerName(properties.binding.gesture) === "hold"}>
        <label class="field-label">
          After (ms)
          <input
            class="field-input"
            type="number"
            value={holdDuration(properties.binding.gesture)}
            onInput={(event) => {
              const durationMs = Number(event.currentTarget.value);

              properties.onMutate((control) => {
                const binding = control.action_bindings[properties.index];

                if (binding !== undefined) binding.gesture = { hold: { duration_ms: durationMs } };
              });
            }}
          />
        </label>
      </Show>
      <button
        type="button"
        class="danger-button"
        aria-label="Remove binding"
        onClick={() =>
          properties.onMutate((control) => {
            control.action_bindings.splice(properties.index, 1);
          })}
      >
        <TbTrash class="h-3.5 w-3.5" />
      </button>
    </div>

    <For each={properties.binding.actions}>
      {(action, actionIndex) => (
        <ActionRow
          binding={properties.binding}
          bindingIndex={properties.index}
          action={action}
          actionIndex={actionIndex()}
          instances={properties.instances}
          onMutate={properties.onMutate}
        />
      )}
    </For>

    <div class="flex flex-wrap gap-1.5">
      <For each={["invoke_integration", "set_variable", "change_panel", "open_subpanel", "close_subpanel", "wait"] as const}>
        {kind => (
          <button
            type="button"
            class="secondary-button"
            onClick={() =>
              properties.onMutate((control) => {
                control.action_bindings[properties.index]?.actions.push(newAction(kind));
              })}
          >
            {`+ ${kind.replaceAll("_", " ")}`}
          </button>
        )}
      </For>
    </div>
  </div>
);

/**
 * What a key does when pressed. How it *looks* is no longer edited here: a style field binds to a
 * value directly, so there is nothing to configure beyond the reference itself.
 */
export const BindingsEditor: Component<{ control: Control; onMutate: Mutate; }> = (properties) => {
  const store = useInventory();
  const instances = createMemo(() =>
    store.plugins().instances.filter(instance => instance.status.state === "running"),
  );

  return (
    <>
      <div class="mt-2 flex items-center justify-between">
        <p class="field-label">Actions</p>
        <button
          type="button"
          class="secondary-button"
          onClick={() =>
            properties.onMutate((control) => {
              control.action_bindings.push({
                gesture: "press",
                actions: [newAction("invoke_integration")],
              });
            })}
        >
          + Binding
        </button>
      </div>
      <Show
        when={properties.control.action_bindings.length > 0}
        fallback={<p class="hint">Nothing happens when this key is pressed.</p>}
      >
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
      </Show>

    </>
  );
};
