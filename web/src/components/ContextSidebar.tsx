import { Link, useLocation } from "@tanstack/solid-router";
import {
  TbFillCirclePlus as TbPlus,
  TbFillExchange as TbRefresh,
} from "solid-icons/tb";
import { Component, createMemo, For, Match, Show, Switch } from "solid-js";

import { Device, deviceGridLayout, displayName, layoutLabel, Panel } from "../api/inventory";
import { statusTone } from "../api/plugins";
import { useInventory } from "../context/InventoryContext";
import { AddPluginDialog } from "../dialogs/AddPluginDialog";
import { DeviceImage } from "./DeviceImage";
import { PanelThumbnail } from "./PanelPreview";
import { StatusDot } from "./StatusDot";

const DeviceItem: Component<{ device: Device; }> = properties => (
  <Link
    to="/devices/$surfaceId"
    params={{ surfaceId: properties.device.surface_id }}
    class="nav-item"
  >
    <StatusDot status={properties.device.status} />
    <DeviceImage model={properties.device.model} class="h-7 w-10" />
    <span class="min-w-0 flex-1">
      <span class="nav-item-title block">{displayName(properties.device.name)}</span>
      <span class="nav-item-meta block">
        {layoutLabel(deviceGridLayout(properties.device.layout))}
        {" - "}
        {properties.device.model}
      </span>
    </span>
  </Link>
);

const DeviceNav: Component = () => {
  const store = useInventory();
  const rootDevices = () =>
    store.inventory().devices.filter(device => device.parent_surface_id === null);
  const childrenOf = (surfaceId: string) =>
    store.inventory().devices.filter(device => device.parent_surface_id === surfaceId);

  return (
    <section class="nav">
      <Link to="/devices" class="nav-heading">
        Devices
        <span class="chip chip-muted">{store.inventory().devices.length}</span>
      </Link>
      <Show when={rootDevices().length > 0} fallback={<p class="nav-empty">No devices added.</p>}>
        <For each={rootDevices()}>
          {device => (
            <>
              <DeviceItem device={device} />
              <Show when={childrenOf(device.surface_id).length > 0}>
                <div class="nav-child">
                  <For each={childrenOf(device.surface_id)}>
                    {child => <DeviceItem device={child} />}
                  </For>
                </div>
              </Show>
            </>
          )}
        </For>
      </Show>
    </section>
  );
};

const PanelItem: Component<{ panel: Panel; }> = (properties) => {
  const store = useInventory();
  const pressedKeys = createMemo(() => store.pressedKeysForPanel(properties.panel.panel_id));
  const dialLevels = createMemo(() => store.dialLevelsForPanel(properties.panel.panel_id));
  const pressedDials = createMemo(() => store.pressedDialsForPanel(properties.panel.panel_id));

  return (
    <Link to="/panels/$panelId" params={{ panelId: properties.panel.panel_id }} class="nav-tile">
      <span class="nav-tile-head">
        <span class="nav-item-title min-w-0 flex-1">{properties.panel.name}</span>
        <span class="chip chip-muted">{layoutLabel(properties.panel.layout)}</span>
      </span>
      <PanelThumbnail
        panel={properties.panel}
        pressedKeys={pressedKeys()}
        dialLevels={dialLevels()}
        pressedDials={pressedDials()}
      />
    </Link>
  );
};

const PanelNav: Component = () => {
  const store = useInventory();

  return (
    <section class="nav">
      <Link to="/panels" class="nav-heading">
        Panels
        <span class="chip chip-muted">{store.inventory().panels.length}</span>
      </Link>
      <Show when={store.inventory().panels.length > 0} fallback={<p class="nav-empty">No panels yet.</p>}>
        <For each={store.inventory().panels}>{panel => <PanelItem panel={panel} />}</For>
      </Show>
    </section>
  );
};

const PluginNav: Component = () => {
  const store = useInventory();

  return (
    <section class="nav">
      <div class="nav-heading-row">
        <Link to="/plugins" class="nav-heading flex-1">
          Plugins
          <span class="chip chip-muted">{store.plugins().instances.length}</span>
        </Link>
        <AddPluginDialog
          trigger={(
            <button
              type="button"
              class="icon-button"
              aria-label="Add a plugin"
              title="Add a plugin"
            >
              <TbPlus class="h-4 w-4" />
            </button>
          )}
        />
      </div>
      <Show
        when={store.plugins().instances.length > 0}
        fallback={<p class="nav-empty">No plugins configured.</p>}
      >
        <For each={store.plugins().instances}>
          {instance => (
            <Link
              to="/plugins/$integrationId"
              params={{ integrationId: instance.integration_id }}
              class="nav-item"
            >
              <StatusDot status={statusTone(instance.status)} />
              <span class="min-w-0 flex-1">
                <span class="nav-item-title block">{instance.display_name}</span>
                <span class="nav-item-meta block">{instance.plugin_type}</span>
              </span>
            </Link>
          )}
        </For>
      </Show>
    </section>
  );
};

const ValueNav: Component = () => {
  const store = useInventory();
  const sources = createMemo(() => {
    const counts = new Map<string, number>();

    for (const entry of store.values().values) {
      counts.set(entry.integration_id, (counts.get(entry.integration_id) ?? 0) + 1);
    }

    return [...counts].map(([integrationId, count]) => ({ integrationId, count }));
  });

  return (
    <section class="nav">
      <Link to="/values" class="nav-heading">
        Values
        <span class="chip chip-muted">{store.values().values.length}</span>
      </Link>
      <Link to="/values" class="nav-item">
        <span class="min-w-0 flex-1">
          <span class="nav-item-title block">Your values</span>
          <span class="nav-item-meta block">
            {store.values().user_values.length}
            {" "}
            defined
          </span>
        </span>
      </Link>
      <Show when={sources().length > 0} fallback={<p class="nav-empty">Nothing published yet.</p>}>
        <For each={sources()}>
          {source => (
            <Link to="/values" class="nav-item">
              <span class="min-w-0 flex-1">
                <span class="nav-item-title block mono">{source.integrationId}</span>
                <span class="nav-item-meta block">
                  {source.count}
                  {" "}
                  values
                </span>
              </span>
            </Link>
          )}
        </For>
      </Show>
      <Link to="/values" class="nav-item">
        <span class="min-w-0 flex-1">
          <span class="nav-item-title block">Actions</span>
          <span class="nav-item-meta block">
            {store.values().actions.length}
            {" "}
            available
          </span>
        </span>
      </Link>
    </section>
  );
};

export const ContextSidebar: Component = () => {
  const store = useInventory();
  const location = useLocation();
  const section = () => {
    const path = location().pathname;

    if (path.startsWith("/panels")) return "panels";

    if (path.startsWith("/plugins")) return "plugins";

    if (path.startsWith("/values")) return "values";

    return "devices";
  };

  return (
    <aside class="sidebar">
      <div class="sidebar-head">
        <Link to="/devices" class="brand">Launchpi</Link>
        <button
          type="button"
          class="icon-button"
          onClick={() => void store.refresh()}
          aria-label="Refresh inventory"
          title="Refresh inventory"
        >
          <TbRefresh class="h-4 w-4" />
        </button>
      </div>
      <div class="sidebar-body">
        <Switch fallback={<DeviceNav />}>
          <Match when={section() === "panels"}><PanelNav /></Match>
          <Match when={section() === "plugins"}><PluginNav /></Match>
          <Match when={section() === "values"}><ValueNav /></Match>
        </Switch>
      </div>
    </aside>
  );
};
