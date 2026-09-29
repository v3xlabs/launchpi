import * as Dialog from "@kobalte/core/dialog";
import { useNavigate } from "@tanstack/solid-router";
import { FiX } from "solid-icons/fi";
import { createMemo, createSignal, For, ParentComponent, Show } from "solid-js";

import {
  Capabilities,
  Device,
  deviceGridLayout,
  DialPlacement,
  displayName,
  emptyCapabilities,
  GridLayout,
  layoutLabel,
  PanelDial,
} from "../api/inventory";
import { dialSide, newDialColor } from "../components/DialEditor";
import { DialIndicator } from "../components/DialIndicator";
import { NeedsField } from "../components/PanelInspector";
import { useInventory } from "../context/InventoryContext";
import { countOf } from "../utils/plural";

type DeviceLayout = { device: Device; layout: GridLayout; };

const SizeChoice: ParentComponent<{ isSelected: boolean; detail?: string; onSelect: () => void; }> = properties => (
  <button
    type="button"
    class="flex items-center gap-2 rounded-control px-3 py-2 text-left transition-colors"
    classList={{
      "bg-raised text-slate-900 dark:text-slate-100": properties.isSelected,
      "text-soft hover:bg-raised/60": !properties.isSelected,
    }}
    aria-pressed={properties.isSelected}
    onClick={properties.onSelect}
  >
    <span class="min-w-0 flex-1 truncate font-medium">{properties.children}</span>
    <Show when={properties.detail}>
      {detail => <span class="shrink-0 text-muted tabular-nums">{detail()}</span>}
    </Show>
  </button>
);

export const CreatePanelDialog: ParentComponent<{ triggerClass: string; }> = (properties) => {
  const store = useInventory();
  const navigate = useNavigate();
  const [isOpen, setIsOpen] = createSignal(false);
  const [name, setName] = createSignal("");
  const [columns, setColumns] = createSignal("4");
  const [rows, setRows] = createSignal("3");
  const [capabilities, setCapabilities] = createSignal<Capabilities>(emptyCapabilities);
  const [sourceSurfaceId, setSourceSurfaceId] = createSignal("");
  const [dialLevels, setDialLevels] = createSignal<Record<string, number>>({});

  const deviceLayouts = createMemo<DeviceLayout[]>(() =>
    store.inventory().devices.flatMap((device) => {
      const layout = deviceGridLayout(device.layout);

      return layout === null ? [] : [{ device, layout }];
    }));
  const source = createMemo<DeviceLayout | null>(
    () => deviceLayouts().find(entry => entry.device.surface_id === sourceSurfaceId()) ?? null,
  );
  const layout = (): GridLayout =>
    source()?.layout ?? { columns: Number(columns()), rows: Number(rows()) };
  const required = (): Capabilities => source()?.device.capabilities ?? capabilities();
  const placements = (): DialPlacement[] => source()?.device.dials ?? [];
  const dialLevel = (placement: DialPlacement): number | undefined =>
    dialLevels()[String(placement.index)];
  const toggleDial = (placement: DialPlacement, isEnabled: boolean) => {
    const key = String(placement.index);

    setDialLevels(current =>
      (isEnabled
        ? { ...current, [key]: 100 }
        : Object.fromEntries(Object.entries(current).filter(([index]) => index !== key))));
  };
  const setDialLevel = (placement: DialPlacement, level: number) =>
    setDialLevels(current => ({ ...current, [String(placement.index)]: level }));
  const dials = (): PanelDial[] =>
    placements().flatMap((placement) => {
      const level = dialLevel(placement);

      return level === undefined ? [] : [{ index: placement.index, level, color: newDialColor }];
    });
  const chooseSource = (surfaceId: string) => {
    setSourceSurfaceId(surfaceId);
    setDialLevels({});
  };

  const reset = () => {
    setName("");
    setColumns("4");
    setRows("3");
    setCapabilities(emptyCapabilities);
    setSourceSurfaceId("");
    setDialLevels({});
  };

  const submit = async (event: SubmitEvent) => {
    event.preventDefault();
    const panel = await store.createPanel({
      name: name().trim(),
      layout: layout(),
      capabilities: required(),
      controls: [],
      dials: dials(),
    });

    if (panel) {
      reset();
      setIsOpen(false);
      navigate({ to: "/panels/$panelId", params: { panelId: panel.panel_id } });
    }
  };

  return (
    <Dialog.Root open={isOpen()} onOpenChange={setIsOpen}>
      <Dialog.Trigger class={properties.triggerClass}>{properties.children}</Dialog.Trigger>
      <Dialog.Portal>
        <Dialog.Overlay class="dialog-overlay" />
        <div class="dialog-positioner">
          <Dialog.Content class="dialog-content">
            <div class="dialog-head">
              <Dialog.Title class="dialog-title">New panel</Dialog.Title>
              <Dialog.CloseButton class="icon-button ml-auto" aria-label="Close">
                <FiX class="size-4" />
              </Dialog.CloseButton>
            </div>
            <form onSubmit={submit}>
              <div class="dialog-body">
                <label class="field-label">
                  Name
                  <input
                    class="field-input"
                    value={name()}
                    onInput={event => setName(event.currentTarget.value)}
                    placeholder="Playback"
                    required
                  />
                </label>
                <div class="grid gap-1.5">
                  <span class="field-label">Size</span>
                  <div class="grid gap-1" role="group" aria-label="Size">
                    <For each={deviceLayouts()}>
                      {entry => (
                        <SizeChoice
                          isSelected={sourceSurfaceId() === entry.device.surface_id}
                          detail={entry.device.dials.length > 0
                            ? `${layoutLabel(entry.layout)}, ${countOf(entry.device.dials.length, "dial")}`
                            : layoutLabel(entry.layout)}
                          onSelect={() => chooseSource(entry.device.surface_id)}
                        >
                          Match
                          {" "}
                          {displayName(entry.device.name)}
                        </SizeChoice>
                      )}
                    </For>
                    <SizeChoice isSelected={source() === null} onSelect={() => chooseSource("")}>
                      Custom
                    </SizeChoice>
                  </div>
                  <Show when={source() === null}>
                    <div class="mt-1 grid grid-cols-2 gap-3">
                      <label class="field-label">
                        Columns
                        <input
                          class="field-input tabular-nums"
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
                          class="field-input tabular-nums"
                          type="number"
                          min="1"
                          value={rows()}
                          onInput={event => setRows(event.currentTarget.value)}
                          required
                        />
                      </label>
                    </div>
                  </Show>
                </div>
                <NeedsField
                  value={required()}
                  isDisabled={source() !== null}
                  onToggle={(key, isNeeded) =>
                    setCapabilities(current => ({ ...current, [key]: isNeeded }))}
                />
                <Show when={placements().length > 0}>
                  <div class="grid gap-1.5">
                    <span class="field-label">Dials</span>
                    <div class="grid gap-1">
                      <For each={placements()}>
                        {placement => (
                          <div class="flex items-center gap-3">
                            <label class="check-tile min-w-0 flex-1">
                              <input
                                type="checkbox"
                                checked={dialLevel(placement) !== undefined}
                                onInput={event => toggleDial(placement, event.currentTarget.checked)}
                              />
                              Dial
                              {" "}
                              {placement.index + 1}
                              <span class="ml-auto font-normal text-muted">{dialSide(placement, layout())}</span>
                            </label>
                            <Show when={dialLevel(placement) !== undefined}>
                              <div class="w-8 shrink-0">
                                <DialIndicator
                                  index={placement.index}
                                  color={newDialColor}
                                  level={dialLevel(placement) ?? 0}
                                />
                              </div>
                              <input
                                class="range-input w-24"
                                type="range"
                                min="0"
                                max="100"
                                value={dialLevel(placement) ?? 0}
                                aria-label={`Dial ${placement.index + 1} ring level`}
                                onInput={event =>
                                  setDialLevel(placement, Number(event.currentTarget.value))}
                              />
                            </Show>
                          </div>
                        )}
                      </For>
                    </div>
                  </div>
                </Show>
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
