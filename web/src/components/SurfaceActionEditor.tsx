import { Component, createMemo, For, Index, Show } from "solid-js";

import { Action, displayName } from "../api/inventory";
import { useInventory } from "../context/InventoryContext";

type SurfaceAction = Extract<Action, { type: "set_surface_display" | "set_surface_brightness"; }>;
type SurfaceActionEditorProperties = {
  action: SurfaceAction;
  onMutate: (mutate: (action: SurfaceAction) => void) => void;
};
type Target = {
  label: string;
  isChosen: boolean;
  choose: (isChosen: boolean) => void;
};

/**
 * The surface a key is pressed on and the surfaces named by id are one list here, because from the
 * key's point of view they are one question: which displays does this button change? Splitting
 * them into a checkbox and a fieldset made "also sleep the other deck" read as a second feature.
 */
export const SurfaceActionEditor: Component<SurfaceActionEditorProperties> = (properties) => {
  const store = useInventory();
  const others = createMemo(() =>
    store.inventory().devices.filter(device =>
      device.parent_surface_id === null
      && (device.capabilities.supports_brightness || device.model === "Stream Deck Network Dock")));

  const setNamed = (surfaceId: string, isChosen: boolean) =>
    properties.onMutate((action) => {
      action.surface_ids = isChosen
        ? [...action.surface_ids.filter(entry => entry !== surfaceId), surfaceId]
        : action.surface_ids.filter(entry => entry !== surfaceId);
    });

  const targets = createMemo<Target[]>(() => {
    const named = new Set(others().map(device => device.surface_id));

    return [
      {
        label: "This surface",
        isChosen: properties.action.include_triggering_surface,
        choose: (isChosen: boolean) =>
          properties.onMutate((action) => {
            action.include_triggering_surface = isChosen;
          }),
      },
      ...others().map(device => ({
        label: displayName(device.name),
        isChosen: properties.action.surface_ids.includes(device.surface_id),
        choose: (isChosen: boolean) => setNamed(device.surface_id, isChosen),
      })),
      ...properties.action.surface_ids
        .filter(surfaceId => !named.has(surfaceId))
        .map(surfaceId => ({
          label: `${surfaceId} (unavailable)`,
          isChosen: true,
          choose: (isChosen: boolean) => setNamed(surfaceId, isChosen),
        })),
    ];
  });

  return (
    <>
      <Show when={properties.action.type === "set_surface_display"}>
        <div class="flex gap-1" role="group" aria-label="Display state">
          <For
            each={[
              { isDisplayOff: true, label: "Sleep" },
              { isDisplayOff: false, label: "Wake" },
            ]}
          >
            {option => (
              <button
                type="button"
                class="toggle-chip"
                aria-pressed={properties.action.type === "set_surface_display"
                  && properties.action.is_display_off === option.isDisplayOff}
                onClick={() => properties.onMutate((action) => {
                  if (action.type === "set_surface_display") {
                    action.is_display_off = option.isDisplayOff;
                  }
                })}
              >
                {option.label}
              </button>
            )}
          </For>
        </div>
      </Show>

      <Show when={properties.action.type === "set_surface_brightness" && properties.action}>
        {brightness => (
          <label class="field-label">
            <span class="flex items-center justify-between gap-2">
              Brightness
              <span class="font-normal text-muted tabular-nums">
                {brightness().type === "set_surface_brightness" ? brightness().brightness : 100}
                %
              </span>
            </span>
            <input
              class="range-input"
              type="range"
              min="0"
              max="100"
              value={brightness().type === "set_surface_brightness" ? brightness().brightness : 100}
              onInput={(event) => {
                const value = event.currentTarget.valueAsNumber;

                properties.onMutate((action) => {
                  if (action.type === "set_surface_brightness") action.brightness = value;
                });
              }}
            />
          </label>
        )}
      </Show>

      <fieldset class="grid gap-1">
        <legend class="field-label">Applies to</legend>
        <div class="mt-1 grid gap-1">
          <Index each={targets()}>
            {target => (
              <label class="check-tile">
                <input
                  type="checkbox"
                  checked={target().isChosen}
                  onInput={event => target().choose(event.currentTarget.checked)}
                />
                {target().label}
              </label>
            )}
          </Index>
        </div>
      </fieldset>
    </>
  );
};
