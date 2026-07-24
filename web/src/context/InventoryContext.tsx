import {
    Accessor,
    createContext,
    createEffect,
    createResource,
    createSignal,
    onCleanup,
    onMount,
    ParentComponent,
    useContext,
} from 'solid-js';
import { createStore, produce, reconcile } from 'solid-js/store';

import * as api from '../api/inventory';
import { Control, Inventory, KeyEvent, Panel } from '../api/inventory';

const isRecord = (value: unknown): value is Record<string, unknown> =>
    typeof value === 'object' && value !== null;

type KeyStateEvent = { type: 'key_state'; surface_id: string; key_index: number; is_pressed: boolean };
const asKeyStateEvent = (value: Record<string, unknown>): KeyStateEvent | null =>
    value.type === 'key_state' &&
    typeof value.surface_id === 'string' &&
    typeof value.key_index === 'number' &&
    typeof value.is_pressed === 'boolean'
        ? { type: 'key_state', surface_id: value.surface_id, key_index: value.key_index, is_pressed: value.is_pressed }
        : null;

const groupPressedKeys = (keyStates: KeyEvent[]): Record<string, number[]> => {
    const grouped: Record<string, number[]> = {};
    for (const keyState of keyStates) {
        if (!keyState.is_pressed) continue;
        (grouped[keyState.surface_id] ??= []).push(keyState.key_index);
    }
    return grouped;
};

export type ControlClipboard = Pick<
    Control,
    'name' | 'default_state' | 'pressed_state' | 'action_bindings' | 'feedback_bindings'
>;

export type InventoryStore = {
    inventory: Accessor<Inventory>;
    isLoading: Accessor<boolean>;
    isSaving: Accessor<boolean>;
    error: Accessor<string | null>;
    setError: (message: string | null) => void;
    refresh: () => Promise<void>;
    addDevice: (input: api.AddDeviceInput) => Promise<boolean>;
    addDiscovered: (discoveryId: string) => Promise<void>;
    setDeviceEnabled: (surfaceId: string, isEnabled: boolean) => Promise<void>;
    removeDevice: (surfaceId: string) => Promise<void>;
    assignPanel: (surfaceId: string, panelId: string) => Promise<void>;
    createPanel: (input: api.CreatePanelInput) => Promise<Panel | null>;
    savePanel: (panel: Panel) => Promise<void>;
    exportPanel: (panel: Panel) => Promise<void>;
    saveConfiguration: () => Promise<void>;
    clipboard: Accessor<ControlClipboard | null>;
    copyControl: (control: Control) => void;
    clearClipboard: () => void;
    pressedKeysFor: (surfaceId: string) => number[];
    pressedKeysForPanel: (panelId: string) => Set<number>;
};

const InventoryContext = createContext<InventoryStore>();

const toClipboard = (control: Control): ControlClipboard => ({
    name: control.name,
    default_state: control.default_state,
    pressed_state: control.pressed_state,
    action_bindings: control.action_bindings,
    feedback_bindings: control.feedback_bindings,
});

export const InventoryProvider: ParentComponent = (props) => {
    const [resource, { refetch }] = createResource<Inventory>(api.fetchInventory);
    const [snapshot, setSnapshot] = createStore<Inventory>(api.emptyInventory);
    const [pressedBySurface, setPressedBySurface] = createStore<Record<string, number[]>>({});
    const [isSaving, setIsSaving] = createSignal(false);
    const [error, setError] = createSignal<string | null>(null);
    const [clipboard, setClipboard] = createSignal<ControlClipboard | null>(null);

    // Reconcile fetched data into stores so unchanged rows keep their identity (no re-mount / flicker).
    // Live key presses arrive over the WebSocket below; a fetch only resyncs the authoritative baseline.
    createEffect(() => {
        const data = resource();
        if (data === undefined) return;
        setSnapshot(reconcile(data));
        setPressedBySurface(reconcile(groupPressedKeys(data.key_states)));
    });

    const inventory: Accessor<Inventory> = () => snapshot;
    const isLoading: Accessor<boolean> = () => resource.loading && resource() === undefined;
    const pressedKeysFor = (surfaceId: string): number[] => pressedBySurface[surfaceId] ?? [];
    const pressedKeysForPanel = (panelId: string): Set<number> => {
        const pressed = new Set<number>();
        for (const device of snapshot.devices) {
            if (device.active_panel_id !== panelId) continue;
            for (const keyIndex of pressedKeysFor(device.surface_id)) pressed.add(keyIndex);
        }
        return pressed;
    };

    const refresh = async () => {
        try {
            await refetch();
            setError(null);
        } catch (refreshError) {
            setError(refreshError instanceof Error ? refreshError.message : 'Unable to reach the daemon.');
        }
    };

    const setKeyPressed = (surfaceId: string, keyIndex: number, isPressed: boolean) => {
        setPressedBySurface(
            produce((state) => {
                const list = (state[surfaceId] ??= []);
                const index = list.indexOf(keyIndex);
                if (isPressed && index === -1) list.push(keyIndex);
                else if (!isPressed && index !== -1) list.splice(index, 1);
            }),
        );
    };

    onMount(() => {
        let socket: WebSocket | null = null;
        let reconnectTimer: ReturnType<typeof setTimeout> | undefined;
        let changedTimer: ReturnType<typeof setTimeout> | undefined;
        let isClosed = false;

        const scheduleResync = () => {
            if (changedTimer !== undefined) return;
            changedTimer = setTimeout(() => {
                changedTimer = undefined;
                void refetch();
            }, 150);
        };

        const handleMessage = (raw: string) => {
            let parsed: unknown;
            try {
                parsed = JSON.parse(raw);
            } catch {
                return;
            }
            if (!isRecord(parsed)) return;
            const keyState = asKeyStateEvent(parsed);
            if (keyState !== null) {
                setKeyPressed(keyState.surface_id, keyState.key_index, keyState.is_pressed);
                return;
            }
            if (parsed.type === 'changed') scheduleResync();
        };

        const connect = () => {
            const protocol = window.location.protocol === 'https:' ? 'wss' : 'ws';
            socket = new WebSocket(`${protocol}://${window.location.host}/api/events`);
            socket.addEventListener('open', () => void refresh());
            socket.addEventListener('message', (event) => {
                if (typeof event.data === 'string') handleMessage(event.data);
            });
            socket.addEventListener('close', () => {
                if (!isClosed) reconnectTimer = setTimeout(connect, 1000);
            });
            socket.addEventListener('error', () => socket?.close());
        };
        connect();

        onCleanup(() => {
            isClosed = true;
            clearTimeout(reconnectTimer);
            clearTimeout(changedTimer);
            socket?.close();
        });
    });

    const run = async (operation: () => Promise<void>): Promise<boolean> => {
        setIsSaving(true);
        try {
            await operation();
            setError(null);
            return true;
        } catch (operationError) {
            setError(
                operationError instanceof Error ? operationError.message : 'Unable to complete the request.',
            );
            return false;
        } finally {
            setIsSaving(false);
        }
    };

    const store: InventoryStore = {
        inventory,
        isLoading,
        isSaving,
        error,
        setError,
        refresh,
        addDevice: (input) =>
            run(async () => {
                await api.addDevice(input);
                await refetch();
            }),
        addDiscovered: async (discoveryId) => {
            await run(async () => {
                await api.addDiscoveredDevice(discoveryId);
                await refetch();
            });
        },
        setDeviceEnabled: async (surfaceId, isEnabled) => {
            await run(async () => {
                await api.setDeviceEnabled(surfaceId, isEnabled);
                await refetch();
            });
        },
        removeDevice: async (surfaceId) => {
            await run(async () => {
                await api.removeDevice(surfaceId);
                await refetch();
            });
        },
        assignPanel: async (surfaceId, panelId) => {
            await run(async () => {
                await api.assignActivePanel(surfaceId, panelId);
                await refetch();
            });
        },
        createPanel: async (input) => {
            let created: Panel | null = null;
            await run(async () => {
                const response = await api.createPanel(input);
                const data: unknown = await response.json();
                created = data as Panel;
                await refetch();
            });
            return created;
        },
        savePanel: async (panel) => {
            await run(async () => {
                await api.updatePanel(panel.panel_id, api.panelPayload(panel));
                await refetch();
            });
        },
        exportPanel: async (panel) => {
            try {
                const content = await api.fetchPanelConfiguration(panel.panel_id);
                const url = URL.createObjectURL(new Blob([content], { type: 'application/toml' }));
                const link = document.createElement('a');
                const slug = panel.name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/(^-|-$)/g, '');
                link.href = url;
                link.download = `${slug || 'panel'}.toml`;
                link.click();
                URL.revokeObjectURL(url);
            } catch (exportError) {
                setError(
                    exportError instanceof Error
                        ? exportError.message
                        : 'Unable to export panel configuration.',
                );
            }
        },
        saveConfiguration: async () => {
            await run(async () => {
                await api.saveConfiguration();
            });
        },
        clipboard,
        copyControl: (control) => {
            const template = toClipboard(control);
            setClipboard(template);
            void navigator.clipboard?.writeText(JSON.stringify(template)).catch(() => undefined);
        },
        clearClipboard: () => setClipboard(null),
        pressedKeysFor,
        pressedKeysForPanel,
    };

    return <InventoryContext.Provider value={store}>{props.children}</InventoryContext.Provider>;
};

export const useInventory = (): InventoryStore => {
    const store = useContext(InventoryContext);
    if (store === undefined) throw new Error('useInventory must be used within an InventoryProvider');
    return store;
};
