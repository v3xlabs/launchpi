import { Link, useNavigate } from "@tanstack/solid-router";
import { FiAlertCircle, FiChevronRight, FiHardDrive, FiPlus } from "solid-icons/fi";
import { Component, createEffect, createMemo, createSignal, For, Index, Show } from "solid-js";

import {
  Device,
  deviceGridLayout,
  DiscoveredDevice,
  displayName,
  isPanelCompatible,
  LogEntry,
  Panel,
} from "../api/inventory";
import { DeviceDisplayControls } from "../components/DeviceDisplayControls";
import { DeviceImage } from "../components/DeviceImage";
import { DevicePresentation } from "../components/DevicePresentation";
import { CopyConfigItems, MenuItem, MenuSeparator, OverflowMenu } from "../components/Menu";
import { MetaSeparator, PageHeader } from "../components/PageHeader";
import { StatusLabel } from "../components/StatusDot";
import { useInventory } from "../context/InventoryContext";
import { AddDeviceDialog } from "../dialogs/AddDeviceDialog";
import { SupportedDevicesDialog } from "../dialogs/SupportedDevicesDialog";
import { countOf } from "../utils/plural";

export const DevicesPage: Component<{ surfaceId?: string; }> = (properties) => {
  const store = useInventory();
  const device = createMemo(
    () => store.inventory().devices.find(entry => entry.surface_id === properties.surfaceId) ?? null,
  );

  return (
    <div class="page">
      <Show when={device()} fallback={<DevicesOverview />}>
        {current => <DeviceDetail device={current()} />}
      </Show>
    </div>
  );
};

const DeviceDetail: Component<{ device: Device; }> = (properties) => {
  const store = useInventory();
  const navigate = useNavigate();

  const name = () => displayName(properties.device.name);
  const children = () =>
    store.inventory().devices.filter(entry => entry.parent_surface_id === properties.device.surface_id);
  const parent = () =>
    store.inventory().devices.find(entry => entry.surface_id === properties.device.parent_surface_id) ?? null;
  const compatiblePanels = () =>
    store.inventory().panels.filter(panel => isPanelCompatible(properties.device, panel));
  const activePanel = (): Panel | null =>
    store.inventory().panels.find(panel => panel.panel_id === properties.device.active_panel_id) ?? null;

  const remove = async () => {
    await store.removeDevice(properties.device.surface_id);
    navigate({ to: "/devices" });
  };

  return (
    <>
      <PageHeader
        mark={<DeviceImage model={properties.device.model} class="h-10 w-10 p-0.5" />}
        title={<span class="truncate">{name()}</span>}
        meta={(
          <>
            <StatusLabel status={properties.device.status} />
            <MetaSeparator />
            <span class="tabular-nums">{`${properties.device.host}:${properties.device.port}`}</span>
            <Show when={parent()}>
              {entry => (
                <>
                  <MetaSeparator />
                  <span>
                    {"on "}
                    <Link
                      to="/devices/$surfaceId"
                      params={{ surfaceId: entry().surface_id }}
                      class="text-soft transition-colors hover:text-slate-900 dark:hover:text-slate-100"
                    >
                      {displayName(entry().name)}
                    </Link>
                  </span>
                </>
              )}
            </Show>
          </>
        )}
        actions={(
          <>
            <button
              type="button"
              class="secondary-button"
              onClick={() =>
                void store.setDeviceEnabled(properties.device.surface_id, !properties.device.is_enabled)}
              disabled={store.isSaving()}
            >
              {properties.device.is_enabled ? "Disable" : "Enable"}
            </button>
            <OverflowMenu label={`More actions for ${name()}`}>
              <CopyConfigItems path={`/api/devices/${encodeURIComponent(properties.device.surface_id)}/config`} />
              <MenuSeparator />
              <MenuItem isDanger isDisabled={store.isSaving()} onSelect={() => void remove()}>
                Remove
              </MenuItem>
            </OverflowMenu>
          </>
        )}
      />

      <div class="grid gap-4">
        <Show when={properties.device.last_error}>
          {message => (
            <div role="alert" class="alert">
              <FiAlertCircle class="size-4 shrink-0" />
              <span class="min-w-0 flex-1">{message()}</span>
              <Show when={properties.device.status === "unavailable" || properties.device.status === "connecting"}>
                <span class="shrink-0 tabular-nums">Retrying every 5 s</span>
              </Show>
            </div>
          )}
        </Show>

        <div class="surface">
          <Show
            when={deviceGridLayout(properties.device.layout)}
            fallback={<p class="empty">No keys of its own. Keys belong to the Stream Deck attached to it.</p>}
          >
            <div class="flex items-center gap-3 px-4 py-3">
              <label class="flex min-w-0 items-center gap-3">
                <span class="font-medium text-soft">Panel</span>
                <select
                  class="field-input w-auto"
                  value={properties.device.active_panel_id ?? ""}
                  disabled={store.isSaving()}
                  onChange={(event) => {
                    if (event.currentTarget.value)
                      void store.assignPanel(properties.device.surface_id, event.currentTarget.value);
                  }}
                >
                  <option value="" disabled>
                    {compatiblePanels().length > 0 ? "Select a panel" : "No compatible panels"}
                  </option>
                  <For each={compatiblePanels()}>
                    {panel => (
                      <option value={panel.panel_id}>
                        {`${panel.name}  ${panel.layout.columns} x ${panel.layout.rows}`}
                      </option>
                    )}
                  </For>
                </select>
              </label>
              <Show when={activePanel()}>
                {panel => (
                  <Link to="/panels/$panelId" params={{ panelId: panel().panel_id }} class="link-button ml-auto">
                    Edit panel
                    <FiChevronRight class="size-3.5" />
                  </Link>
                )}
              </Show>
            </div>
            <Show when={activePanel()} fallback={<p class="empty">No panel on this device.</p>}>
              {panel => (
                <DevicePresentation
                  device={properties.device}
                  panel={panel()}
                  pressedKeys={new Set(store.pressedKeysFor(properties.device.surface_id))}
                />
              )}
            </Show>
          </Show>

          <Show when={children().length > 0}>
            <div class="rows border-t border-hairline">
              <For each={children()}>{child => <DeviceRow device={child} />}</For>
            </div>
          </Show>

          <div
            classList={{
              "grid border-t border-hairline": true,
              "sm:grid-cols-2": properties.device.capabilities.supports_brightness,
            }}
          >
            <Show when={properties.device.capabilities.supports_brightness}>
              <DeviceDisplayControls device={properties.device} />
            </Show>
            <dl
              classList={{
                "grid content-start grid-cols-[6rem_1fr] gap-y-2 px-4 py-4": true,
                "border-t border-hairline sm:border-t-0 sm:border-l": properties.device.capabilities.supports_brightness,
              }}
            >
              <dt class="text-muted">Model</dt>
              <dd class="text-soft">{properties.device.model}</dd>
              <dt class="text-muted">Host</dt>
              <dd class="text-soft tabular-nums">{properties.device.host}</dd>
              <dt class="text-muted">Port</dt>
              <dd class="text-soft tabular-nums">{properties.device.port}</dd>
              <dt class="text-muted">Serial</dt>
              <Show when={properties.device.serial_number} fallback={<dd class="text-muted">Not reported</dd>}>
                {serial => <dd class="text-soft tabular-nums">{serial()}</dd>}
              </Show>
            </dl>
          </div>
        </div>
      </div>

      <DeviceLog surfaceId={properties.device.surface_id} />
    </>
  );
};

const pad = (value: number, length = 2) => String(value).padStart(length, "0");

const logTime = (atMs: number): string => {
  const at = new Date(atMs);

  return `${pad(at.getHours())}:${pad(at.getMinutes())}:${pad(at.getSeconds())}.${pad(
    at.getMilliseconds(),
    3,
  )}`;
};

type LogRow = { entry: LogEntry; count: number; };

// A reconnect loop alternates two messages, so a repeat also joins the row before the last one;
// that keeps the loop at two rows instead of one row per attempt.
const groupRepeats = (entries: LogEntry[]): LogRow[] => {
  const rows: LogRow[] = [];

  for (const entry of entries) {
    const repeated = rows
      .slice(-2)
      .find(row => row.entry.message === entry.message && row.entry.level === entry.level);

    if (repeated === undefined) rows.push({ entry, count: 1 });
    else {
      repeated.entry = entry;
      repeated.count += 1;
    }
  }

  return rows;
};

const DeviceLog: Component<{ surfaceId: string; }> = (properties) => {
  const store = useInventory();
  const rows = createMemo(() => groupRepeats(store.logsFor(properties.surfaceId)));
  const [isPinned, setIsPinned] = createSignal(true);
  let container: HTMLDivElement | undefined;

  const onScroll = () => {
    if (container === undefined) return;

    setIsPinned(container.scrollHeight - container.scrollTop - container.clientHeight < 24);
  };

  // Tails like a terminal: follow new lines unless the reader has scrolled up to look at something.
  createEffect(() => {
    const rowCount = rows().length;

    if (container !== undefined && rowCount > 0 && isPinned())
      container.scrollTop = container.scrollHeight;
  });

  return (
    <section>
      <div class="section-head">
        <h2 class="label-sm">Activity</h2>
        <span class="ml-auto text-muted tabular-nums">{countOf(store.logsFor(properties.surfaceId).length, "event")}</span>
      </div>
      <div class="surface overflow-hidden">
        <Show when={rows().length > 0} fallback={<p class="empty">No events yet.</p>}>
          <div class="log" ref={element => (container = element)} onScroll={onScroll}>
            <Index each={rows()}>
              {row => (
                <div class="log-row" data-level={row().entry.level}>
                  <span class="log-time">{logTime(row().entry.at_ms)}</span>
                  <span class="log-message">
                    {row().entry.message.charAt(0).toUpperCase() + row().entry.message.slice(1)}
                  </span>
                  <span class="log-count">{row().count > 1 ? `${row().count}x` : ""}</span>
                </div>
              )}
            </Index>
          </div>
        </Show>
      </div>
    </section>
  );
};

const DeviceRow: Component<{ device: Device; isChild?: boolean; }> = (properties) => {
  const store = useInventory();
  const name = () => displayName(properties.device.name);
  const activePanel = () =>
    store.inventory().panels.find(panel => panel.panel_id === properties.device.active_panel_id) ?? null;

  return (
    <div classList={{ "row transition-colors hover:bg-raised/60": true, "pl-10": properties.isChild }}>
      <Link
        to="/devices/$surfaceId"
        params={{ surfaceId: properties.device.surface_id }}
        class="flex min-w-0 flex-1 items-center gap-4"
      >
        <DeviceImage model={properties.device.model} class="h-9 w-14" />
        <span class="min-w-0 flex-1">
          <span class="row-title block">{name()}</span>
          <span class="row-meta block">
            <Show when={properties.device.model !== name()}>
              {properties.device.model}
              {" "}
              <MetaSeparator />
              {" "}
            </Show>
            {properties.device.host}
          </span>
        </span>
        <span class="w-28 shrink-0">
          <StatusLabel status={properties.device.status} />
        </span>
        <span class="w-24 shrink-0 truncate">
          <Show when={activePanel()} fallback={<span class="text-muted">No panel</span>}>
            {panel => <span class="text-soft">{panel().name}</span>}
          </Show>
        </span>
      </Link>
      <OverflowMenu label={`More actions for ${name()}`}>
        <MenuItem
          isDisabled={store.isSaving()}
          onSelect={() => void store.setDeviceEnabled(properties.device.surface_id, !properties.device.is_enabled)}
        >
          {properties.device.is_enabled ? "Disable" : "Enable"}
        </MenuItem>
        <CopyConfigItems path={`/api/devices/${encodeURIComponent(properties.device.surface_id)}/config`} />
        <MenuSeparator />
        <MenuItem
          isDanger
          isDisabled={store.isSaving()}
          onSelect={() => void store.removeDevice(properties.device.surface_id)}
        >
          Remove
        </MenuItem>
      </OverflowMenu>
    </div>
  );
};

const isSameEndpoint = (device: Device, discovered: DiscoveredDevice): boolean =>
  (device.serial_number !== null && discovered.serial_number !== null
    ? device.serial_number === discovered.serial_number
    : device.host === discovered.host && device.port === discovered.port);

const DiscoveredRow: Component<{ discovered: DiscoveredDevice; }> = (properties) => {
  const store = useInventory();
  const [isAdding, setIsAdding] = createSignal(false);

  const add = async () => {
    setIsAdding(true);
    await store.addDiscovered(properties.discovered.discovery_id);
    setIsAdding(false);
  };

  return (
    <div class="row">
      <DeviceImage model={properties.discovered.model} class="h-9 w-14" />
      <span
        class="min-w-0 flex-1"
        title={properties.discovered.serial_number === null ? undefined : `Serial ${properties.discovered.serial_number}`}
      >
        <span class="row-title block">{displayName(properties.discovered.name)}</span>
        <span class="row-meta block">{properties.discovered.host}</span>
      </span>
      <button
        type="button"
        class="secondary-button"
        onClick={() => void add()}
        disabled={store.isSaving()}
      >
        <Show when={!isAdding()} fallback="Adding...">
          <FiPlus class="size-3.5" />
          Add
        </Show>
      </button>
    </div>
  );
};

const DevicesOverview: Component = () => {
  const store = useInventory();
  const rootDevices = () => store.inventory().devices.filter(device => device.parent_surface_id === null);
  const discovered = () =>
    store.inventory().discovered.filter(entry =>
      store.inventory().devices.every(device => !isSameEndpoint(device, entry)));

  return (
    <>
      <PageHeader
        mark={<FiHardDrive class="size-5" />}
        title="Devices"
        meta={(
          <>
            <span>
              <span class="tabular-nums">{store.inventory().devices.length}</span>
              {" added"}
            </span>
            <MetaSeparator />
            <span>
              <span class="tabular-nums">{discovered().length}</span>
              {" on your network"}
            </span>
          </>
        )}
        actions={(
          <>
            <AddDeviceDialog />
            <SupportedDevicesDialog />
          </>
        )}
      />

      <section>
        <div class="section-head">
          <h2 class="label-sm">Added</h2>
        </div>
        <div class="surface rows overflow-hidden">
          <Show when={rootDevices().length > 0} fallback={<p class="empty">No devices yet.</p>}>
            <For each={rootDevices()}>
              {device => (
                <>
                  <DeviceRow device={device} />
                  <For
                    each={store.inventory().devices.filter(child => child.parent_surface_id === device.surface_id)}
                  >
                    {child => <DeviceRow device={child} isChild />}
                  </For>
                </>
              )}
            </For>
          </Show>
        </div>
      </section>

      <section>
        <div class="section-head">
          <h2 class="label-sm">On your network</h2>
          <span class="ml-auto inline-flex items-center gap-1.5 text-muted" role="status">
            <Show when={store.inventory().config.discovery} fallback="Discovery is off">
              <span class="relative inline-flex size-2">
                <span class="absolute inline-flex size-2 animate-ping rounded-full bg-emerald-500 opacity-60" />
                <span class="status-dot size-2 bg-emerald-500" />
              </span>
              Listening for devices
            </Show>
          </span>
        </div>
        <div class="surface rows overflow-hidden">
          <Show
            when={discovered().length > 0}
            fallback={<p class="empty">No devices found yet. A dock on another subnet needs Add.</p>}
          >
            <For each={discovered()}>{entry => <DiscoveredRow discovered={entry} />}</For>
          </Show>
        </div>
      </section>
    </>
  );
};
