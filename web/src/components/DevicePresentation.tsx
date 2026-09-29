import { Component, createMemo, createResource, For, Show } from "solid-js";

import { Device, fetchDevicePresentation, Panel, panelDial } from "../api/inventory";
import { useInventory } from "../context/InventoryContext";
import { DialIndicator } from "./DialIndicator";
import { KeyImage } from "./KeyImage";
import { cellStyle, dialLevel, gridStyle, surfaceGrid } from "./PanelPreview";

export const DevicePresentation: Component<{ device: Device; panel: Panel; pressedKeys: Set<number>; }> = (
  properties,
) => {
  const store = useInventory();
  const presentationKey = () =>
    `${properties.device.surface_id}:${store.presentationVersionFor(properties.device.surface_id)}`;
  const [presentation] = createResource(presentationKey, () => fetchDevicePresentation(properties.device.surface_id));
  const grid = createMemo(() => surfaceGrid(properties.panel, properties.device.dials));

  return (
    <Show when={presentation.latest} fallback={<p class="hint px-4 py-3">Loading device presentation...</p>}>
      {(current) => {
        const controls = () => new Map(current().controls.map(entry => [entry.key_index, entry]));
        const cells = () => Array.from({ length: current().columns * current().rows }, (_, index) => index);

        return (
          <div class="stage">
            <div
              classList={{ "key-grid": true, "key-grid-dials": properties.device.dials.length > 0 }}
              style={gridStyle(properties.panel, grid())}
            >
              <For each={properties.device.dials}>
                {placement => (
                  <div
                    class="dial-cell"
                    style={cellStyle(grid(), placement.column, placement.row, placement.row_span)}
                  >
                    <Show when={panelDial(properties.panel, placement.index)}>
                      {dial => (
                        <DialIndicator
                          index={dial().index}
                          color={dial().color}
                          level={dialLevel(dial(), store.dialLevelsFor(properties.device.surface_id))}
                          isPressed={store.pressedDialsFor(properties.device.surface_id).has(dial().index)}
                        />
                      )}
                    </Show>
                  </div>
                )}
              </For>
              <For each={cells()}>
                {(keyIndex) => {
                  const entry = () => controls().get(keyIndex);
                  const isPressed = () => properties.pressedKeys.has(keyIndex);

                  return (
                    <div
                      classList={{
                        "key": true,
                        "bg-raised": entry() === undefined,
                        "key-pressed": isPressed(),
                        "key-dimmed": entry()?.is_dimmed ?? false,
                      }}
                      style={cellStyle(grid(), keyIndex % current().columns, Math.floor(keyIndex / current().columns))}
                    >
                      <Show when={entry()}>
                        {item => <KeyImage control={item().control} isPressed={isPressed()} />}
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
