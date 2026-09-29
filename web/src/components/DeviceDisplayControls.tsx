import { FiMoon, FiSun } from "solid-icons/fi";
import { Component } from "solid-js";

import { Device } from "../api/inventory";
import { useInventory } from "../context/InventoryContext";

export const DeviceDisplayControls: Component<{ device: Device; }> = (properties) => {
  const store = useInventory();

  return (
    <div class="grid content-start gap-3 px-4 py-4">
      <label class="grid gap-3">
        <span class="flex items-center justify-between">
          <span class="font-medium text-soft">Brightness</span>
          <span class="text-muted tabular-nums">{`${properties.device.brightness}%`}</span>
        </span>
        <input
          class="range-input"
          type="range"
          min="0"
          max="100"
          value={properties.device.brightness}
          aria-valuetext={`${properties.device.brightness}%`}
          disabled={store.isSaving()}
          onChange={(event) => {
            const input = event.currentTarget;

            void store
              .setSurfaceBrightness([properties.device.surface_id], Number(input.value))
              .then((isSaved) => {
                if (!isSaved) input.value = String(properties.device.brightness);
              });
          }}
        />
      </label>
      <div>
        <button
          type="button"
          class="secondary-button"
          disabled={store.isSaving()}
          onClick={() => void store.setSurfaceDisplay(
            [properties.device.surface_id],
            !properties.device.is_display_off,
          )}
        >
          {properties.device.is_display_off
            ? (
                <>
                  <FiSun class="size-3.5" />
                  Wake display
                </>
              )
            : (
                <>
                  <FiMoon class="size-3.5" />
                  Sleep display
                </>
              )}
        </button>
      </div>
    </div>
  );
};
