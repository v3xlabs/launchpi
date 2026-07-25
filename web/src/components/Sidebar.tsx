import { Link } from '@tanstack/solid-router';
import { Component, For, Show } from 'solid-js';

import { Device, deviceGridLayout, displayName, layoutLabel } from '../api/inventory';
import { useInventory } from '../context/InventoryContext';
import { DeviceImage } from './DeviceImage';
import { StatusDot } from './StatusDot';

const DeviceItem: Component<{ device: Device }> = (props) => (
    <Link to="/devices/$surfaceId" params={{ surfaceId: props.device.surface_id }} class="nav-item">
        <StatusDot status={props.device.status} />
        <DeviceImage model={props.device.model} class="h-7 w-10" />
        <span class="min-w-0 flex-1">
            <span class="nav-item-title block">{displayName(props.device.name)}</span>
            <span class="nav-item-meta block">
                {layoutLabel(deviceGridLayout(props.device.layout))} · {props.device.model}
            </span>
        </span>
    </Link>
);

export const Sidebar: Component = () => {
    const store = useInventory();
    const rootDevices = () => store.inventory().devices.filter((device) => device.parent_surface_id === null);
    const childrenOf = (surfaceId: string) =>
        store.inventory().devices.filter((device) => device.parent_surface_id === surfaceId);

    return (
        <div class="nav">
            <section>
                <Link to="/devices" class="nav-heading">
                    Devices
                    <span class="chip chip-muted">{store.inventory().devices.length}</span>
                </Link>
                <Show when={rootDevices().length > 0} fallback={<p class="nav-empty">No devices added.</p>}>
                    <For each={rootDevices()}>
                        {(device) => (
                            <>
                                <DeviceItem device={device} />
                                <Show when={childrenOf(device.surface_id).length > 0}>
                                    <div class="nav-child">
                                        <For each={childrenOf(device.surface_id)}>
                                            {(child) => <DeviceItem device={child} />}
                                        </For>
                                    </div>
                                </Show>
                            </>
                        )}
                    </For>
                </Show>
            </section>

            <section>
                <Link to="/panels" class="nav-heading">
                    Panels
                    <span class="chip chip-muted">{store.inventory().panels.length}</span>
                </Link>
                <Show when={store.inventory().panels.length > 0} fallback={<p class="nav-empty">No panels yet.</p>}>
                    <For each={store.inventory().panels}>
                        {(panel) => (
                            <Link to="/panels/$panelId" params={{ panelId: panel.panel_id }} class="nav-item">
                                <span class="chip">{layoutLabel(panel.layout)}</span>
                                <span class="min-w-0 flex-1">
                                    <span class="nav-item-title block">{panel.name}</span>
                                    <span class="nav-item-meta block">{panel.controls.length} controls</span>
                                </span>
                            </Link>
                        )}
                    </For>
                </Show>
            </section>
        </div>
    );
};
