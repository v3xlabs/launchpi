import * as Dialog from "@kobalte/core/dialog";
import { TbFillCircleX as TbX } from "solid-icons/tb";
import { Component, createMemo, createSignal, For, JSX, onCleanup, onMount, Show } from "solid-js";

import { Control } from "../api/inventory";
import { ControlTemplate, Preset } from "../api/presets";
import { TextField } from "../components/fields";
import { KeyImage } from "../components/KeyImage";
import { useInventory } from "../context/InventoryContext";

type Section = { integrationId: string; title: string; presets: Preset[]; };

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
      { rootMargin: "200px" },
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

  const sections = createMemo<Section[]>(() => {
    const needle = search().trim()
      .toLowerCase();
    const matching = (preset: Preset) =>
      needle === ""
      || preset.name.toLowerCase().includes(needle)
      || preset.category.toLowerCase().includes(needle);

    return store
      .presets()
      .map(instance => ({
        integrationId: instance.integration_id,
        title: instance.display_name,
        presets: instance.presets.filter(matching),
      }))
      .filter(section => section.presets.length > 0);
  });

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
      }}
    >
      <Dialog.Trigger as="div" class="contents">{properties.trigger}</Dialog.Trigger>
      <Dialog.Portal>
        <Dialog.Overlay class="dialog-overlay" />
        <div class="dialog-positioner">
          <Dialog.Content class="dialog-content" data-size="wide">
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
              <div class="max-h-[62vh] overflow-y-auto overflow-x-hidden">
                <Show
                  when={sections().length > 0}
                  fallback={<p class="empty">No plugin is offering a preset.</p>}
                >
                  <For each={sections()}>
                    {section => (
                      <section class="preset-section">
                        <p class="preset-heading">{section.title}</p>
                        <div class="preset-grid">
                          <For each={section.presets}>
                            {preset => (
                              <button
                                type="button"
                                class="preset-tile"
                                title={preset.description ?? preset.category}
                                onClick={() => choose(preset)}
                              >
                                <PresetKey control={asControl(section.integrationId, preset)} />
                                <span class="preset-name">{preset.name}</span>
                              </button>
                            )}
                          </For>
                        </div>
                      </section>
                    )}
                  </For>
                </Show>
              </div>
            </div>
          </Dialog.Content>
        </div>
      </Dialog.Portal>
    </Dialog.Root>
  );
};
