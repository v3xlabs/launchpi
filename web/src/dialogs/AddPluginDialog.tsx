import * as Dialog from "@kobalte/core/dialog";
import { useNavigate } from "@tanstack/solid-router";
import { FiPlus, FiX } from "solid-icons/fi";
import { Component, createMemo, createSignal, For, Show } from "solid-js";
import { createStore } from "solid-js/store";

import { coerceConfigValue, ConfigField, PluginManifest } from "../api/plugins";
import { ConfigFieldInput, SearchField, TextField } from "../components/fields";
import { useInventory } from "../context/InventoryContext";
import { countOf } from "../utils/plural";

/** Derived from the type, so `http` + `weather` reads back as `http.weather` before you commit. */
const suggestedName = (existing: string[], pluginType: string): string => {
  const taken = new Set(existing);

  if (!taken.has(`${pluginType}.default`)) return "default";

  for (let index = 2; index < 100; index += 1) {
    if (!taken.has(`${pluginType}.instance-${index}`)) return `instance-${index}`;
  }

  return "";
};

export const AddPluginDialog: Component = () => {
  const store = useInventory();
  const navigate = useNavigate();
  const [isOpen, setIsOpen] = createSignal(false);
  const [search, setSearch] = createSignal("");
  const [chosen, setChosen] = createSignal<PluginManifest | null>(null);
  const [name, setName] = createSignal("");
  const [config, setConfig] = createStore<Record<string, unknown>>({});

  const matches = createMemo(() => {
    const needle = search().trim()
      .toLowerCase();

    return store.plugins().types.filter(manifest =>
      needle === ""
      || manifest.display_name.toLowerCase().includes(needle)
      || manifest.plugin_type.toLowerCase().includes(needle)
      || manifest.description.toLowerCase().includes(needle),
    );
  });

  const reset = () => {
    setSearch("");
    setChosen(null);
    setName("");
    setConfig((store) => {
      for (const key of Object.keys(store)) delete store[key];

      return store;
    });
  };

  const choose = (manifest: PluginManifest) => {
    setChosen(manifest);
    setName(suggestedName(
      store.plugins().instances.map(instance => instance.integration_id),
      manifest.plugin_type,
    ));
  };

  const submit = async (event: SubmitEvent) => {
    event.preventDefault();

    const manifest = chosen();

    if (manifest === null || name().trim() === "") return;

    const created = await store.createPluginInstance({
      plugin_type: manifest.plugin_type,
      name: name().trim(),
      display_name: null,
      config: { ...config },
    });

    if (!created) return;

    const integrationId = `${manifest.plugin_type}.${name().trim()}`;

    reset();
    setIsOpen(false);
    navigate({ to: "/plugins/$integrationId", params: { integrationId } });
  };

  const setField = (field: ConfigField, raw: string | boolean) =>
    setConfig(field.key, coerceConfigValue(field, raw));

  return (
    <Dialog.Root
      open={isOpen()}
      onOpenChange={(open) => {
        setIsOpen(open);

        if (!open) reset();
      }}
    >
      <Dialog.Trigger class="primary-button">
        <FiPlus class="size-4" />
        Add plugin
      </Dialog.Trigger>
      <Dialog.Portal>
        <Dialog.Overlay class="dialog-overlay" />
        <div class="dialog-positioner">
          <Dialog.Content class="dialog-content max-w-lg">
            <div class="dialog-head">
              <Dialog.Title class="dialog-title">
                {chosen()?.display_name ?? "Add plugin"}
              </Dialog.Title>
              <Dialog.CloseButton class="icon-button ml-auto" aria-label="Close">
                <FiX class="size-4" />
              </Dialog.CloseButton>
            </div>

            <Show
              when={chosen()}
              fallback={(
                <>
                  <div class="px-3 pt-1 pb-2">
                    <SearchField
                      label="Search plugins"
                      value={search()}
                      placeholder="Search plugins"
                      onChange={setSearch}
                    />
                  </div>
                  <div class="grid max-h-[60vh] grid-cols-[minmax(0,1fr)] gap-0.5 overflow-y-auto px-1 pb-2">
                    <Show
                      when={matches().length > 0}
                      fallback={<p class="empty">Nothing matches that.</p>}
                    >
                      <For each={matches()}>
                        {manifest => (
                          <button
                            type="button"
                            class="flex w-full items-center gap-3 rounded-control px-3 py-2 text-left transition-colors hover:bg-raised"
                            onClick={() => choose(manifest)}
                          >
                            <span class="min-w-0 flex-1">
                              <span class="row-title block">{manifest.display_name}</span>
                              <span class="row-meta block">{manifest.description}</span>
                            </span>
                            <span class="w-20 shrink-0 text-right text-muted tabular-nums">
                              {countOf(manifest.actions.length, "action")}
                            </span>
                          </button>
                        )}
                      </For>
                    </Show>
                  </div>
                </>
              )}
            >
              {manifest => (
                <form onSubmit={event => void submit(event)}>
                  <div class="dialog-body">
                    <Dialog.Description class="dialog-description">
                      {manifest().description}
                    </Dialog.Description>
                    <TextField
                      label="Instance name"
                      value={name()}
                      placeholder="default"
                      help={`Referenced as $(${manifest().plugin_type}.${name().trim() || "name"}:value)`}
                      onChange={setName}
                    />
                    <For each={manifest().config_schema}>
                      {field => (
                        <ConfigFieldInput
                          field={field}
                          value={config[field.key]}
                          onChange={raw => setField(field, raw)}
                        />
                      )}
                    </For>
                  </div>
                  <div class="dialog-actions">
                    <button type="button" class="secondary-button" onClick={() => setChosen(null)}>
                      Back
                    </button>
                    <button
                      type="submit"
                      class="primary-button"
                      disabled={store.isSaving() || name().trim() === ""}
                    >
                      {store.isSaving() ? "Adding..." : "Add plugin"}
                    </button>
                  </div>
                </form>
              )}
            </Show>
          </Dialog.Content>
        </div>
      </Dialog.Portal>
    </Dialog.Root>
  );
};
