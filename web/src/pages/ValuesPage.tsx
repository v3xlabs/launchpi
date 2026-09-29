import { FiPlus, FiTag, FiTrash2 } from "solid-icons/fi";
import { Component, createMemo, createSignal, For, Show } from "solid-js";

import { parseUserValue, variableReference } from "../api/plugins";
import { SearchField } from "../components/fields";
import { InfoTip } from "../components/InfoTip";
import { PageHeader } from "../components/PageHeader";
import { useInventory } from "../context/InventoryContext";
import { countOf } from "../utils/plural";
import { instanceTitle } from "./PluginsPage";

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
    <form class="flex items-center gap-2 px-4 py-2.5" onSubmit={event => void submit(event)}>
      <input
        class="field-input w-48"
        aria-label="Name"
        placeholder="Name"
        value={name()}
        onInput={event => setName(event.currentTarget.value)}
      />
      <input
        class="field-input flex-1"
        aria-label="Value"
        placeholder="Value"
        value={value()}
        onInput={event => setValue(event.currentTarget.value)}
      />
      <button type="submit" class="secondary-button" disabled={store.isSaving()}>
        <FiPlus class="size-4" />
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

  const publishedValues = createMemo(() =>
    store.values().values.filter(entry => entry.integration_id !== "user"),
  );
  const userValues = createMemo(() =>
    store.values().user_values.filter(value => matchesSearch("user", value.name)),
  );
  // Live where the event stream has published one, falling back to what the snapshot fetched.
  const groups = createMemo(() => {
    const byInstance = new Map<string, Array<{ name: string; rendered: string; }>>();

    for (const entry of publishedValues()) {
      if (!matchesSearch(entry.integration_id, entry.name)) continue;

      const entries = byInstance.get(entry.integration_id) ?? [];

      entries.push({
        name: entry.name,
        rendered: store.variables[`${entry.integration_id}:${entry.name}`] ?? entry.rendered,
      });
      byInstance.set(entry.integration_id, entries);
    }

    return [...byInstance].map(([integrationId, entries]) => {
      const instance = store.plugins().instances.find(found => found.integration_id === integrationId);

      return {
        integrationId,
        title: instance === undefined ? integrationId : instanceTitle(instance, store.plugins().types),
        entries,
      };
    });
  });

  return (
    <div class="page">
      <PageHeader
        mark={<FiTag class="size-5" />}
        title="Values"
        meta={(
          <span class="tabular-nums">
            {countOf(store.values().user_values.length + publishedValues().length, "value")}
          </span>
        )}
        actions={(
          <div class="w-64">
            <SearchField
              label="Search values"
              value={search()}
              placeholder="Search values"
              onChange={setSearch}
            />
          </div>
        )}
      />

      <section>
        <div class="section-head">
          <h2 class="label-sm">Yours</h2>
          <InfoTip>Set by you or by a key's Set value action. Use them anywhere as $(user:name).</InfoTip>
        </div>
        <div class="surface rows">
          <For each={userValues()}>
            {value => (
              <div class="row py-2">
                <code class="mono min-w-0 flex-1 truncate">{variableReference("user", value.name)}</code>
                <span class="truncate text-right tabular-nums">{String(value.value)}</span>
                <button
                  type="button"
                  class="danger-button"
                  aria-label={`Remove ${value.name}`}
                  title={`Remove ${value.name}`}
                  disabled={store.isSaving()}
                  onClick={() => void store.removeUserValue(value.name)}
                >
                  <FiTrash2 class="size-4" />
                </button>
              </div>
            )}
          </For>
          <CreateUserValue />
        </div>
      </section>

      <Show when={needle() !== "" && userValues().length === 0 && groups().length === 0}>
        <p class="surface empty">Nothing matches that.</p>
      </Show>

      <For each={groups()}>
        {group => (
          <section>
            <div class="section-head">
              <h2 class="label-sm">{group.title}</h2>
              <code class="mono">{group.integrationId}</code>
              <span class="ml-auto text-muted tabular-nums">{group.entries.length}</span>
            </div>
            <div class="surface rows">
              <For each={group.entries}>
                {entry => (
                  <div class="row py-2">
                    <code class="mono min-w-0 flex-1 truncate">
                      {variableReference(group.integrationId, entry.name)}
                    </code>
                    <span class="max-w-[50%] truncate text-right tabular-nums">{entry.rendered}</span>
                  </div>
                )}
              </For>
            </div>
          </section>
        )}
      </For>
    </div>
  );
};
