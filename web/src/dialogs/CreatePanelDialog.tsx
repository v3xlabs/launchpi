import * as Dialog from "@kobalte/core/dialog";
import { useNavigate } from "@tanstack/solid-router";
import { TbFillCircleX as TbX } from "solid-icons/tb";
import { Component, createMemo, createSignal, For, JSX, Show } from "solid-js";

import {
  Capabilities,
  capabilityLabels,
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
import { useInventory } from "../context/InventoryContext";

type DeviceLayout = { device: Device; layout: GridLayout; };

export const CreatePanelDialog: Component<{ trigger: JSX.Element; }> = (properties) => {
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
      <Dialog.Trigger as="div" class="contents">
        {properties.trigger}
      </Dialog.Trigger>
      <Dialog.Portal>
        <Dialog.Overlay class="dialog-overlay" />
        <div class="dialog-positioner">
          <Dialog.Content class="dialog-content">
            <div class="dialog-head">
              <div>
                <Dialog.Title class="dialog-title">Create panel</Dialog.Title>
                <Dialog.Description class="dialog-description">
                  Define the grid and the capabilities a device must support, or take both from a
                  device you already have.
                </Dialog.Description>
              </div>
              <Dialog.CloseButton class="icon-button" aria-label="Close create panel dialog">
                <TbX class="h-4 w-4" />
              </Dialog.CloseButton>
            </div>
            <form onSubmit={submit}>
              <div class="dialog-body">
                <label class="field-label">
                  Panel name
                  <input
                    class="field-input"
                    value={name()}
                    onInput={event => setName(event.currentTarget.value)}
                    placeholder="Playback"
                    required
                  />
                </label>
                <Show when={deviceLayouts().length > 0}>
                  <label class="field-label">
                    Layout
                    <select
                      class="field-input"
                      value={sourceSurfaceId()}
                      onChange={(event) => {
                        setSourceSurfaceId(event.currentTarget.value);
                        setDialLevels({});
                      }}
                    >
                      <option value="">Custom</option>
                      <For each={deviceLayouts()}>
                        {entry => (
                          <option value={entry.device.surface_id}>
                            {displayName(entry.device.name)}
                            {" - "}
                            {layoutLabel(entry.layout)}
                          </option>
                        )}
                      </For>
                    </select>
                  </label>
                </Show>
                <div class="grid grid-cols-2 gap-3">
                  <label class="field-label">
                    Columns
                    <input
                      class="field-input"
                      type="number"
                      min="1"
                      value={layout().columns}
                      onInput={event => setColumns(event.currentTarget.value)}
                      disabled={source() !== null}
                      required
                    />
                  </label>
                  <label class="field-label">
                    Rows
                    <input
                      class="field-input"
                      type="number"
                      min="1"
                      value={layout().rows}
                      onInput={event => setRows(event.currentTarget.value)}
                      disabled={source() !== null}
                      required
                    />
                  </label>
                </div>
                <fieldset class="grid gap-1">
                  <legend class="field-label">Required capabilities</legend>
                  <div class="mt-1 grid grid-cols-2 gap-1.5 sm:grid-cols-3">
                    <For each={capabilityLabels}>
                      {({ key, label }) => (
                        <label class="check-tile">
                          <input
                            type="checkbox"
                            checked={required()[key]}
                            disabled={source() !== null}
                            onInput={(event) => {
                              const { checked } = event.currentTarget;

                              setCapabilities(current => ({ ...current, [key]: checked }));
                            }}
                          />
                          {label}
                        </label>
                      )}
                    </For>
                  </div>
                </fieldset>
                <Show when={placements().length > 0}>
                  <fieldset class="grid gap-1.5">
                    <legend class="field-label">Dials</legend>
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
                            <span class="mono ml-auto">{dialSide(placement, layout())}</span>
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
                  </fieldset>
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
