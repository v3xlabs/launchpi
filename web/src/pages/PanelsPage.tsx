import { Link, useNavigate } from "@tanstack/solid-router";
import {
  TbFillCircleCheck as TbCheck,
  TbFillClipboard as TbCopy,
  TbFillFileDownload as TbDownload,
  TbFillTrash as TbTrash,
} from "solid-icons/tb";
import { Component, createEffect, createMemo, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { createStore, produce } from "solid-js/store";

import {
  Control,
  Device,
  displayName,
  fetchPanelConfig,
  layoutLabel,
  Panel,
  panelDialCount,
  RgbaColor,
  studioDialCount,
} from "../api/inventory";
import { CopyTomlButton } from "../components/CopyTomlButton";
import { PanelInspector, PanelSelection } from "../components/PanelInspector";
import { PanelStage, PanelThumbnail } from "../components/PanelPreview";
import { StatusDot } from "../components/StatusDot";
import { ControlClipboard, useInventory } from "../context/InventoryContext";
import { CreatePanelDialog } from "../dialogs/CreatePanelDialog";
import { DeletePanelDialog } from "../dialogs/DeletePanelDialog";
import { newState } from "../utils/rendered";

const cloneState = <T,>(value: T): T => JSON.parse(JSON.stringify(value)) as T;

const padded = <T,>(values: T[], length: number, fallback: T): T[] =>
  Array.from({ length }, (_, index) => values[index] ?? fallback);

const defaultDialColor: RgbaColor = { red: 30, green: 41, blue: 59, alpha: 255 };

export const PanelsPage: Component<{ panelId?: string; }> = (properties) => {
  const store = useInventory();
  const navigate = useNavigate();
  const [draft, setDraft] = createStore<{ panel: Panel | null; dirty: boolean; }>({
    panel: null,
    dirty: false,
  });
  const [selection, setSelection] = createSignal<PanelSelection | null>(null);
  const [pasteTarget, setPasteTarget] = createSignal<{ column: number; row: number; } | null>(null);

  const serverPanel = createMemo(
    () => store.inventory().panels.find(panel => panel.panel_id === properties.panelId) ?? null,
  );
  const pressedKeys = createMemo(() => store.pressedKeysForPanel(properties.panelId ?? ""));
  const dialLevels = createMemo(() => store.dialLevelsForPanel(properties.panelId ?? ""));
  const pressedDials = createMemo(() => store.pressedDialsForPanel(properties.panelId ?? ""));
  const assignedDevices = createMemo(() =>
    store.inventory().devices.filter(device => device.active_panel_id === properties.panelId),
  );

  createEffect(() => {
    const panelId = properties.panelId;
    const server = serverPanel();

    if (draft.panel?.panel_id !== panelId) {
      setDraft({ panel: server ? cloneState(server) : null, dirty: false });
      setSelection(null);

      return;
    }

    if (draft.panel === null && server) setDraft("panel", cloneState(server));
  });

  const selectedControlId = () => {
    const current = selection();

    return current?.kind === "control" ? current.controlId : null;
  };
  const selectedDialIndex = () => {
    const current = selection();

    return current?.kind === "dial" ? current.index : null;
  };
  const selectedControl = createMemo(
    () => draft.panel?.controls.find(control => control.control_id === selectedControlId()) ?? null,
  );

  const mutatePanel = (mutate: (panel: Panel) => void) => {
    setDraft(
      "panel",
      produce((panel) => {
        if (panel) mutate(panel);
      }),
    );
    setDraft("dirty", true);
  };

  const mutateSelectedControl = (mutate: (control: Control) => void) =>
    mutatePanel((panel) => {
      const control = panel.controls.find(entry => entry.control_id === selectedControlId());

      if (control) mutate(control);
    });

  const setDialColor = (index: number, color: RgbaColor) =>
    mutatePanel((panel) => {
      const colors = padded(panel.dial_colors, studioDialCount, defaultDialColor);

      colors[index] = color;
      panel.dial_colors = colors;
    });

  const setDialLevel = (index: number, level: number) =>
    mutatePanel((panel) => {
      const levels = padded(panel.dial_ring_levels, studioDialCount, 100);

      levels[index] = level;
      panel.dial_ring_levels = levels;
    });

  const placeControl = (column: number, row: number, template?: ControlClipboard) => {
    const panel = draft.panel;

    if (panel === null) return;

    const controlId = `control-${Date.now()}`;
    const control: Control = {
      control_id: controlId,
      name: template?.name ?? `Control ${panel.controls.length + 1}`,
      position: { column, row },
      default_state: cloneState(template?.default_state ?? newState(false)),
      pressed_state: template?.pressed_state ? cloneState(template.pressed_state) : null,
      action_bindings: cloneState(template?.action_bindings ?? []),
    };

    mutatePanel(entry => entry.controls.push(control));
    setSelection({ kind: "control", controlId });
    setPasteTarget(null);
  };

  const removeControl = () => {
    const controlId = selectedControlId();

    if (controlId === null) return;

    mutatePanel((panel) => {
      panel.controls = panel.controls.filter(control => control.control_id !== controlId);
    });
    setSelection(null);
  };

  const firstFreeCell = (): { column: number; row: number; } | null => {
    const panel = draft.panel;

    if (panel === null) return null;

    for (let row = 0; row < panel.layout.rows; row += 1) {
      for (let column = 0; column < panel.layout.columns; column += 1) {
        const occupied = panel.controls.some(
          control => control.position.column === column && control.position.row === row,
        );

        if (!occupied) return { column, row };
      }
    }

    return null;
  };

  const handleCellClick = (control: Control | undefined, column: number, row: number) => {
    if (control !== undefined) {
      setSelection({ kind: "control", controlId: control.control_id });
      setPasteTarget(null);

      return;
    }

    placeControl(column, row, store.clipboard() ?? undefined);
  };

  const handleCellFocus = (control: Control | undefined, column: number, row: number) => setPasteTarget(control === undefined ? { column, row } : null);

  const savePanel = async () => {
    const panel = draft.panel;

    if (panel === null) return;

    await store.savePanel(panel);
    setDraft("dirty", false);
  };

  const copySelectedControl = () => {
    const control = selectedControl();

    if (control !== null) store.copyControl(control);
  };

  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      store.clearClipboard();
      setSelection(null);
      setPasteTarget(null);

      return;
    }

    const target = event.target instanceof HTMLElement ? event.target : null;
    const isField = target !== null && (["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName) || target.isContentEditable);

    if (isField) return;

    if (event.key === "Delete" || event.key === "Backspace") {
      if (selectedControl() !== null) {
        removeControl();
        event.preventDefault();
      }

      return;
    }

    if (!(event.metaKey || event.ctrlKey)) return;

    const key = event.key.toLowerCase();

    if (key === "c" && selectedControl() !== null) {
      copySelectedControl();
      event.preventDefault();
    }

    if (key === "v") {
      const clip = store.clipboard();
      const cell = pasteTarget() ?? firstFreeCell();

      if (cell !== null && clip !== null) {
        placeControl(cell.column, cell.row, clip);
        event.preventDefault();
      }
    }
  };

  onMount(() => globalThis.addEventListener("keydown", onKeyDown));
  onCleanup(() => globalThis.removeEventListener("keydown", onKeyDown));

  return (
    <div class="page">
      <Show when={draft.panel} fallback={<PanelsOverview />}>
        {panel => (
          <>
            <div class="page-head">
              <div class="min-w-0">
                <p class="breadcrumb">
                  <Link to="/panels">Panels</Link>
                  <span class="meta-sep">/</span>
                  <span class="text-neutral-400">{layoutLabel(panel().layout)}</span>
                </p>
                <h1 class="page-title mt-1">{panel().name}</h1>
                <div class="meta-line">
                  <span>
                    {panel().controls.length}
                    {" "}
                    of
                    {" "}
                    {panel().layout.columns * panel().layout.rows}
                    {" "}
                    keys assigned
                  </span>
                  <Show when={panelDialCount(panel()) > 0}>
                    <span class="meta-sep">-</span>
                    <span>
                      {panelDialCount(panel())}
                      {" "}
                      dials
                    </span>
                  </Show>
                  <Show when={draft.dirty}>
                    <span class="meta-sep">-</span>
                    <span class="text-amber-400">unsaved changes</span>
                  </Show>
                </div>
              </div>
              <div class="flex gap-2">
                <button
                  type="button"
                  class="secondary-button"
                  onClick={() => void store.exportPanel(panel())}
                >
                  <TbDownload class="h-3.5 w-3.5" />
                  Export TOML
                </button>
                <CopyTomlButton load={() => fetchPanelConfig(panel().panel_id)} />
                <button
                  type="button"
                  class="primary-button"
                  onClick={() => void savePanel()}
                  disabled={store.isSaving() || !draft.dirty}
                >
                  <TbCheck class="h-3.5 w-3.5" />
                  {store.isSaving() ? "Saving..." : "Save panel"}
                </button>
                <DeletePanelDialog
                  panel={panel()}
                  onDeleted={() => navigate({ to: "/panels" })}
                  trigger={(
                    <button
                      type="button"
                      class="danger-button"
                      aria-label={`Delete ${panel().name}`}
                      title="Delete panel"
                    >
                      <TbTrash class="h-4 w-4" />
                    </button>
                  )}
                />
              </div>
            </div>

            <Show when={store.clipboard()}>
              {clip => (
                <div class="clipboard-banner">
                  <TbCopy class="h-3.5 w-3.5 shrink-0" />
                  <span class="min-w-0 flex-1 truncate">
                    Copied
                    {" "}
                    <strong>{clip().name}</strong>
                    <span class="hidden sm:inline"> - click an empty key or use Ctrl/Cmd+V. Delete removes the selected key; Esc clears.</span>
                  </span>
                  <button
                    type="button"
                    class="link-button"
                    onClick={() => store.clearClipboard()}
                  >
                    Clear
                  </button>
                </div>
              )}
            </Show>

            <div class="editor">
              <div class="grid gap-4">
                <div class="card">
                  <div class="card-head">
                    <p class="card-title">Surface</p>
                    <span class="chip chip-muted">
                      {panel().layout.columns}
                      {" x "}
                      {panel().layout.rows}
                    </span>
                  </div>
                  <PanelStage
                    panel={panel()}
                    pressedKeys={pressedKeys()}
                    dialLevels={dialLevels()}
                    pressedDials={pressedDials()}
                    activeControlId={selectedControlId()}
                    activeDialIndex={selectedDialIndex()}
                    pasteMode={store.clipboard() !== null}
                    onCellClick={handleCellClick}
                    onCellFocus={handleCellFocus}
                    onDialClick={index => setSelection({ kind: "dial", index })}
                  />
                </div>

                <Show when={assignedDevices().length > 0}>
                  <div class="card">
                    <div class="card-head">
                      <p class="card-title">In use by</p>
                      <span class="chip chip-muted">{assignedDevices().length}</span>
                    </div>
                    <div class="rows">
                      <For each={assignedDevices()}>
                        {device => <AssignedDeviceRow device={device} />}
                      </For>
                    </div>
                  </div>
                </Show>
              </div>

              <PanelInspector
                panel={panel()}
                selection={selection()}
                control={selectedControl()}
                onPanelMutate={mutatePanel}
                onControlMutate={mutateSelectedControl}
                onCopyControl={copySelectedControl}
                onRemoveControl={removeControl}
                onDialColorChange={setDialColor}
                onDialLevelChange={setDialLevel}
              />
            </div>
          </>
        )}
      </Show>
    </div>
  );
};

const AssignedDeviceRow: Component<{ device: Device; }> = properties => (
  <Link
    to="/devices/$surfaceId"
    params={{ surfaceId: properties.device.surface_id }}
    class="row row-main no-underline"
  >
    <StatusDot status={properties.device.status} />
    <span class="min-w-0 flex-1">
      <span class="row-title block">{displayName(properties.device.name)}</span>
      <span class="row-meta block">
        {properties.device.model}
        {" - "}
        {properties.device.host}
      </span>
    </span>
  </Link>
);

const PanelCard: Component<{ panel: Panel; }> = (properties) => {
  const store = useInventory();
  const pressedKeys = createMemo(() => store.pressedKeysForPanel(properties.panel.panel_id));
  const dialLevels = createMemo(() => store.dialLevelsForPanel(properties.panel.panel_id));
  const pressedDials = createMemo(() => store.pressedDialsForPanel(properties.panel.panel_id));
  const assignedCount = () =>
    store.inventory().devices.filter(device => device.active_panel_id === properties.panel.panel_id).length;

  return (
    <Link
      to="/panels/$panelId"
      params={{ panelId: properties.panel.panel_id }}
      class="card no-underline transition hover:border-neutral-700"
    >
      <div class="card-head">
        <p class="row-title">{properties.panel.name}</p>
        <span class="chip">{layoutLabel(properties.panel.layout)}</span>
      </div>
      <PanelThumbnail
        panel={properties.panel}
        pressedKeys={pressedKeys()}
        dialLevels={dialLevels()}
        pressedDials={pressedDials()}
      />
      <div class="flex items-center justify-between gap-3 border-t border-neutral-800 px-3 py-2 text-xs text-neutral-500">
        <span>
          {properties.panel.controls.length}
          {" "}
          controls
        </span>
        <Show when={assignedCount() > 0} fallback={<span>Unassigned</span>}>
          <span class="text-neutral-400">
            On
            {" "}
            {assignedCount()}
            {" "}
            device
            {assignedCount() === 1 ? "" : "s"}
          </span>
        </Show>
      </div>
    </Link>
  );
};

const PanelsOverview: Component = () => {
  const store = useInventory();

  return (
    <>
      <div class="page-head">
        <div>
          <h1 class="page-title">Panels</h1>
          <p class="page-subtitle">
            Reusable key layouts. A panel runs on any device with a matching grid and capabilities.
          </p>
        </div>
        <CreatePanelDialog
          trigger={(
            <button type="button" class="primary-button">
              New panel
            </button>
          )}
        />
      </div>
      <Show
        when={store.inventory().panels.length > 0}
        fallback={(
          <div class="card">
            <p class="empty">No panels yet. Create one to begin.</p>
          </div>
        )}
      >
        <div class="grid items-start gap-4 lg:grid-cols-2 2xl:grid-cols-3">
          <For each={store.inventory().panels}>{panel => <PanelCard panel={panel} />}</For>
        </div>
      </Show>
    </>
  );
};
