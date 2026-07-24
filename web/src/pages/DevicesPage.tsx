import { Link, useNavigate } from '@tanstack/solid-router';
import { Component, createMemo, For, Show } from 'solid-js';
import { TbFillDeviceDesktop as TbDeviceDesktop, TbFillTrash as TbTrash } from 'solid-icons/tb';

import { deviceGridLayout, isPanelCompatible } from '../api/inventory';
import { StatusDot } from '../components/StatusDot';
import { useInventory } from '../context/InventoryContext';
import { AddDeviceDialog } from '../dialogs/AddDeviceDialog';

export const DevicesPage: Component<{ surfaceId?: string }> = (props) => {
    const store = useInventory();
    const navigate = useNavigate();

    const device = createMemo(
        () => store.inventory().devices.find((entry) => entry.surface_id === props.surfaceId) ?? null,
    );
    const compatiblePanels = createMemo(() => {
        const current = device();
        return current === null
            ? []
            : store.inventory().panels.filter((panel) => isPanelCompatible(current, panel));
    });

    const childDevicesOf = (surfaceId: string) =>
        store.inventory().devices.filter((entry) => entry.parent_surface_id === surfaceId);

    const remove = async (surfaceId: string) => {
        await store.removeDevice(surfaceId);
        navigate({ to: '/devices' });
    };

    return (
        <div class="page">
            <Show when={device()} fallback={<DevicesOverview />}>
                {(current) => {
                    const layout = () => deviceGridLayout(current().layout);
                    return (
                        <div class="space-y-6">
                            <div class="flex flex-wrap items-start justify-between gap-4">
                                <div>
                                    <div class="flex items-center gap-2 text-sm capitalize text-neutral-400">
                                        <StatusDot status={current().status} class="h-2 w-2" />
                                        {current().status}
                                    </div>
                                    <h1 class="mt-1 text-2xl font-semibold tracking-tight">{current().name}</h1>
                                    <p class="mt-1 text-sm text-neutral-400">
                                        {current().host}:{current().port} ·{' '}
                                        {layout() === null
                                            ? `${current().model} (freeform)`
                                            : `${layout()?.columns} × ${layout()?.rows} · ${current().model}`}
                                    </p>
                                </div>
                                <div class="flex gap-2">
                                    <button
                                        type="button"
                                        class="secondary-button"
                                        onClick={() =>
                                            void store.setDeviceEnabled(
                                                current().surface_id,
                                                !current().is_enabled,
                                            )
                                        }
                                        disabled={store.isSaving()}
                                    >
                                        {current().is_enabled ? 'Disable' : 'Enable'}
                                    </button>
                                    <button
                                        type="button"
                                        class="icon-button hover:border-rose-500/60 hover:text-rose-300"
                                        onClick={() => void remove(current().surface_id)}
                                        aria-label={`Remove ${current().name}`}
                                    >
                                        <TbTrash />
                                    </button>
                                </div>
                            </div>

                            <Show when={current().last_error}>
                                {(message) => (
                                    <p class="border border-rose-500/40 bg-rose-950/30 px-4 py-3 text-sm text-rose-200">
                                        {message()}
                                    </p>
                                )}
                            </Show>

                            <Show when={layout()}>
                                <div class="max-w-md space-y-2">
                                    <label class="field-label">
                                        Active panel
                                        <select
                                            class="field-input"
                                            value={current().active_panel_id ?? ''}
                                            onChange={(event) => {
                                                if (event.currentTarget.value)
                                                    void store.assignPanel(
                                                        current().surface_id,
                                                        event.currentTarget.value,
                                                    );
                                            }}
                                        >
                                            <option value="" disabled>
                                                Select a compatible panel
                                            </option>
                                            <For each={compatiblePanels()}>
                                                {(panel) => (
                                                    <option value={panel.panel_id}>
                                                        {panel.name} · {panel.layout.columns}×{panel.layout.rows}
                                                    </option>
                                                )}
                                            </For>
                                        </select>
                                    </label>
                                    <p class="text-xs text-neutral-500">
                                        {compatiblePanels().length} compatible panel
                                        {compatiblePanels().length === 1 ? '' : 's'} for this layout and capability
                                        profile.
                                    </p>
                                </div>
                            </Show>

                            <Show when={childDevicesOf(current().surface_id).length > 0}>
                                <div class="max-w-md space-y-2">
                                    <p class="section-title">Attached devices</p>
                                    <For each={childDevicesOf(current().surface_id)}>
                                        {(child) => (
                                            <Link
                                                to="/devices/$surfaceId"
                                                params={{ surfaceId: child.surface_id }}
                                                class="sidebar-item border border-neutral-800"
                                            >
                                                <StatusDot status={child.status} class="h-2.5 w-2.5" />
                                                <span class="min-w-0 flex-1">
                                                    <span class="block truncate text-sm font-medium">
                                                        {child.model}
                                                    </span>
                                                    <span class="block truncate text-xs text-neutral-500">
                                                        {child.host}:{child.port}
                                                    </span>
                                                </span>
                                            </Link>
                                        )}
                                    </For>
                                </div>
                            </Show>
                        </div>
                    );
                }}
            </Show>
        </div>
    );
};

const DevicesOverview: Component = () => {
    const store = useInventory();

    return (
        <div class="space-y-8">
            <div class="flex flex-wrap items-start justify-between gap-4">
                <div>
                    <p class="eyebrow">Devices</p>
                    <h1 class="mt-1 text-2xl font-semibold tracking-tight">Connected hardware</h1>
                    <p class="mt-2 max-w-xl text-sm leading-6 text-neutral-400">
                        Pick a device from the sidebar to assign a panel, claim a discovered one, or add one by
                        address.
                    </p>
                </div>
                <AddDeviceDialog
                    trigger={
                        <button type="button" class="primary-button">
                            <span aria-hidden="true">+</span>
                            Add device
                        </button>
                    }
                />
            </div>

            <Show
                when={store.inventory().discovered.length > 0}
                fallback={
                    <p class="empty-state">
                        No devices discovered yet. Add one by address with “Add device” above.
                    </p>
                }
            >
                <div class="space-y-2">
                    <div class="flex items-center gap-2 text-sm text-neutral-300">
                        <TbDeviceDesktop class="h-5 w-5 text-cyan-300" />
                        {store.inventory().discovered.length} discovered on the network
                    </div>
                    <div class="flex flex-wrap gap-2">
                        <For each={store.inventory().discovered}>
                            {(discovered) => (
                                <button
                                    type="button"
                                    class="discovery-device"
                                    onClick={() => void store.addDiscovered(discovered.discovery_id)}
                                    disabled={store.isSaving()}
                                >
                                    <span class="text-left">
                                        <span class="block font-semibold">{discovered.name}</span>
                                        <span class="block text-xs font-normal text-neutral-400">
                                            {discovered.host} · {discovered.model}
                                        </span>
                                    </span>
                                    <span aria-hidden="true" class="text-lg leading-none">+</span>
                                </button>
                            )}
                        </For>
                    </div>
                </div>
            </Show>
        </div>
    );
};
