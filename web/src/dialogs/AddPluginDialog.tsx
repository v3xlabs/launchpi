import * as Dialog from "@kobalte/core/dialog";
import { useNavigate } from "@tanstack/solid-router";
import { TbFillCircleX as TbX } from "solid-icons/tb";
import { Component, createMemo, createSignal, For, JSX, Show } from "solid-js";
import { createStore } from "solid-js/store";

import { coerceConfigValue, ConfigField, PluginManifest } from "../api/plugins";
import { ConfigFieldInput, TextField } from "../components/fields";
import { useInventory } from "../context/InventoryContext";

/** Derived from the type, so `http` + `weather` reads back as `http.weather` before you commit. */
const suggestedName = (existing: string[], pluginType: string): string => {
  const taken = new Set(existing);

  if (!taken.has(`${pluginType}.default`)) return "default";

  for (let index = 2; index < 100; index += 1) {
    if (!taken.has(`${pluginType}.instance-${index}`)) return `instance-${index}`;
  }

  return "";
};

export const AddPluginDialog: Component<{ trigger: JSX.Element; }> = (properties) => {
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
      <Dialog.Trigger as="div" class="contents">{properties.trigger}</Dialog.Trigger>
      <Dialog.Portal>
        <Dialog.Overlay class="dialog-overlay" />
        <div class="dialog-positioner">
          <Dialog.Content class="dialog-content">
            <div class="dialog-head">
              <Dialog.Title class="dialog-title">
                {chosen()?.display_name ?? "Add a plugin"}
              </Dialog.Title>
              <Dialog.CloseButton class="icon-button" aria-label="Close">
                <TbX class="h-3.5 w-3.5" />
              </Dialog.CloseButton>
            </div>

            <Show
              when={chosen()}
              fallback={(
                <div class="dialog-body">
                  <Dialog.Description class="dialog-description">
                    Pick what to connect to. Each one can be added more than once.
                  </Dialog.Description>
                  <TextField
                    label="Search"
                    value={search()}
                    placeholder="http, music, lights..."
                    onChange={setSearch}
                  />
                  <div class="rows max-h-72 overflow-y-auto">
                    <Show
                      when={matches().length > 0}
                      fallback={<p class="empty">Nothing matches that.</p>}
                    >
                      <For each={matches()}>
                        {manifest => (
                          <button
                            type="button"
                            class="row w-full text-left"
                            onClick={() => choose(manifest)}
                          >
                            <span class="row-main">
                              <span class="min-w-0 flex-1">
                                <span class="row-title block">{manifest.display_name}</span>
                                <span class="row-meta block">{manifest.description}</span>
                              </span>
                            </span>
                            <span class="chip chip-muted">
                              {manifest.actions.length}
                              {" "}
                              actions
                            </span>
                          </button>
                        )}
                      </For>
                    </Show>
                  </div>
                </div>
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
                      onChange={setName}
                    />
                    <p class="hint">
                      Referenced as
                      {" "}
                      <span class="mono">
                        {`$(${manifest().plugin_type}.${name().trim() || "name"}:value)`}
                      </span>
                    </p>
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
                      Add plugin
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
