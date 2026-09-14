import { Component } from "solid-js";

import { Device } from "../api/inventory";
import { useInventory } from "../context/InventoryContext";

export const DeviceDisplayControls: Component<{ device: Device; }> = (properties) => {
  const store = useInventory();

  return (
    <div class="card">
      <div class="card-head">
        <p class="card-title">Display</p>
        <span class="chip">
          {properties.device.is_display_off ? "Off" : `${properties.device.brightness}%`}
        </span>
      </div>
      <div class="card-body">
        <label class="field-label">
          Awake brightness
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
        <button
          type="button"
          classList={{
            "primary-button": properties.device.is_display_off,
            "secondary-button": !properties.device.is_display_off,
          }}
          disabled={store.isSaving()}
          onClick={() => void store.setSurfaceDisplay(
            [properties.device.surface_id],
            !properties.device.is_display_off,
          )}
        >
          {properties.device.is_display_off ? "Wake" : "Sleep"}
        </button>
      </div>
    </div>
  );
};
