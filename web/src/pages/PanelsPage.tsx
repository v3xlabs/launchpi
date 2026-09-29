import { Link, useNavigate } from "@tanstack/solid-router";
import { FiCheck, FiGrid, FiPlus } from "solid-icons/fi";
import { Component, createEffect, createMemo, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { createStore, produce, unwrap } from "solid-js/store";

import {
  Control,
  Device,
  dialsForPanel,
  displayName,
  layoutLabel,
  Panel,
  PanelDial,
  RgbaColor,
} from "../api/inventory";
import { InfoTip } from "../components/InfoTip";
import { CopyConfigItems, MenuItem, MenuSeparator, OverflowMenu } from "../components/Menu";
import { MetaSeparator, PageHeader } from "../components/PageHeader";
import { PanelInspector, PanelSelection } from "../components/PanelInspector";
import { PanelStage, PanelThumbnail } from "../components/PanelPreview";
import { StatusLabel } from "../components/StatusDot";
import { ControlClipboard, useInventory } from "../context/InventoryContext";
import { CreatePanelDialog } from "../dialogs/CreatePanelDialog";
import { DeletePanelDialog } from "../dialogs/DeletePanelDialog";
import { countOf } from "../utils/plural";
import { newState } from "../utils/rendered";

// Store proxies cannot be structured-cloned, so the draft copies the underlying object.
const cloneState = <T,>(value: T): T => structuredClone(unwrap(value));

const DeviceNames: Component<{ devices: Device[]; }> = properties => (
  <Show when={properties.devices[0]}>
    {first => (
      <span class="inline-flex min-w-0 items-center gap-1.5">
        <StatusLabel status={first().status} label={displayName(first().name)} />
        <Show when={properties.devices.length > 1}>
          <span class="text-muted tabular-nums">{`+${properties.devices.length - 1}`}</span>
        </Show>
      </span>
    )}
  </Show>
);

const PanelSize: Component<{ panel: Panel; }> = properties => (
  <>
    <span class="tabular-nums">{layoutLabel(properties.panel.layout)}</span>
    <Show when={properties.panel.dials.length > 0}>
      <MetaSeparator />
      <span class="tabular-nums">{countOf(properties.panel.dials.length, "dial")}</span>
    </Show>
  </>
);

export const PanelsPage: Component<{ panelId?: string; }> = (properties) => {
  const store = useInventory();
  const navigate = useNavigate();
  const [draft, setDraft] = createStore<{ panel: Panel | null; dirty: boolean; }>({
    panel: null,
    dirty: false,
  });
  const [selection, setSelection] = createSignal<PanelSelection | null>(null);
  const [pasteTarget, setPasteTarget] = createSignal<{ column: number; row: number; } | null>(null);
  const [isDeleting, setIsDeleting] = createSignal(false);
  const serverPanel = createMemo(
    () => store.inventory().panels.find(panel => panel.panel_id === properties.panelId) ?? null,
  );
  const pressedKeys = createMemo(() => store.pressedKeysForPanel(properties.panelId ?? ""));
  const dialLevels = createMemo(() => store.dialLevelsForPanel(properties.panelId ?? ""));
  const pressedDials = createMemo(() => store.pressedDialsForPanel(properties.panelId ?? ""));
  const assignedDevices = createMemo(() =>
    store.inventory().devices.filter(device => device.active_panel_id === properties.panelId),
  );
  const dials = createMemo(() =>
    (draft.panel === null ? [] : dialsForPanel(store.inventory().devices, draft.panel.layout)));

  createEffect(() => {
    const panelId = properties.panelId;
    const server = serverPanel();

    if (draft.panel?.panel_id !== panelId) {
      setDraft({ panel: server ? cloneState(server) : null, dirty: false });
      setSelection(null);
      setPasteTarget(null);

      return;
    }

    if (server && draft.panel === null) setDraft("panel", cloneState(server));
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

  const mutateDial = (index: number, mutate: (dial: PanelDial) => void) =>
    mutatePanel((panel) => {
      const dial = panel.dials.find(entry => entry.index === index);

      if (dial) mutate(dial);
    });

  const setDialColor = (index: number, color: RgbaColor) =>
    mutateDial(index, (dial) => {
      dial.color = color;
    });

  const setDialLevel = (index: number, level: number) =>
    mutateDial(index, (dial) => {
      dial.level = level;
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

    mutatePanel((entry) => {
      entry.controls = entry.controls.filter(existing => existing.position.column !== column || existing.position.row !== row);
      entry.controls.push(control);
    });
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
      setPasteTarget({ column, row });

      return;
    }

    placeControl(column, row, store.clipboard() ?? undefined);
  };

  const handleCellFocus = (_control: Control | undefined, column: number, row: number) => setPasteTarget({ column, row });

  const savePanel = async () => {
    const panel = draft.panel;

    if (panel === null) return;

    if (await store.savePanel(panel)) setDraft("dirty", false);
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
        event.preventDefault();
        const existing = draft.panel?.controls.find(control => control.position.column === cell.column && control.position.row === cell.row);

        if (existing && !globalThis.confirm(`Replace ${existing.name}? Its settings will be overwritten.`)) return;

        placeControl(cell.column, cell.row, clip);
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
            <PageHeader
              mark={<FiGrid class="size-5" />}
              title={(
                <>
                  <span class="truncate">{panel().name}</span>
                  <Show when={draft.dirty}>
                    <span class="unsaved">
                      <span class="status-dot size-2 bg-amber-500" aria-hidden="true" />
                      Unsaved
                    </span>
                  </Show>
                </>
              )}
              meta={(
                <>
                  <PanelSize panel={panel()} />
                  <Show when={assignedDevices().length > 0}>
                    <MetaSeparator />
                    <span class="inline-flex min-w-0 items-center gap-1.5">
                      On
                      <DeviceNames devices={assignedDevices()} />
                    </span>
                  </Show>
                </>
              )}
              actions={(
                <>
                  <button
                    type="button"
                    class="primary-button"
                    onClick={() => void savePanel()}
                    disabled={store.isSaving() || !draft.dirty}
                  >
                    <FiCheck class="size-4" />
                    {store.isSaving() ? "Saving..." : "Save"}
                  </button>
                  <OverflowMenu label={`More actions for ${panel().name}`}>
                    <CopyConfigItems path={`/api/panels/${encodeURIComponent(panel().panel_id)}/config`} />
                    <MenuSeparator />
                    <MenuItem isDanger onSelect={() => setIsDeleting(true)}>Delete panel</MenuItem>
                  </OverflowMenu>
                  <DeletePanelDialog
                    panel={panel()}
                    isOpen={isDeleting()}
                    onOpenChange={setIsDeleting}
                    onDeleted={() => navigate({ to: "/panels" })}
                  />
                </>
              )}
            />

            <div class="grid gap-4">
              <div class="surface">
                <PanelStage
                  panel={panel()}
                  dials={dials()}
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
                <div class="flex items-center gap-2 border-t border-hairline px-4 py-2 text-muted">
                  <Show
                    when={store.clipboard()}
                    fallback={(
                      <>
                        <span class="flex-1">Click an empty key to add one.</span>
                        <InfoTip label="Keyboard shortcuts">
                          Ctrl/Cmd+C copies the selected key, Ctrl/Cmd+V pastes on the focused key, Delete
                          removes it, Esc clears the selection.
                        </InfoTip>
                      </>
                    )}
                  >
                    {clip => (
                      <>
                        <span class="flex min-w-0 flex-1 items-center gap-2" role="status">
                          <span class="truncate">{`Copied ${clip().name}`}</span>
                          <span class="flex shrink-0 items-center gap-1">
                            <kbd class="kbd">Ctrl+V</kbd>
                            to paste
                          </span>
                        </span>
                        <button type="button" class="link-button" onClick={() => store.clearClipboard()}>
                          Clear
                        </button>
                      </>
                    )}
                  </Show>
                </div>
              </div>

              <PanelInspector
                panel={panel()}
                dials={dials()}
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

const PanelTile: Component<{ panel: Panel; }> = (properties) => {
  const store = useInventory();
  const pressedKeys = createMemo(() => store.pressedKeysForPanel(properties.panel.panel_id));
  const dialLevels = createMemo(() => store.dialLevelsForPanel(properties.panel.panel_id));
  const pressedDials = createMemo(() => store.pressedDialsForPanel(properties.panel.panel_id));
  const assignedDevices = createMemo(() =>
    store.inventory().devices.filter(device => device.active_panel_id === properties.panel.panel_id));

  return (
    <Link
      to="/panels/$panelId"
      params={{ panelId: properties.panel.panel_id }}
      class="surface grid gap-3 p-4"
    >
      <div class="grid h-40 place-items-center">
        <div class="w-full">
          <PanelThumbnail
            panel={properties.panel}
            dials={dialsForPanel(store.inventory().devices, properties.panel.layout)}
            pressedKeys={pressedKeys()}
            dialLevels={dialLevels()}
            pressedDials={pressedDials()}
          />
        </div>
      </div>
      <div class="flex items-center gap-3">
        <span class="min-w-0 flex-1">
          <span class="row-title block">{properties.panel.name}</span>
          <span class="flex items-center gap-2 text-muted">
            <PanelSize panel={properties.panel} />
          </span>
        </span>
        <Show
          when={assignedDevices().length > 0}
          fallback={<span class="shrink-0 text-muted">Not on a device</span>}
        >
          <DeviceNames devices={assignedDevices()} />
        </Show>
      </div>
    </Link>
  );
};

const PanelsOverview: Component = () => {
  const store = useInventory();

  return (
    <>
      <PageHeader
        mark={<FiGrid class="size-5" />}
        title="Panels"
        meta={<span class="tabular-nums">{countOf(store.inventory().panels.length, "panel")}</span>}
        actions={(
          <CreatePanelDialog triggerClass="primary-button">
            <FiPlus class="size-4" />
            New panel
          </CreatePanelDialog>
        )}
      />
      <Show
        when={store.inventory().panels.length > 0}
        fallback={(
          <div class="surface">
            <p class="empty">No panels yet.</p>
          </div>
        )}
      >
        <div class="grid items-start gap-4 sm:grid-cols-2">
          <For each={store.inventory().panels}>{panel => <PanelTile panel={panel} />}</For>
        </div>
      </Show>
    </>
  );
};
