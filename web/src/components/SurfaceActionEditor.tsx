import { Component, createMemo, For, Show } from "solid-js";

import { Action, displayName } from "../api/inventory";
import { useInventory } from "../context/InventoryContext";
import { SelectField } from "./fields";

type SurfaceAction = Extract<Action, { type: "set_surface_display" | "set_surface_brightness"; }>;
type SurfaceActionEditorProperties = {
  action: SurfaceAction;
  onMutate: (mutate: (action: SurfaceAction) => void) => void;
};

export const SurfaceActionEditor: Component<SurfaceActionEditorProperties> = (properties) => {
  const store = useInventory();
  const targets = createMemo(() => store.inventory().devices.filter(device =>
    device.parent_surface_id === null
    && (device.capabilities.supports_brightness || device.model === "Stream Deck Network Dock")));
  const targetOptions = createMemo(() => {
    const available = targets().map(device => ({
      surfaceId: device.surface_id,
      label: displayName(device.name),
    }));
    const availableIds = new Set(available.map(({ surfaceId }) => surfaceId));

    return [
      ...available,
      ...properties.action.surface_ids
        .filter(surfaceId => !availableIds.has(surfaceId))
        .map(surfaceId => ({ surfaceId, label: `${surfaceId} (unavailable)` })),
    ];
  });

  return (
    <>
      <label class="check-tile">
        <input
          type="checkbox"
          checked={properties.action.include_triggering_surface}
          onInput={(event) => {
            const isIncluded = event.currentTarget.checked;

            properties.onMutate((action) => {
              action.include_triggering_surface = isIncluded;
            });
          }}
        />
        Triggering surface
      </label>
      <fieldset class="grid gap-1">
        <legend class="field-label">Other surfaces</legend>
        <Show when={targetOptions().length > 0} fallback={<p class="hint">No eligible Stream Decks.</p>}>
          <div class="grid gap-1">
            <For each={targetOptions()}>
              {target => (
                <label class="check-tile">
                  <input
                    type="checkbox"
                    checked={properties.action.surface_ids.includes(target.surfaceId)}
                    onInput={(event) => {
                      const isIncluded = event.currentTarget.checked;

                      properties.onMutate((action) => {
                        if (isIncluded) {
                          if (!action.surface_ids.includes(target.surfaceId)) {
                            action.surface_ids.push(target.surfaceId);
                          }
                        }
                        else {
                          action.surface_ids = action.surface_ids.filter(
                            surfaceId => surfaceId !== target.surfaceId,
                          );
                        }
                      });
                    }}
                  />
                  {target.label}
                </label>
              )}
            </For>
          </div>
        </Show>
      </fieldset>
      <Show when={properties.action.type === "set_surface_display"}>
        <SelectField
          label="Display state"
          value={properties.action.type === "set_surface_display" && properties.action.is_display_off
            ? "off"
            : "awake"}
          options={[
            { value: "off", label: "Display off" },
            { value: "awake", label: "Awake" },
          ]}
          onChange={value => properties.onMutate((action) => {
            if (action.type === "set_surface_display") action.is_display_off = value === "off";
          })}
        />
      </Show>
      <Show when={properties.action.type === "set_surface_brightness"}>
        <label class="field-label">
          Brightness
          <input
            class="field-input"
            type="number"
            min="0"
            max="100"
            value={properties.action.type === "set_surface_brightness" ? properties.action.brightness : 100}
            onChange={(event) => {
              const value = event.currentTarget.valueAsNumber;
              const brightness = Number.isNaN(value)
                ? 100
                : Math.min(100, Math.max(0, Math.round(value)));

              event.currentTarget.value = String(brightness);

              properties.onMutate((action) => {
                if (action.type === "set_surface_brightness") action.brightness = brightness;
              });
            }}
          />
        </label>
      </Show>
    </>
  );
};
