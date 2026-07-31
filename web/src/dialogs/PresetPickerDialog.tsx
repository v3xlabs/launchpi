import * as Dialog from "@kobalte/core/dialog";
import { TbFillCircleX as TbX } from "solid-icons/tb";
import { Component, createMemo, createSignal, For, JSX, onCleanup, onMount, Show } from "solid-js";

import { Control } from "../api/inventory";
import { ControlTemplate, Preset } from "../api/presets";
import { TextField } from "../components/fields";
import { KeyImage } from "../components/KeyImage";
import { useInventory } from "../context/InventoryContext";

type Category = { title: string; presets: Preset[]; };
type Section = { integrationId: string; title: string; categories: Category[]; presetCount: number; };

/** `KeyImage` reads only the two states, so a preset can be previewed without being placed. */
const asControl = (integrationId: string, preset: Preset): Control => ({
  control_id: `${integrationId}:${preset.preset_id}`,
  position: { column: 0, row: 0 },
  ...preset.control,
});

/**
 * Only draws while on screen. Every preview costs the daemon a render, and an installation with
 * hundreds of entities would otherwise ask for all of them at once and evict its own blob URLs out
 * from under the tiles still showing them.
 */
const PresetKey: Component<{ control: Control; }> = (properties) => {
  const [isVisible, setIsVisible] = createSignal(false);
  let host: HTMLSpanElement | undefined;
  const holdHost = (element: HTMLSpanElement) => (host = element);

  onMount(() => {
    const observer = new IntersectionObserver(
      entries => setIsVisible(entries.some(entry => entry.isIntersecting)),
      { root: host?.closest<HTMLDivElement>(".preset-list"), rootMargin: "96px" },
    );

    if (host !== undefined) observer.observe(host);

    onCleanup(() => observer.disconnect());
  });

  return (
    <span class="preset-key" ref={holdHost}>
      <Show when={isVisible()}>
        <KeyImage control={properties.control} isPressed={false} />
      </Show>
    </span>
  );
};

export const PresetPickerDialog: Component<{
  trigger: JSX.Element;
  onChoose: (template: ControlTemplate) => void;
}> = (properties) => {
  const store = useInventory();
  const [isOpen, setIsOpen] = createSignal(false);
  const [search, setSearch] = createSignal("");
  const [selectedIntegrationId, setSelectedIntegrationId] = createSignal<string | null>(null);
  let presetList: HTMLDivElement | undefined;

  const sections = createMemo<Section[]>(() => {
    const needle = search().trim()
      .toLowerCase();
    const matching = (preset: Preset) =>
      needle === ""
      || preset.name.toLowerCase().includes(needle)
      || preset.category.toLowerCase().includes(needle);

    return store
      .presets()
      .map((instance): Section => {
        const categories: Category[] = [];
        const matchingPresets = instance.presets.filter(matching);

        for (const preset of matchingPresets) {
          const category = categories.find(({ title }) => title === preset.category);

          if (category === undefined) {
            categories.push({ title: preset.category, presets: [preset] });
          }
          else {
            category.presets.push(preset);
          }
        }

        return {
          integrationId: instance.integration_id,
          title: instance.display_name,
          categories,
          presetCount: matchingPresets.length,
        };
      })
      .filter(section => section.categories.length > 0);
  });
  const selectedSection = createMemo(
    () => sections().find(section => section.integrationId === selectedIntegrationId()) ?? sections()[0],
  );

  const selectIntegration = (integrationId: string) => {
    setSelectedIntegrationId(integrationId);
    presetList?.scrollTo({ top: 0 });
  };

  const choose = (preset: Preset) => {
    properties.onChoose(preset.control);
    setIsOpen(false);
    setSearch("");
  };

  return (
    <Dialog.Root
      open={isOpen()}
      onOpenChange={(open) => {
        setIsOpen(open);

        if (!open) setSearch("");
        else setSelectedIntegrationId(sections()[0]?.integrationId ?? null);
      }}
    >
      <Dialog.Trigger as="div" class="contents">{properties.trigger}</Dialog.Trigger>
      <Dialog.Portal>
        <Dialog.Overlay class="dialog-overlay" />
        <div class="dialog-positioner">
          <Dialog.Content class="dialog-content" data-size="wide" data-preset-picker>
            <div class="dialog-head">
              <Dialog.Title class="dialog-title">Presets</Dialog.Title>
              <Dialog.CloseButton class="icon-button" aria-label="Close">
                <TbX class="h-3.5 w-3.5" />
              </Dialog.CloseButton>
            </div>
            <div class="dialog-body">
              <Dialog.Description class="dialog-description">
                Replaces this key's label, image, colours and bindings.
              </Dialog.Description>
              <TextField
                label="Search"
                value={search()}
                placeholder="member, lights, channel..."
                onChange={setSearch}
              />
              <div class="preset-picker">
                <Show
                  when={sections().length > 0}
                  fallback={<p class="empty">No configured integration is offering a preset.</p>}
                >
                  <>
                    <nav class="preset-sidebar" aria-label="Preset integrations">
                      <For each={sections()}>
                        {section => (
                          <button
                            type="button"
                            class="preset-integration"
                            data-selected={section.integrationId === selectedSection()?.integrationId}
                            onClick={() => selectIntegration(section.integrationId)}
                          >
                            <span class="preset-integration-name">{section.integrationId}</span>
                            <span class="preset-integration-count">{section.presetCount}</span>
                          </button>
                        )}
                      </For>
                    </nav>
                    <div class="preset-list" ref={presetList}>
                      <Show when={selectedSection()}>
                        {section => (
                          <section class="preset-section">
                            <p class="preset-heading">{section().title}</p>
                            <div class="preset-categories">
                              <For each={section().categories}>
                                {category => (
                                  <section class="preset-category" aria-labelledby={`${section().integrationId}-${category.title}`}>
                                    <p class="preset-category-heading" id={`${section().integrationId}-${category.title}`}>
                                      {category.title}
                                    </p>
                                    <div class="preset-grid">
                                      <For each={category.presets}>
                                        {preset => (
                                          <button
                                            type="button"
                                            class="preset-tile"
                                            title={preset.description ?? preset.category}
                                            onClick={() => choose(preset)}
                                          >
                                            <PresetKey control={asControl(section().integrationId, preset)} />
                                            <span class="preset-name">{preset.name}</span>
                                          </button>
                                        )}
                                      </For>
                                    </div>
                                  </section>
                                )}
                              </For>
                            </div>
                          </section>
                        )}
                      </Show>
                    </div>
                  </>
                </Show>
              </div>
            </div>
          </Dialog.Content>
        </div>
      </Dialog.Portal>
    </Dialog.Root>
  );
};
