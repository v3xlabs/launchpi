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
} from "solid-js";
import { createStore, produce, reconcile } from "solid-js/store";

import {
  asDeviceStatusEvent,
  asDialPressEvent,
  asDialStateEvent,
  asEventFrame,
  asKeyStateEvent,
  asVariableChangedEvent,
} from "../api/events";
import * as api from "../api/inventory";
import {
  Control,
  Device,
  DialPress,
  DialState,
  Inventory,
  isLogEntry,
  KeyEvent,
  LogEntry,
  Panel,
  studioDialCount,
} from "../api/inventory";
import { createPluginStore, PluginStore } from "./pluginStore";

/** Mirrors the daemon's per-surface ring buffer, so the tail stays bounded on a long session. */
const logCapacity = 400;

const groupPressedKeys = (keyStates: KeyEvent[]): Record<string, number[]> => {
  const grouped: Record<string, number[]> = {};

  for (const keyState of keyStates) {
    if (!keyState.is_pressed) continue;

    (grouped[keyState.surface_id] ??= []).push(keyState.key_index);
  }

  return grouped;
};

const groupLogs = (logs: LogEntry[]): Record<string, LogEntry[]> => {
  const grouped: Record<string, LogEntry[]> = {};

  for (const entry of logs) (grouped[entry.surface_id] ??= []).push(entry);

  return grouped;
};

const groupPressedDials = (dialPresses: DialPress[]): Record<string, number[]> => {
  const grouped: Record<string, number[]> = {};

  for (const dial of dialPresses) {
    if (!dial.is_pressed) continue;

    (grouped[dial.surface_id] ??= []).push(dial.dial_index);
  }

  return grouped;
};

// Live dial levels keyed by surface, then by dial index. A missing entry means the dial still sits
// wherever its panel configured it.
type DialLevels = Record<string, Record<string, number>>;
const groupDialLevels = (dialStates: DialState[]): DialLevels => {
  const grouped: DialLevels = {};

  for (const dial of dialStates) {
    (grouped[dial.surface_id] ??= {})[String(dial.dial_index)] = dial.level;
  }

  return grouped;
};

export type ControlClipboard = Pick<
  Control,
    "name" | "default_state" | "pressed_state" | "action_bindings" | "feedback_bindings"
>;

export type InventoryStore = {
  inventory: Accessor<Inventory>;
  isLoading: Accessor<boolean>;
  isSaving: Accessor<boolean>;
  isConnected: Accessor<boolean>;
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
  deletePanel: (panelId: string) => Promise<boolean>;
  exportPanel: (panel: Panel) => Promise<void>;
  saveConfig: () => Promise<void>;
  clipboard: Accessor<ControlClipboard | null>;
  copyControl: (control: Control) => void;
  clearClipboard: () => void;
  pressedKeysFor: (surfaceId: string) => number[];
  pressedKeysForPanel: (panelId: string) => Set<number>;
  /** Live dial levels per dial index, `null` where the dial has not moved since the panel loaded. */
  dialLevelsFor: (surfaceId: string) => Array<number | null>;
  dialLevelsForPanel: (panelId: string) => Array<number | null>;
  pressedDialsFor: (surfaceId: string) => Set<number>;
  pressedDialsForPanel: (panelId: string) => Set<number>;
  /** A device's activity log, oldest first. */
  logsFor: (surfaceId: string) => LogEntry[];
} & PluginStore;

const InventoryContext = createContext<InventoryStore>();

const toClipboard = (control: Control): ControlClipboard => ({
  name: control.name,
  default_state: control.default_state,
  pressed_state: control.pressed_state,
  action_bindings: control.action_bindings,
  feedback_bindings: control.feedback_bindings,
});

export const InventoryProvider: ParentComponent = (properties) => {
  const [resource, { refetch }] = createResource<Inventory>(api.fetchInventory);
  const [snapshot, setSnapshot] = createStore<Inventory>(api.emptyInventory);
  const [pressedBySurface, setPressedBySurface] = createStore<Record<string, number[]>>({});
  const [dialLevels, setDialLevels] = createStore<DialLevels>({});
  const [pressedDialsBySurface, setPressedDialsBySurface] = createStore<Record<string, number[]>>({});
  const [logsBySurface, setLogsBySurface] = createStore<Record<string, LogEntry[]>>({});
  const [isSaving, setIsSaving] = createSignal(false);
  const [isConnected, setIsConnected] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [clipboard, setClipboard] = createSignal<ControlClipboard | null>(null);

  // Reconcile fetched data into stores so unchanged rows keep their identity (no re-mount / flicker).
  // Live key presses arrive over the WebSocket below; a fetch only resyncs the authoritative baseline.
  createEffect(() => {
    const data = resource();

    if (data === undefined) return;

    setSnapshot(reconcile(data));
    setPressedBySurface(reconcile(groupPressedKeys(data.key_states)));
    setDialLevels(reconcile(groupDialLevels(data.dial_states)));
    setPressedDialsBySurface(reconcile(groupPressedDials(data.dial_presses)));
    setLogsBySurface(reconcile(groupLogs(data.logs), { key: "sequence" }));
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
  const dialLevelsFor = (surfaceId: string): Array<number | null> =>
    Array.from({ length: studioDialCount }, (_, index) => dialLevels[surfaceId]?.[String(index)] ?? null);
  const pressedDialsFor = (surfaceId: string): Set<number> =>
    new Set(pressedDialsBySurface[surfaceId]);
  const pressedDialsForPanel = (panelId: string): Set<number> => {
    const pressed = new Set<number>();

    for (const device of snapshot.devices) {
      if (device.active_panel_id !== panelId) continue;

      for (const dialIndex of pressedDialsBySurface[device.surface_id] ?? []) pressed.add(dialIndex);
    }

    return pressed;
  };
  const logsFor = (surfaceId: string): LogEntry[] => logsBySurface[surfaceId] ?? [];
  const appendLog = (entry: LogEntry) => {
    setLogsBySurface(
      produce((state) => {
        const entries = (state[entry.surface_id] ??= []);

        if (entries.some(existing => existing.sequence === entry.sequence)) return;

        entries.push(entry);

        if (entries.length > logCapacity) entries.splice(0, entries.length - logCapacity);
      }),
    );
  };
  const dialLevelsForPanel = (panelId: string): Array<number | null> => {
    const devices = snapshot.devices.filter(device => device.active_panel_id === panelId);

    return Array.from({ length: studioDialCount }, (_, index) => {
      for (const device of devices) {
        const level = dialLevels[device.surface_id]?.[String(index)];

        if (level !== undefined) return level;
      }

      return null;
    });
  };

  const refresh = async () => {
    try {
      await refetch();
      setError(null);
    }
    catch (refreshError) {
      setError(refreshError instanceof Error ? refreshError.message : "Unable to reach the daemon.");
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

  const setDialPressed = (surfaceId: string, dialIndex: number, isPressed: boolean) => {
    setPressedDialsBySurface(
      produce((state) => {
        const list = (state[surfaceId] ??= []);
        const index = list.indexOf(dialIndex);

        if (isPressed && index === -1) list.push(dialIndex);
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
      const parsed = asEventFrame(raw);

      if (parsed === null) return;

      const keyState = asKeyStateEvent(parsed);

      if (keyState !== null) {
        setKeyPressed(keyState.surface_id, keyState.key_index, keyState.is_pressed);

        return;
      }

      const dialPress = asDialPressEvent(parsed);

      if (dialPress !== null) {
        setDialPressed(dialPress.surface_id, dialPress.dial_index, dialPress.is_pressed);

        return;
      }

      if (parsed.type === "log" && isLogEntry(parsed)) {
        appendLog(parsed);

        return;
      }

      const dialState = asDialStateEvent(parsed);

      if (dialState !== null) {
        setDialLevels(
          produce((state) => {
            (state[dialState.surface_id] ??= {})[String(dialState.dial_index)]
              = dialState.level;
          }),
        );

        return;
      }

      const variable = asVariableChangedEvent(parsed);

      if (variable !== null) {
        pluginStore.setVariable(variable.integration_id, variable.name, variable.rendered);

        return;
      }

      const deviceStatus = asDeviceStatusEvent(parsed);

      if (deviceStatus !== null) {
        // Patched in place. An unreachable surface flips status on every reconnect attempt, and
        // refetching the whole inventory on that cadence made the entire UI churn.
        setSnapshot(
          "devices",
          device => device.surface_id === deviceStatus.surface_id,
          produce((device) => {
            device.status = deviceStatus.status as Device["status"];
            device.last_error = deviceStatus.last_error;
          }),
        );

        return;
      }

      if (parsed.type === "changed") scheduleResync();
    };

    const connect = () => {
      const protocol = globalThis.location.protocol === "https:" ? "wss" : "ws";

      socket = new WebSocket(`${protocol}://${globalThis.location.host}/api/events`);
      socket.addEventListener("open", () => {
        setIsConnected(true);
        void refresh();
      });
      socket.addEventListener("message", (event) => {
        if (typeof event.data === "string") handleMessage(event.data);
      });
      socket.addEventListener("close", () => {
        setIsConnected(false);

        if (!isClosed) reconnectTimer = setTimeout(connect, 1000);
      });
      socket.addEventListener("error", () => socket?.close());
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
    }
    catch (operationError) {
      setError(
        operationError instanceof Error ? operationError.message : "Unable to complete the request.",
      );

      return false;
    }
    finally {
      setIsSaving(false);
    }
  };

  const pluginStore = createPluginStore(run, setError);

  const store: InventoryStore = {
    inventory,
    isLoading,
    isSaving,
    isConnected,
    error,
    setError,
    refresh,
    addDevice: input =>
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
    deletePanel: panelId =>
      run(async () => {
        await api.deletePanel(panelId);
        await refetch();
      }),
    exportPanel: async (panel) => {
      try {
        const content = await api.fetchPanelConfig(panel.panel_id);
        const url = URL.createObjectURL(new Blob([content], { type: "application/toml" }));
        const link = document.createElement("a");
        const slug = panel.name.toLowerCase().replaceAll(/[^a-z0-9]+/g, "-")
          .replaceAll(/(^-|-$)/g, "");

        link.href = url;
        link.download = `${slug || "panel"}.toml`;
        link.click();
        URL.revokeObjectURL(url);
      }
      catch (exportError) {
        setError(
          exportError instanceof Error
            ? exportError.message
            : "Unable to export panel configuration.",
        );
      }
    },
    saveConfig: async () => {
      await run(async () => {
        await api.saveConfig();
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
    dialLevelsFor,
    dialLevelsForPanel,
    pressedDialsFor,
    pressedDialsForPanel,
    logsFor,
    ...pluginStore,
  };

  return <InventoryContext.Provider value={store}>{properties.children}</InventoryContext.Provider>;
};

export const useInventory = (): InventoryStore => {
  const store = useContext(InventoryContext);

  if (store === undefined) throw new Error("useInventory must be used within an InventoryProvider");

  return store;
};
