import * as AlertDialog from "@kobalte/core/alert-dialog";
import { FiX } from "solid-icons/fi";
import { Component, createMemo, For, Show } from "solid-js";

import { displayName, layoutLabel, Panel } from "../api/inventory";
import { StatusLabel } from "../components/StatusDot";
import { useInventory } from "../context/InventoryContext";
import { countOf } from "../utils/plural";

/** Opened from a menu item, so the caller owns the open state rather than a trigger living in here. */
export const DeletePanelDialog: Component<{
  panel: Panel;
  isOpen: boolean;
  onOpenChange: (isOpen: boolean) => void;
  onDeleted?: () => void;
}> = (properties) => {
  const store = useInventory();

  const assignedDevices = createMemo(() =>
    store.inventory().devices.filter(device => device.active_panel_id === properties.panel.panel_id),
  );

  const confirm = async () => {
    const isDeleted = await store.deletePanel(properties.panel.panel_id);

    if (!isDeleted) return;

    properties.onOpenChange(false);
    properties.onDeleted?.();
  };

  return (
    <AlertDialog.Root open={properties.isOpen} onOpenChange={properties.onOpenChange}>
      <AlertDialog.Portal>
        <AlertDialog.Overlay class="dialog-overlay" />
        <div class="dialog-positioner">
          <AlertDialog.Content class="dialog-content">
            <div class="dialog-head">
              <AlertDialog.Title class="dialog-title">
                {`Delete ${properties.panel.name}?`}
              </AlertDialog.Title>
              <AlertDialog.CloseButton class="icon-button ml-auto" aria-label="Close">
                <FiX class="size-4" />
              </AlertDialog.CloseButton>
            </div>
            <div class="dialog-body">
              <AlertDialog.Description class="text-muted tabular-nums">
                {`${countOf(properties.panel.controls.length, "control")}, ${layoutLabel(properties.panel.layout)}.`}
              </AlertDialog.Description>
              <Show when={assignedDevices().length > 0}>
                <div class="grid gap-1.5">
                  <p>Devices showing it switch to no panel:</p>
                  <ul class="grid gap-1">
                    <For each={assignedDevices()}>
                      {device => (
                        <li>
                          <StatusLabel status={device.status} label={displayName(device.name)} />
                        </li>
                      )}
                    </For>
                  </ul>
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
