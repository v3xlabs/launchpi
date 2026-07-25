import * as Dialog from "@kobalte/core/dialog";
import { TbFillCircleX as TbX } from "solid-icons/tb";
import { Component, createMemo, createSignal, For, JSX, Show } from "solid-js";

import { Control } from "../api/inventory";
import { ControlTemplate, Preset } from "../api/presets";
import { TextField } from "../components/fields";
import { KeyImage } from "../components/KeyImage";
import { useInventory } from "../context/InventoryContext";

type Row = { key: string; instance: string; category: string; preset: Preset; };

/** `KeyImage` reads only the two states, so a preset can be previewed without being placed. */
const asControl = (row: Row): Control => ({
  control_id: row.key,
  position: { column: 0, row: 0 },
  ...row.preset.control,
});

export const PresetPickerDialog: Component<{
  trigger: JSX.Element;
  onChoose: (template: ControlTemplate) => void;
}> = (properties) => {
  const store = useInventory();
  const [isOpen, setIsOpen] = createSignal(false);
  const [search, setSearch] = createSignal("");

  const matches = createMemo<Row[]>(() => {
    const needle = search().trim()
      .toLowerCase();
    const matching = (row: Row) =>
      needle === ""
      || row.preset.name.toLowerCase().includes(needle)
      || row.category.toLowerCase().includes(needle)
      || row.instance.toLowerCase().includes(needle);

    return store
      .presets()
      .flatMap(instance =>
        instance.presets.map(preset => ({
          key: `${instance.integration_id}:${preset.preset_id}`,
          instance: instance.display_name,
          category: preset.category,
          preset,
        })),
      )
      .filter(matching);
  });

  const choose = (row: Row) => {
    properties.onChoose(row.preset.control);
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
          <Dialog.Content class="dialog-content">
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
              <div class="rows max-h-72 overflow-y-auto">
                <Show
                  when={matches().length > 0}
                  fallback={<p class="empty">No plugin is offering a preset.</p>}
                >
                  <For each={matches()}>
                    {row => (
                      <button type="button" class="row w-full text-left" onClick={() => choose(row)}>
                        <span class="row-main">
                          <span class="relative h-9 w-9 shrink-0 overflow-hidden rounded bg-slate-800">
                            <KeyImage control={asControl(row)} isPressed={false} />
                          </span>
                          <span class="min-w-0 flex-1">
                            <span class="row-title block">{row.preset.name}</span>
                            <span class="row-meta block">
                              {row.instance}
                              {" - "}
                              {row.category}
                            </span>
                          </span>
                        </span>
                      </button>
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
