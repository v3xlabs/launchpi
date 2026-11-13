import { Link } from "@tanstack/solid-router";
import { FiBox, FiGrid, FiHardDrive, FiMoon, FiSliders, FiSun } from "solid-icons/fi";
import { Component, createEffect, createSignal, For, JSX, Match, Show, Switch } from "solid-js";

import { Device, displayName } from "../api/inventory";
import { useInventory } from "../context/InventoryContext";
import { deviceTree } from "../utils/deviceTree";
import { ChangesPopover } from "./ChangesPopover";
import { StatusDot } from "./StatusDot";

const themeKey = "launchpi-theme";

const ThemeToggle: Component = () => {
  const [isDark, setIsDark] = createSignal(document.documentElement.classList.contains("dark"));
  const toggle = () => {
    const isNextDark = !isDark();

    document.documentElement.classList.toggle("dark", isNextDark);
    localStorage.setItem(themeKey, isNextDark ? "dark" : "light");
    setIsDark(isNextDark);
  };
  const label = () => (isDark() ? "Switch to light mode" : "Switch to dark mode");

  return (
    <button
      type="button"
      class="icon-button"
      onClick={toggle}
      aria-label={label()}
      title={label()}
    >
      <Show when={isDark()} fallback={<FiMoon class="size-4" />}>
        <FiSun class="size-4" />
      </Show>
    </button>
  );
};

const ConnectionStatus: Component = () => {
  const store = useInventory();
  const [hasConnected, setHasConnected] = createSignal(false);

  // The socket opens a moment after the first fetch; only a dropped connection is a reconnect.
  createEffect(() => {
    if (store.isConnected()) setHasConnected(true);
  });

  const word = () => {
    if (store.isConnected()) return "Connected";

    return hasConnected() ? "Reconnecting..." : "Connecting...";
  };

  return (
    <span role="status" class="sidebar-status" title={word()}>
      <span
        class="status-dot size-2"
        classList={{ "bg-emerald-500": store.isConnected(), "bg-amber-500": !store.isConnected() }}
        aria-hidden="true"
      />
      <span class="sidebar-label">{word()}</span>
    </span>
  );
};

type NavItemProperties = { to: "/devices" | "/panels" | "/plugins" | "/values"; label: string; icon: JSX.Element; };

const NavItem: Component<NavItemProperties> = properties => (
  <Link to={properties.to} class="nav-item" activeOptions={{ exact: properties.to === "/devices" }}>
    {properties.icon}
    <span class="sidebar-label">{properties.label}</span>
  </Link>
);

const TreeItem: Component<{ device: Device; }> = properties => (
  <Link to="/devices/$surfaceId" params={{ surfaceId: properties.device.surface_id }} class="tree-item">
    <span class="min-w-0 flex-1 truncate">{displayName(properties.device.name)}</span>
    <StatusDot status={properties.device.status} />
  </Link>
);

const DeviceTree: Component = () => {
  const store = useInventory();

  return (
    <Show when={store.inventory().devices.length > 0}>
      <div class="tree">
        <For each={deviceTree(store.inventory().devices)}>
          {entry => (
            <Switch>
              <Match when={entry.kind === "device" && entry}>
                {deviceEntry => <TreeItem device={deviceEntry().device} />}
              </Match>
              <Match when={entry.kind === "group" && entry}>
                {group => (
                  <div role="group" aria-label={displayName(group().link.name)} class="grid gap-0.5">
                    <span class="tree-group-label">{displayName(group().link.name)}</span>
                    <For each={group().children}>{child => <TreeItem device={child} />}</For>
                  </div>
                )}
              </Match>
            </Switch>
          )}
        </For>
      </div>
    </Show>
  );
};

export const Sidebar: Component<{ elementId: string; }> = (properties) => {
  const store = useInventory();

  return (
    <div id={properties.elementId} class="sidebar-content">
      <Link to="/devices" class="sidebar-brand">
        <img src="/icon.svg" class="size-6 shrink-0" alt="" />
        <span class="sidebar-label">Launchpi</span>
      </Link>
      <nav class="sidebar-nav" aria-label="Sections">
        <NavItem to="/devices" label="Devices" icon={<FiHardDrive class="size-4 shrink-0" />} />
        <DeviceTree />
        <NavItem to="/panels" label="Panels" icon={<FiGrid class="size-4 shrink-0" />} />
        <NavItem to="/plugins" label="Plugins" icon={<FiBox class="size-4 shrink-0" />} />
        <NavItem to="/values" label="Values" icon={<FiSliders class="size-4 shrink-0" />} />
      </nav>
      <div class="sidebar-foot">
        <Show when={store.inventory().config.mode === "declarative" && store.inventory().config.changes > 0}>
          <ChangesPopover />
        </Show>
        <div class="flex items-center gap-2 sidebar-rail-stack">
          <ConnectionStatus />
          <ThemeToggle />
        </div>
      </div>
    </div>
  );
};
