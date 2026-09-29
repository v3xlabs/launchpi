import * as Dialog from "@kobalte/core/dialog";
import { FiCheck, FiX } from "solid-icons/fi";
import { Component, createSignal, For, Show } from "solid-js";

import { DeviceKind, deviceKindLabels } from "../api/inventory";
import { DeviceImage } from "../components/DeviceImage";
import { InfoTip } from "../components/InfoTip";
import { useInventory } from "../context/InventoryContext";

export const AddDeviceDialog: Component = () => {
  const store = useInventory();
  const [isOpen, setIsOpen] = createSignal(false);
  const [name, setName] = createSignal("");
  const [kind, setKind] = createSignal<DeviceKind>("studio");
  const [host, setHost] = createSignal("");
  const [port, setPort] = createSignal("5343");

  const reset = () => {
    setName("");
    setKind("studio");
    setHost("");
    setPort("5343");
  };

  const submit = async (event: SubmitEvent) => {
    event.preventDefault();
    const parsedPort = Number(port());
    const created = await store.addDevice({
      name: name().trim() || "Network device",
      host: host().trim(),
      port: Number.isSafeInteger(parsedPort) && parsedPort > 0 ? parsedPort : undefined,
      serial_number: null,
      kind: kind(),
    });

    if (created) {
      reset();
      setIsOpen(false);
    }
  };

  return (
    <Dialog.Root open={isOpen()} onOpenChange={setIsOpen}>
      <Dialog.Trigger class="secondary-button">Add</Dialog.Trigger>
      <Dialog.Portal>
        <Dialog.Overlay class="dialog-overlay" />
        <div class="dialog-positioner">
          <Dialog.Content class="dialog-content">
            <div class="dialog-head">
              <Dialog.Title class="dialog-title">Add by address</Dialog.Title>
              <InfoTip>For a device that discovery cannot reach, for example a dock on another subnet.</InfoTip>
              <Dialog.CloseButton class="icon-button ml-auto" aria-label="Close">
                <FiX class="size-4" />
              </Dialog.CloseButton>
            </div>
            <form onSubmit={submit}>
              <div class="dialog-body">
                <div class="grid gap-1.5" role="group" aria-labelledby="add-device-model">
                  <span id="add-device-model" class="font-medium text-soft">Model</span>
                  <div class="flex gap-1">
                    <For each={deviceKindLabels}>
                      {entry => (
                        <button
                          type="button"
                          aria-pressed={kind() === entry.value}
                          classList={{
                            "flex flex-1 items-center gap-3 rounded-control px-3 py-2.5 text-left transition-colors": true,
                            "bg-raised": kind() === entry.value,
                            "hover:bg-raised/60": kind() !== entry.value,
                          }}
                          onClick={() => setKind(entry.value)}
                        >
                          <DeviceImage model={entry.label} class="h-8 w-12" />
                          <span
                            classList={{
                              "flex-1 font-medium": true,
                              "text-slate-900 dark:text-slate-100": kind() === entry.value,
                              "text-soft": kind() !== entry.value,
                            }}
                          >
                            {entry.label}
                          </span>
                          <Show when={kind() === entry.value}>
                            <FiCheck class="size-4 shrink-0" />
                          </Show>
                        </button>
                      )}
                    </For>
                  </div>
                </div>
                <label class="field-label">
                  Name
                  <input
                    class="field-input"
                    value={name()}
                    onInput={event => setName(event.currentTarget.value)}
                    placeholder="Control room dock"
                  />
                </label>
                <div class="grid grid-cols-[minmax(0,1fr)_6rem] gap-3">
                  <label class="field-label">
                    Host
                    <input
                      class="field-input"
                      value={host()}
                      onInput={event => setHost(event.currentTarget.value)}
                      placeholder="192.168.1.42"
                      required
                    />
                  </label>
                  <label class="field-label">
                    Port
                    <input
                      class="field-input tabular-nums"
                      value={port()}
                      onInput={event => setPort(event.currentTarget.value)}
                      inputMode="numeric"
                    />
                  </label>
                </div>
              </div>
              <div class="dialog-actions">
                <Dialog.CloseButton class="secondary-button" type="button">
                  Cancel
                </Dialog.CloseButton>
                <button class="primary-button" type="submit" disabled={store.isSaving()}>
                  {store.isSaving() ? "Adding..." : "Add device"}
                </button>
              </div>
            </form>
          </Dialog.Content>
        </div>
      </Dialog.Portal>
    </Dialog.Root>
  );
};
