import * as Dialog from "@kobalte/core/dialog";
import { useNavigate } from "@tanstack/solid-router";
import { TbFillCircleX as TbX } from "solid-icons/tb";
import { Component, createSignal, For, JSX } from "solid-js";

import { Capabilities, capabilityLabels, emptyCapabilities } from "../api/inventory";
import { useInventory } from "../context/InventoryContext";

export const CreatePanelDialog: Component<{ trigger: JSX.Element; }> = (properties) => {
  const store = useInventory();
  const navigate = useNavigate();
  const [isOpen, setIsOpen] = createSignal(false);
  const [name, setName] = createSignal("");
  const [columns, setColumns] = createSignal("4");
  const [rows, setRows] = createSignal("3");
  const [capabilities, setCapabilities] = createSignal<Capabilities>(emptyCapabilities);

  const reset = () => {
    setName("");
    setColumns("4");
    setRows("3");
    setCapabilities(emptyCapabilities);
  };

  const submit = async (event: SubmitEvent) => {
    event.preventDefault();
    const panel = await store.createPanel({
      name: name().trim(),
      layout: { columns: Number(columns()), rows: Number(rows()) },
      capabilities: capabilities(),
      controls: [],
      dial_colors: [],
      dial_ring_levels: [],
    });

    if (panel) {
      reset();
      setIsOpen(false);
      navigate({ to: "/panels/$panelId", params: { panelId: panel.panel_id } });
    }
  };

  return (
    <Dialog.Root open={isOpen()} onOpenChange={setIsOpen}>
      <Dialog.Trigger as="div" class="contents">
        {properties.trigger}
      </Dialog.Trigger>
      <Dialog.Portal>
        <Dialog.Overlay class="dialog-overlay" />
        <div class="dialog-positioner">
          <Dialog.Content class="dialog-content">
            <div class="dialog-head">
              <div>
                <Dialog.Title class="dialog-title">Create panel</Dialog.Title>
                <Dialog.Description class="dialog-description">
                  Define the grid and the capabilities a device must support. A 16 x 2 panel gets
                  the Studio dials.
                </Dialog.Description>
              </div>
              <Dialog.CloseButton class="icon-button" aria-label="Close create panel dialog">
                <TbX class="h-4 w-4" />
              </Dialog.CloseButton>
            </div>
            <form onSubmit={submit}>
              <div class="dialog-body">
                <label class="field-label">
                  Panel name
                  <input
                    class="field-input"
                    value={name()}
                    onInput={event => setName(event.currentTarget.value)}
                    placeholder="Playback"
                    required
                  />
                </label>
                <div class="grid grid-cols-2 gap-3">
                  <label class="field-label">
                    Columns
                    <input
                      class="field-input"
                      type="number"
                      min="1"
                      value={columns()}
                      onInput={event => setColumns(event.currentTarget.value)}
                      required
                    />
                  </label>
                  <label class="field-label">
                    Rows
                    <input
                      class="field-input"
                      type="number"
                      min="1"
                      value={rows()}
                      onInput={event => setRows(event.currentTarget.value)}
                      required
                    />
                  </label>
                </div>
                <fieldset class="grid gap-1">
                  <legend class="field-label">Required capabilities</legend>
                  <div class="mt-1 grid grid-cols-2 gap-1.5 sm:grid-cols-3">
                    <For each={capabilityLabels}>
                      {({ key, label }) => (
                        <label class="check-tile">
                          <input
                            type="checkbox"
                            checked={capabilities()[key]}
                            onInput={event =>
                              setCapabilities(current => ({
                                ...current,
                                [key]: event.currentTarget.checked,
                              }))}
                          />
                          {label}
                        </label>
                      )}
                    </For>
                  </div>
                </fieldset>
              </div>
              <div class="dialog-actions">
                <Dialog.CloseButton class="secondary-button" type="button">
                  Cancel
                </Dialog.CloseButton>
                <button class="primary-button" type="submit" disabled={store.isSaving()}>
                  {store.isSaving() ? "Creating..." : "Create panel"}
                </button>
              </div>
            </form>
          </Dialog.Content>
        </div>
      </Dialog.Portal>
    </Dialog.Root>
  );
};
