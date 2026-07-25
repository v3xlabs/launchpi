import { Link, useNavigate } from '@tanstack/solid-router';
import { Component, createEffect, createMemo, createSignal, For, Show } from 'solid-js';
import { TbFillInfoCircle as TbInfo, TbFillTrash as TbTrash } from 'solid-icons/tb';

import {
    capabilityLabels,
    Device,
    deviceGridLayout,
    DiscoveredDevice,
    displayName,
    isPanelCompatible,
    layoutLabel,
    Panel,
} from '../api/inventory';
import { DeviceImage } from '../components/DeviceImage';
import { PanelThumbnail } from '../components/PanelPreview';
import { StatusDot, StatusLabel } from '../components/StatusDot';
import { useInventory } from '../context/InventoryContext';
import { AddDeviceDialog } from '../dialogs/AddDeviceDialog';

export const DevicesPage: Component<{ surfaceId?: string }> = (props) => {
    const store = useInventory();
    const device = createMemo(
        () => store.inventory().devices.find((entry) => entry.surface_id === props.surfaceId) ?? null,
    );

    return (
        <div class="page">
            <Show when={device()} fallback={<DevicesOverview />}>
                {(current) => <DeviceDetail device={current()} />}
            </Show>
        </div>
    );
};

const DetailRow: Component<{ label: string; value: string }> = (props) => (
    <div class="flex items-baseline justify-between gap-3 px-3 py-1.5">
        <span class="text-xs text-neutral-500">{props.label}</span>
        <span class="mono truncate text-neutral-300">{props.value}</span>
    </div>
);

const DeviceDetail: Component<{ device: Device }> = (props) => {
    const store = useInventory();
    const navigate = useNavigate();

    const layout = () => deviceGridLayout(props.device.layout);
    const children = () =>
        store.inventory().devices.filter((entry) => entry.parent_surface_id === props.device.surface_id);
    const parent = () =>
        store.inventory().devices.find((entry) => entry.surface_id === props.device.parent_surface_id) ?? null;
    const compatiblePanels = () =>
        store.inventory().panels.filter((panel) => isPanelCompatible(props.device, panel));
    const activePanel = (): Panel | null =>
        store.inventory().panels.find((panel) => panel.panel_id === props.device.active_panel_id) ?? null;
    const pressedKeys = () => new Set(store.pressedKeysFor(props.device.surface_id));
    const dialLevels = () => store.dialLevelsFor(props.device.surface_id);
    const pressedDials = () => store.pressedDialsFor(props.device.surface_id);

    const remove = async () => {
        await store.removeDevice(props.device.surface_id);
        navigate({ to: '/devices' });
    };

    return (
        <>
            <div class="page-head">
                <div class="flex min-w-0 items-start gap-3">
                    <DeviceImage model={props.device.model} class="hidden h-16 w-24 sm:block" />
                    <div class="min-w-0">
                        <p class="breadcrumb">
                            <Link to="/devices">Devices</Link>
                            <span class="meta-sep">/</span>
                            <Show when={parent()}>
                                {(entry) => (
                                    <>
                                        <Link
                                            to="/devices/$surfaceId"
                                            params={{ surfaceId: entry().surface_id }}
                                        >
                                            {displayName(entry().name)}
                                        </Link>
                                        <span class="meta-sep">/</span>
                                    </>
                                )}
                            </Show>
                            <span class="text-neutral-400">{props.device.model}</span>
                        </p>
                        <h1 class="page-title mt-1">{displayName(props.device.name)}</h1>
                        <div class="meta-line">
                            <StatusLabel status={props.device.status} />
                            <span class="meta-sep">·</span>
                            <span>{layoutLabel(layout())}</span>
                            <span class="meta-sep">·</span>
                            <span class="mono">
                                {props.device.host}:{props.device.port}
                            </span>
                        </div>
                    </div>
                </div>
                <div class="flex gap-2">
                    <button
                        type="button"
                        class="secondary-button"
                        onClick={() =>
                            void store.setDeviceEnabled(props.device.surface_id, !props.device.is_enabled)
                        }
                        disabled={store.isSaving()}
                    >
                        {props.device.is_enabled ? 'Disable' : 'Enable'}
                    </button>
                    <button
                        type="button"
                        class="danger-button"
                        onClick={() => void remove()}
                        aria-label={`Remove ${displayName(props.device.name)}`}
                        title="Remove device"
                    >
                        <TbTrash class="h-4 w-4" />
                    </button>
                </div>
            </div>

            <Show when={props.device.last_error}>
                {(message) => (
                    <p role="alert" class="alert">
                        {message()}
                    </p>
                )}
            </Show>

            <div class="grid items-start gap-4 xl:grid-cols-[minmax(0,1fr)_18rem]">
                <div class="grid gap-4">
                    <Show
                        when={layout()}
                        fallback={
                            <div class="card">
                                <div class="card-head">
                                    <p class="card-title">Active panel</p>
                                </div>
                                <p class="empty">
                                    This device has no keys of its own. Attached devices below carry the panels.
                                </p>
                            </div>
                        }
                    >
                        <div class="card">
                            <div class="card-head">
                                <p class="card-title">Active panel</p>
                                <Show when={activePanel()}>
                                    {(panel) => (
                                        <Link
                                            to="/panels/$panelId"
                                            params={{ panelId: panel().panel_id }}
                                            class="link-button"
                                        >
                                            Edit panel
                                        </Link>
                                    )}
                                </Show>
                            </div>
                            <div class="card-body">
                                <label class="field-label max-w-sm">
                                    Panel
                                    <select
                                        class="field-input"
                                        value={props.device.active_panel_id ?? ''}
                                        onChange={(event) => {
                                            if (event.currentTarget.value)
                                                void store.assignPanel(
                                                    props.device.surface_id,
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
                                                    {panel.name} · {layoutLabel(panel.layout)}
                                                </option>
                                            )}
                                        </For>
                                    </select>
                                </label>
                                <Show
                                    when={activePanel()}
                                    fallback={
                                        <p class="hint">
                                            {compatiblePanels().length} panel
                                            {compatiblePanels().length === 1 ? '' : 's'} match this layout and
                                            capability profile.
                                        </p>
                                    }
                                >
                                    {(panel) => (
                                        <PanelThumbnail
                                            panel={panel()}
                                            pressedKeys={pressedKeys()}
                                            dialLevels={dialLevels()}
                                            pressedDials={pressedDials()}
                                        />
                                    )}
                                </Show>
                            </div>
                        </div>
                    </Show>

                    <Show when={children().length > 0}>
                        <div class="card">
                            <div class="card-head">
                                <p class="card-title">Attached devices</p>
                                <span class="chip chip-muted">{children().length}</span>
                            </div>
                            <div class="rows">
                                <For each={children()}>{(child) => <DeviceRow device={child} />}</For>
                            </div>
                        </div>
                    </Show>

                    <DeviceLog surfaceId={props.device.surface_id} />
                </div>

                <div class="grid gap-4">
                    <div class="card">
                        <div class="card-head">
                            <p class="card-title">Connection</p>
                        </div>
                        <div class="rows">
                            <DetailRow label="Model" value={props.device.model} />
                            <DetailRow label="Host" value={props.device.host} />
                            <DetailRow label="Port" value={String(props.device.port)} />
                            <DetailRow label="Serial" value={props.device.serial_number ?? '—'} />
                            <DetailRow label="Layout" value={layoutLabel(layout())} />
                            <DetailRow label="Enabled" value={props.device.is_enabled ? 'yes' : 'no'} />
                        </div>
                    </div>
                    <div class="card">
                        <div class="card-head">
                            <p class="card-title">Capabilities</p>
                        </div>
                        <div class="card-body">
                            <div class="flex flex-wrap gap-1.5">
                                <For each={capabilityLabels}>
                                    {({ key, label }) => (
                                        <span
                                            classList={{
                                                chip: true,
                                                'chip-accent': props.device.capabilities[key],
                                                'chip-muted': !props.device.capabilities[key],
                                            }}
                                        >
                                            {label}
                                        </span>
                                    )}
                                </For>
                            </div>
                        </div>
                    </div>
                </div>
            </div>
        </>
    );
};

const logTime = (atMs: number): string => {
    const at = new Date(atMs);
    const pad = (value: number, length = 2) => String(value).padStart(length, '0');
    return `${pad(at.getHours())}:${pad(at.getMinutes())}:${pad(at.getSeconds())}.${pad(
        at.getMilliseconds(),
        3,
    )}`;
};

const DeviceLog: Component<{ surfaceId: string }> = (props) => {
    const store = useInventory();
    const entries = createMemo(() => store.logsFor(props.surfaceId));
    const [isPinned, setIsPinned] = createSignal(true);
    let container: HTMLDivElement | undefined;

    const onScroll = () => {
        if (container === undefined) return;
        setIsPinned(container.scrollHeight - container.scrollTop - container.clientHeight < 24);
    };

    // Tails like a terminal: follow new lines unless the reader has scrolled up to look at something.
    createEffect(() => {
        entries().length;
        if (isPinned() && container !== undefined) container.scrollTop = container.scrollHeight;
    });

    return (
        <div class="card">
            <div class="card-head">
                <p class="card-title">Log</p>
                <span class="chip chip-muted">{entries().length}</span>
            </div>
            <Show when={entries().length > 0} fallback={<p class="empty">No events yet.</p>}>
                <div class="log" ref={container} onScroll={onScroll}>
                    <For each={entries()}>
                        {(entry) => (
                            <div class="log-row" data-level={entry.level}>
                                <span class="log-time">{logTime(entry.at_ms)}</span>
                                <span class="log-message">{entry.message}</span>
                            </div>
                        )}
                    </For>
                </div>
            </Show>
        </div>
    );
};

const DeviceRow: Component<{ device: Device; isChild?: boolean }> = (props) => {
    const store = useInventory();
    const layout = () => deviceGridLayout(props.device.layout);
    const activePanel = () =>
        store.inventory().panels.find((panel) => panel.panel_id === props.device.active_panel_id) ?? null;

    return (
        <div classList={{ row: true, 'row-child': props.isChild }}>
            <Link
                to="/devices/$surfaceId"
                params={{ surfaceId: props.device.surface_id }}
                class="row-main"
                aria-label={`Open ${displayName(props.device.name)}`}
            >
                <StatusDot status={props.device.status} />
                <DeviceImage model={props.device.model} class="h-10 w-16" />
                <span class="min-w-0 flex-1">
                    <span class="row-title block">{displayName(props.device.name)}</span>
                    <span class="row-meta block">
                        {props.device.model} · {props.device.host}:{props.device.port}
                    </span>
                </span>
                <span class="chip">{layoutLabel(layout())}</span>
                <span class="hidden w-40 shrink-0 text-xs text-neutral-500 sm:block">
                    <Show when={activePanel()} fallback={<span class="text-neutral-600">No panel</span>}>
                        {(panel) => <span class="truncate text-neutral-300">{panel().name}</span>}
                    </Show>
                </span>
            </Link>
            <button
                type="button"
                class="secondary-button"
                onClick={() => void store.setDeviceEnabled(props.device.surface_id, !props.device.is_enabled)}
                disabled={store.isSaving()}
            >
                {props.device.is_enabled ? 'Disable' : 'Enable'}
            </button>
        </div>
    );
};

const isSameEndpoint = (device: Device, discovered: DiscoveredDevice): boolean =>
    device.serial_number !== null && discovered.serial_number !== null
        ? device.serial_number === discovered.serial_number
        : device.host === discovered.host && device.port === discovered.port;

const DiscoveredRow: Component<{ discovered: DiscoveredDevice }> = (props) => {
    const store = useInventory();
    const isAdded = () => store.inventory().devices.some((device) => isSameEndpoint(device, props.discovered));

    return (
        <div class="row">
            <DeviceImage model={props.discovered.model} class="h-10 w-16" />
            <span class="min-w-0 flex-1">
                <span class="row-title block">{displayName(props.discovered.name)}</span>
                <span class="row-meta block">
                    {props.discovered.model} · {props.discovered.host}:{props.discovered.port}
                </span>
            </span>
            <Show when={props.discovered.serial_number}>
                {(serial) => <span class="mono hidden sm:block">{serial()}</span>}
            </Show>
            <Show
                when={!isAdded()}
                fallback={<span class="chip chip-muted">Added</span>}
            >
                <button
                    type="button"
                    class="primary-button"
                    onClick={() => void store.addDiscovered(props.discovered.discovery_id)}
                    disabled={store.isSaving()}
                >
                    Add
                </button>
            </Show>
        </div>
    );
};

const DevicesOverview: Component = () => {
    const store = useInventory();
    const rootDevices = () => store.inventory().devices.filter((device) => device.parent_surface_id === null);
    const childrenOf = (surfaceId: string) =>
        store.inventory().devices.filter((device) => device.parent_surface_id === surfaceId);

    return (
        <>
            <div class="page-head">
                <div>
                    <h1 class="page-title">Devices</h1>
                    <p class="page-subtitle">
                        Surfaces the daemon connects to, and everything it currently sees on the network.
                    </p>
                </div>
                <div class="flex items-center gap-2">
                    <AddDeviceDialog
                        trigger={
                            <button type="button" class="primary-button">
                                Add device
                            </button>
                        }
                    />
                    <Link to="/devices/supported" class="icon-button" title="Supported devices">
                        <TbInfo class="h-4 w-4" />
                    </Link>
                </div>
            </div>

            <div class="card">
                <div class="card-head">
                    <p class="card-title">Configured</p>
                    <span class="chip chip-muted">{store.inventory().devices.length}</span>
                </div>
                <Show
                    when={rootDevices().length > 0}
                    fallback={<p class="empty">No devices yet. Add one below or by address.</p>}
                >
                    <div class="rows">
                        <For each={rootDevices()}>
                            {(device) => (
                                <>
                                    <DeviceRow device={device} />
                                    <For each={childrenOf(device.surface_id)}>
                                        {(child) => <DeviceRow device={child} isChild />}
                                    </For>
                                </>
                            )}
                        </For>
                    </div>
                </Show>
            </div>

            <div class="card">
                <div class="card-head">
                    <p class="card-title">Discovered on the network</p>
                    <span class="chip chip-muted">{store.inventory().discovered.length}</span>
                </div>
                <Show
                    when={store.inventory().discovered.length > 0}
                    fallback={
                        <p class="empty">
                            Nothing announced over mDNS yet. Devices on another subnet have to be added by
                            address.
                        </p>
                    }
                >
                    <div class="rows">
                        <For each={store.inventory().discovered}>
                            {(discovered) => <DiscoveredRow discovered={discovered} />}
                        </For>
                    </div>
                </Show>
            </div>
        </>
    );
};
