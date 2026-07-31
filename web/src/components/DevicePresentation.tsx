import { Component, createResource, For, Show } from "solid-js";

import { Device, fetchDevicePresentation } from "../api/inventory";
import { useInventory } from "../context/InventoryContext";
import { KeyImage } from "./KeyImage";

export const DevicePresentation: Component<{ device: Device; pressedKeys: Set<number>; }> = (properties) => {
  const store = useInventory();
  const presentationKey = () =>
    `${properties.device.surface_id}:${store.presentationVersionFor(properties.device.surface_id)}`;
  const [presentation] = createResource(presentationKey, () => fetchDevicePresentation(properties.device.surface_id));

  return (
    <Show when={presentation.latest} fallback={<p class="hint">Loading device presentation...</p>}>
      {(current) => {
        const controls = () => new Map(current().controls.map(entry => [entry.key_index, entry]));
        const cells = () => Array.from({ length: current().columns * current().rows }, (_, index) => index);

        return (
          <div class="stage">
            <div
              class="key-grid"
              style={{ "--columns": String(current().columns), "--rows": String(current().rows) }}
            >
              <For each={cells()}>
                {(keyIndex) => {
                  const entry = () => controls().get(keyIndex);

                  return (
                    <div
                      classList={{
                        "key": true,
                        "key-pressed": properties.pressedKeys.has(keyIndex),
                        "key-dimmed": entry()?.is_dimmed ?? false,
                      }}
                    >
                      <Show when={entry()}>
                        {item => <KeyImage control={item().control} isPressed={properties.pressedKeys.has(keyIndex)} />}
                      </Show>
                    </div>
                  );
                }}
              </For>
            </div>
          </div>
        );
      }}
    </Show>
  );
};
