import * as AlertDialog from "@kobalte/core/alert-dialog";
import { TbFillCircleX as TbX } from "solid-icons/tb";
import { Component, createMemo, createSignal, For, JSX, Show } from "solid-js";

import { displayName, Panel } from "../api/inventory";
import { StatusDot } from "../components/StatusDot";
import { useInventory } from "../context/InventoryContext";

export const DeletePanelDialog: Component<{
  panel: Panel;
  trigger: JSX.Element;
  onDeleted?: () => void;
}> = (properties) => {
  const store = useInventory();
  const [isOpen, setIsOpen] = createSignal(false);

  const assignedDevices = createMemo(() =>
    store.inventory().devices.filter(device => device.active_panel_id === properties.panel.panel_id),
  );

  const confirm = async () => {
    const isDeleted = await store.deletePanel(properties.panel.panel_id);

    if (!isDeleted) return;

    setIsOpen(false);
    properties.onDeleted?.();
  };

  return (
    <AlertDialog.Root open={isOpen()} onOpenChange={setIsOpen}>
      <AlertDialog.Trigger as="div" class="contents">
        {properties.trigger}
      </AlertDialog.Trigger>
      <AlertDialog.Portal>
        <AlertDialog.Overlay class="dialog-overlay" />
        <div class="dialog-positioner">
          <AlertDialog.Content class="dialog-content">
            <div class="dialog-head">
              <AlertDialog.Title class="dialog-title">
                Delete
                {" "}
                {properties.panel.name}
              </AlertDialog.Title>
              <AlertDialog.CloseButton
                class="icon-button"
                aria-label="Close delete panel dialog"
              >
                <TbX class="h-4 w-4" />
              </AlertDialog.CloseButton>
            </div>
            <div class="dialog-body">
              <div class="flex flex-wrap items-center gap-1.5">
                <span class="chip">
                  {properties.panel.controls.length}
                  {" "}
                  controls
                </span>
                <span class="chip chip-muted">
                  {properties.panel.layout.columns}
                  {" x "}
                  {properties.panel.layout.rows}
                </span>
              </div>
              <Show when={assignedDevices().length > 0}>
                <div class="grid gap-1">
                  <p class="field-label">Devices losing this panel</p>
                  <div class="rows">
                    <For each={assignedDevices()}>
                      {device => (
                        <div class="row row-main">
                          <StatusDot status={device.status} />
                          <span class="row-title min-w-0 flex-1">
                            {displayName(device.name)}
                          </span>
                        </div>
                      )}
                    </For>
                  </div>
                </div>
              </Show>
            </div>
            <div class="dialog-actions">
              <AlertDialog.CloseButton class="secondary-button" type="button">
                Cancel
              </AlertDialog.CloseButton>
              <button
                type="button"
                class="destructive-button"
                onClick={() => void confirm()}
                disabled={store.isSaving()}
              >
                {store.isSaving() ? "Deleting..." : "Delete panel"}
              </button>
            </div>
          </AlertDialog.Content>
        </div>
      </AlertDialog.Portal>
    </AlertDialog.Root>
  );
};
