import { TbFillCirclePlus as TbPlus, TbFillTrash as TbTrash } from "solid-icons/tb";
import { Component, createMemo, createSignal, For, Show } from "solid-js";
import { createStore } from "solid-js/store";

import {
  AvailableAction,
  coerceConfigValue,
  ConfigField,
  parseUserValue,
  variableReference,
} from "../api/plugins";
import { ConfigFieldInput, TextField } from "../components/fields";
import { useInventory } from "../context/InventoryContext";

const ActionRow: Component<{ action: AvailableAction; }> = (properties) => {
  const store = useInventory();
  const [parameters, setParameters] = createStore<Record<string, unknown>>({});
  const [isOpen, setIsOpen] = createSignal(false);
  const setField = (field: ConfigField, raw: string | boolean) =>
    setParameters(field.key, coerceConfigValue(field, raw));

  return (
    <div class="row flex-col items-stretch gap-2">
      <div class="flex items-center gap-3">
        <div class="row-main">
          <div class="min-w-0 flex-1">
            <p class="row-title">{properties.action.label}</p>
            <p class="row-meta">
              <span class="mono">
                {properties.action.integration_id}
                {" - "}
                {properties.action.name}
              </span>
              <Show when={properties.action.description}>
                {description => (
                  <>
                    <span class="meta-sep">-</span>
                    {description()}
                  </>
                )}
              </Show>
            </p>
          </div>
        </div>
        <button type="button" class="secondary-button" onClick={() => setIsOpen(!isOpen())}>
          {isOpen() ? "Close" : "Run"}
        </button>
      </div>

      <Show when={isOpen()}>
        <div class="pressed-fields">
          <For each={properties.action.parameters}>
            {field => (
              <ConfigFieldInput
                field={field}
                value={parameters[field.key]}
                onChange={raw => setField(field, raw)}
              />
            )}
          </For>
          <button
            type="button"
            class="primary-button"
            disabled={store.isSaving()}
            onClick={() =>
              void store.runPluginAction(
                properties.action.integration_id,
                properties.action.name,
                { ...parameters },
              )}
          >
            Run now
          </button>
        </div>
      </Show>
    </div>
  );
};

const CreateUserValue: Component = () => {
  const store = useInventory();
  const [name, setName] = createSignal("");
  const [value, setValue] = createSignal("");

  const submit = async (event: SubmitEvent): Promise<void> => {
    event.preventDefault();

    if (name().trim() === "") return;

    const saved = await store.saveUserValue({
      name: name().trim(),
      value: parseUserValue(value()),
      description: null,
    });

    if (saved) {
      setName("");
      setValue("");
    }
  };

  return (
    <form class="grid gap-2 sm:grid-cols-[1fr_1fr_auto] sm:items-end" onSubmit={event => void submit(event)}>
      <TextField
        label="Name"
        value={name()}
        placeholder="mode"
        onChange={setName}
      />
      <TextField
        label="Value"
        value={value()}
        placeholder="day"
        onChange={setValue}
      />
      <button type="submit" class="primary-button" disabled={store.isSaving()}>
        <TbPlus class="h-3.5 w-3.5" />
        Add
      </button>
    </form>
  );
};

export const ValuesPage: Component = () => {
  const store = useInventory();
  const [search, setSearch] = createSignal("");

  const needle = () => search().trim()
    .toLowerCase();
  const matchesSearch = (...haystack: string[]) =>
    needle() === "" || haystack.some(text => text.toLowerCase().includes(needle()));

  // Live where the event stream has published one, falling back to what the snapshot fetched.
  const values = createMemo(() =>
    store
      .values()
      .values.map(entry => ({
        ...entry,
        rendered: store.variables[`${entry.integration_id}:${entry.name}`] ?? entry.rendered,
      }))
      .filter(entry => matchesSearch(entry.integration_id, entry.name)),
  );
  const actions = createMemo(() =>
    store.values().actions.filter(action =>
      matchesSearch(action.integration_id, action.name, action.label),
    ),
  );
  const userValues = createMemo(() =>
    store.values().user_values.filter(value => matchesSearch("user", value.name)),
  );

  return (
    <div class="page">
      <div class="page-head">
        <div>
          <h1 class="page-title">Values</h1>
          <p class="page-subtitle">
            Everything the daemon knows, and everything it can be asked to do.
          </p>
        </div>
        <div class="w-64">
          <TextField
            label="Search"
            value={search()}
            placeholder="light, title, toggle..."
            onChange={setSearch}
          />
        </div>
      </div>

      <div class="card">
        <div class="card-head">
          <p class="card-title">Your values</p>
          <span class="chip chip-muted">{userValues().length}</span>
        </div>
        <div class="card-body">
          <CreateUserValue />
          <Show
            when={userValues().length > 0}
            fallback={<p class="empty">None yet. These persist in values.toml.</p>}
          >
            <div class="rows">
              <For each={userValues()}>
                {value => (
                  <div class="row">
                    <div class="row-main">
                      <div class="min-w-0 flex-1">
                        <p class="row-title mono">{variableReference("user", value.name)}</p>
                        <p class="row-meta">{String(value.value)}</p>
                      </div>
                    </div>
                    <button
                      type="button"
                      class="danger-button"
                      aria-label={`Remove ${value.name}`}
                      onClick={() => void store.removeUserValue(value.name)}
                    >
                      <TbTrash class="h-3.5 w-3.5" />
                    </button>
                  </div>
                )}
              </For>
            </div>
          </Show>
        </div>
      </div>

      <div class="card">
        <div class="card-head">
          <p class="card-title">Published by plugins</p>
          <span class="chip chip-muted">{values().length}</span>
        </div>
        <div class="card-body">
          <Show
            when={values().length > 0}
            fallback={<p class="empty">No plugin has published anything yet.</p>}
          >
            <div class="rows">
              <For each={values()}>
                {entry => (
                  <div class="row">
                    <div class="row-main">
                      <div class="min-w-0 flex-1">
                        <p class="row-title mono">
                          {variableReference(entry.integration_id, entry.name)}
                        </p>
                        <p class="row-meta">{entry.rendered}</p>
                      </div>
                    </div>
                  </div>
                )}
              </For>
            </div>
          </Show>
        </div>
      </div>

      <div class="card">
        <div class="card-head">
          <p class="card-title">Actions</p>
          <span class="chip chip-muted">{actions().length}</span>
        </div>
        <div class="card-body">
          <Show
            when={actions().length > 0}
            fallback={<p class="empty">Add a plugin to get some actions.</p>}
          >
            <div class="rows">
              <For each={actions()}>{action => <ActionRow action={action} />}</For>
            </div>
          </Show>
        </div>
      </div>
    </div>
  );
};
