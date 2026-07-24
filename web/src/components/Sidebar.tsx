import { Link } from '@tanstack/solid-router';
import { Component, For, Show } from 'solid-js';
import { TbFillExchange as TbRefresh, TbFillLayoutGrid as TbLayoutGrid } from 'solid-icons/tb';

import { Device, deviceGridLayout } from '../api/inventory';
import { useInventory } from '../context/InventoryContext';
import { StatusDot } from './StatusDot';

const DeviceLink: Component<{ device: Device }> = (props) => {
    const layout = () => deviceGridLayout(props.device.layout);
    return (
        <Link to="/devices/$surfaceId" params={{ surfaceId: props.device.surface_id }} class="sidebar-item">
            <StatusDot status={props.device.status} class="h-2.5 w-2.5" />
            <span class="min-w-0 flex-1">
                <span class="block truncate text-sm font-medium">{props.device.name}</span>
                <span class="block truncate text-xs text-neutral-500">
                    {layout() === null
                        ? props.device.model
                        : `${layout()?.columns}×${layout()?.rows} · ${props.device.model}`}
                </span>
            </span>
        </Link>
    );
};

export const Sidebar: Component = () => {
    const store = useInventory();
    const topLevelDevices = () => store.inventory().devices.filter((device) => !device.parent_surface_id);
    const childrenOf = (surfaceId: string) =>
        store.inventory().devices.filter((device) => device.parent_surface_id === surfaceId);

    return (
        <div class="sidebar">
            <Link to="/devices" class="flex items-center gap-3 px-5 py-5 no-underline">
                <div class="grid h-9 w-9 place-items-center bg-cyan-300 text-neutral-950">
                    <TbLayoutGrid class="h-5 w-5" />
                </div>
                <div>
                    <p class="text-[0.68rem] font-bold uppercase tracking-[0.22em] text-cyan-300">Launchpi</p>
                    <p class="text-sm font-semibold tracking-tight text-neutral-100">Control workspace</p>
                </div>
            </Link>

            <nav class="flex-1 space-y-6 overflow-y-auto px-3 pb-4">
                <section class="space-y-1">
                    <Link
                        to="/devices"
                        class="sidebar-section-heading block no-underline transition hover:text-neutral-300"
                    >
                        Devices
                    </Link>
                    <Show
                        when={topLevelDevices().length > 0}
                        fallback={<p class="sidebar-empty">No devices yet.</p>}
                    >
                        <For each={topLevelDevices()}>
                            {(device) => (
                                <>
                                    <DeviceLink device={device} />
                                    <Show when={childrenOf(device.surface_id).length > 0}>
                                        <div class="sidebar-child space-y-1">
                                            <For each={childrenOf(device.surface_id)}>
                                                {(child) => <DeviceLink device={child} />}
                                            </For>
                                        </div>
                                    </Show>
                                </>
                            )}
                        </For>
                    </Show>
                    <Show when={store.inventory().discovered.length > 0}>
                        <Link to="/devices" class="sidebar-discovered">
                            {store.inventory().discovered.length} discovered on the network
                        </Link>
                    </Show>
                </section>

                <section class="space-y-1">
                    <Link
                        to="/panels"
                        class="sidebar-section-heading block no-underline transition hover:text-neutral-300"
                    >
                        Panels
                    </Link>
                    <Show
                        when={store.inventory().panels.length > 0}
                        fallback={<p class="sidebar-empty">No panels yet.</p>}
                    >
                        <For each={store.inventory().panels}>
                            {(panel) => (
                                <Link
                                    to="/panels/$panelId"
                                    params={{ panelId: panel.panel_id }}
                                    class="sidebar-item"
                                >
                                    <TbLayoutGrid class="h-4 w-4 shrink-0 text-neutral-500" />
                                    <span class="min-w-0 flex-1">
                                        <span class="block truncate text-sm font-medium">{panel.name}</span>
                                        <span class="block truncate text-xs text-neutral-500">
                                            {panel.layout.columns}×{panel.layout.rows} · {panel.controls.length}{' '}
                                            controls
                                        </span>
                                    </span>
                                </Link>
                            )}
                        </For>
                    </Show>
                </section>
            </nav>

            <div class="flex items-center justify-between border-t border-neutral-800 px-4 py-3">
                <span class="text-xs text-neutral-600">Auto-saved</span>
                <button
                    type="button"
                    class="icon-button"
                    onClick={() => void store.refresh()}
                    aria-label="Refresh inventory"
                >
                    <TbRefresh />
                </button>
            </div>
        </div>
    );
};
