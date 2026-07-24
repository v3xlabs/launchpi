import { Link } from '@tanstack/solid-router';
import { Component, createEffect, createMemo, createSignal, For, onCleanup, onMount, Show } from 'solid-js';
import { createStore, produce } from 'solid-js/store';
import {
    TbFillCircleCheck as TbCheck,
    TbFillClipboard as TbCopy,
    TbFillFileDownload as TbDownload,
    TbFillTrash as TbTrash,
} from 'solid-icons/tb';

import { capabilityLabels, Control, Panel } from '../api/inventory';
import { GridPreview, KeyImage } from '../components/GridPreview';
import { ControlClipboard, useInventory } from '../context/InventoryContext';
import { CreatePanelDialog } from '../dialogs/CreatePanelDialog';
import { fromHex, newState, toHex } from '../utils/rendered';

const cloneState = <T,>(value: T): T => JSON.parse(JSON.stringify(value)) as T;

export const PanelsPage: Component<{ panelId?: string }> = (props) => {
    const store = useInventory();
    const [draft, setDraft] = createStore<{ panel: Panel | null; dirty: boolean }>({
        panel: null,
        dirty: false,
    });
    const [selectedControlId, setSelectedControlId] = createSignal<string | null>(null);

    const serverPanel = createMemo(
        () => store.inventory().panels.find((panel) => panel.panel_id === props.panelId) ?? null,
    );

    const pressedKeys = createMemo(() => store.pressedKeysForPanel(props.panelId ?? ''));

    createEffect(() => {
        const id = props.panelId;
        const server = serverPanel();
        if (draft.panel?.panel_id !== id) {
            setDraft({ panel: server ? cloneState(server) : null, dirty: false });
            setSelectedControlId(null);
            return;
        }
        if (draft.panel === null && server) setDraft('panel', cloneState(server));
    });

    const selectedControl = createMemo(
        () => draft.panel?.controls.find((control) => control.control_id === selectedControlId()) ?? null,
    );

    const mutatePanel = (mutate: (panel: Panel) => void) => {
        setDraft(
            'panel',
            produce((panel) => {
                if (panel) mutate(panel);
            }),
        );
        setDraft('dirty', true);
    };

    const mutateSelected = (mutate: (control: Control) => void) =>
        mutatePanel((panel) => {
            const control = panel.controls.find((entry) => entry.control_id === selectedControlId());
            if (control) mutate(control);
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
            feedback_bindings: cloneState(template?.feedback_bindings ?? []),
        };
        mutatePanel((entry) => entry.controls.push(control));
        setSelectedControlId(controlId);
    };

    const removeControl = () => {
        const controlId = selectedControlId();
        if (controlId === null) return;
        mutatePanel((panel) => {
            panel.controls = panel.controls.filter((control) => control.control_id !== controlId);
        });
        setSelectedControlId(null);
    };

    const firstFreeCell = (): { column: number; row: number } | null => {
        const panel = draft.panel;
        if (panel === null) return null;
        for (let row = 0; row < panel.layout.rows; row += 1) {
            for (let column = 0; column < panel.layout.columns; column += 1) {
                const occupied = panel.controls.some(
                    (control) => control.position.column === column && control.position.row === row,
                );
                if (!occupied) return { column, row };
            }
        }
        return null;
    };

    const handleCellClick = (control: Control | undefined, column: number, row: number) => {
        if (control !== undefined) {
            setSelectedControlId(control.control_id);
            return;
        }
        const clip = store.clipboard();
        if (clip !== null) {
            placeControl(column, row, clip);
            return;
        }
        placeControl(column, row);
    };

    const savePanel = async () => {
        const panel = draft.panel;
        if (panel === null) return;
        await store.savePanel(panel);
        setDraft('dirty', false);
    };

    const onKeyDown = (event: KeyboardEvent) => {
        const target = event.target as HTMLElement | null;
        if (event.key === 'Escape') {
            store.clearClipboard();
            setSelectedControlId(null);
            return;
        }
        const isField = target !== null && ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName);
        if (isField || !(event.metaKey || event.ctrlKey)) return;
        const key = event.key.toLowerCase();
        if (key === 'c') {
            const control = selectedControl();
            if (control !== null) {
                store.copyControl(control);
                event.preventDefault();
            }
        }
        if (key === 'v') {
            const cell = firstFreeCell();
            const clip = store.clipboard();
            if (cell !== null && clip !== null) {
                placeControl(cell.column, cell.row, clip);
                event.preventDefault();
            }
        }
    };

    onMount(() => window.addEventListener('keydown', onKeyDown));
    onCleanup(() => window.removeEventListener('keydown', onKeyDown));

    return (
        <div class="page">
            <Show when={draft.panel} fallback={<PanelsOverview />}>
                {(panel) => (
                    <div class="space-y-6">
                        <div class="flex flex-wrap items-start justify-between gap-4">
                            <div>
                                <h1 class="text-2xl font-semibold tracking-tight">{panel().name}</h1>
                                <p class="mt-1 text-sm text-neutral-400">
                                    {panel().layout.columns} × {panel().layout.rows} grid · {panel().controls.length}{' '}
                                    controls
                                    <Show when={draft.dirty}>
                                        <span class="ml-2 text-amber-300">· unsaved changes</span>
                                    </Show>
                                </p>
                            </div>
                            <div class="flex gap-2">
                                <button
                                    type="button"
                                    class="secondary-button"
                                    onClick={() => void store.exportPanel(panel())}
                                >
                                    <TbDownload class="h-4 w-4" />
                                    Export TOML
                                </button>
                                <button
                                    type="button"
                                    class="primary-button"
                                    onClick={() => void savePanel()}
                                    disabled={store.isSaving() || !draft.dirty}
                                >
                                    <TbCheck class="h-4 w-4" />
                                    {store.isSaving() ? 'Saving…' : 'Save panel'}
                                </button>
                            </div>
                        </div>

                        <Show when={store.clipboard()}>
                            {(clip) => (
                                <div class="clipboard-banner">
                                    <TbCopy class="h-4 w-4 shrink-0" />
                                    <span class="min-w-0 flex-1 truncate">
                                        Copied <strong>{clip().name}</strong> — click an empty cell to paste, or
                                        press Esc to clear.
                                    </span>
                                    <button type="button" class="link-button" onClick={() => store.clearClipboard()}>
                                        Clear
                                    </button>
                                </div>
                            )}
                        </Show>

                        <div class="editor-body">
                            <div class="space-y-4">
                                <label class="field-label">
                                    Panel name
                                    <input
                                        class="field-input"
                                        value={panel().name}
                                        onInput={(event) =>
                                            mutatePanel((entry) => {
                                                entry.name = event.currentTarget.value;
                                            })
                                        }
                                    />
                                </label>
                                <GridPreview
                                    panel={panel()}
                                    activeControlId={selectedControlId()}
                                    pasteMode={store.clipboard() !== null}
                                    pressedKeys={pressedKeys()}
                                    onCellClick={handleCellClick}
                                />
                                <div class="grid grid-cols-2 gap-2 sm:grid-cols-3">
                                    <For each={capabilityLabels}>
                                        {({ key, label }) => (
                                            <label class="capability-toggle">
                                                <input
                                                    type="checkbox"
                                                    checked={panel().capabilities[key]}
                                                    onInput={(event) =>
                                                        mutatePanel((entry) => {
                                                            entry.capabilities[key] = event.currentTarget.checked;
                                                        })
                                                    }
                                                />
                                                {label}
                                            </label>
                                        )}
                                    </For>
                                </div>
                            </div>

                            <aside class="control-editor">
                                <Show
                                    when={selectedControl()}
                                    fallback={
                                        <p class="empty-state">
                                            Select a control to edit it, or click an empty cell to add one
                                            {store.clipboard() !== null ? ' or paste the copied control' : ''}.
                                        </p>
                                    }
                                >
                                    {(control) => (
                                        <div class="space-y-5">
                                            <div class="flex items-center justify-between gap-3">
                                                <div>
                                                    <p class="section-title">Control</p>
                                                    <p class="mt-1 text-sm text-neutral-400">
                                                        Position {control().position.row + 1}:
                                                        {control().position.column + 1}
                                                    </p>
                                                </div>
                                                <div class="flex gap-2">
                                                    <button
                                                        type="button"
                                                        class="icon-button"
                                                        onClick={() => store.copyControl(control())}
                                                        aria-label="Copy control"
                                                        title="Copy (Ctrl/Cmd+C)"
                                                    >
                                                        <TbCopy />
                                                    </button>
                                                    <button
                                                        type="button"
                                                        class="icon-button hover:border-rose-500/60 hover:text-rose-300"
                                                        onClick={removeControl}
                                                        aria-label="Remove control"
                                                    >
                                                        <TbTrash />
                                                    </button>
                                                </div>
                                            </div>

                                            <label class="field-label">
                                                Name
                                                <input
                                                    class="field-input"
                                                    value={control().name}
                                                    onInput={(event) =>
                                                        mutateSelected((entry) => {
                                                            entry.name = event.currentTarget.value;
                                                        })
                                                    }
                                                />
                                            </label>
                                            <label class="field-label">
                                                Default label
                                                <input
                                                    class="field-input"
                                                    value={control().default_state.text ?? ''}
                                                    placeholder="Label shown on device"
                                                    onInput={(event) =>
                                                        mutateSelected((entry) => {
                                                            entry.default_state.text =
                                                                event.currentTarget.value || null;
                                                        })
                                                    }
                                                />
                                            </label>
                                            <div class="grid grid-cols-2 gap-3">
                                                <label class="field-label">
                                                    Text color
                                                    <input
                                                        class="color-input"
                                                        type="color"
                                                        value={toHex(control().default_state.foreground_color, '#ffffff')}
                                                        onInput={(event) =>
                                                            mutateSelected((entry) => {
                                                                entry.default_state.foreground_color = fromHex(
                                                                    event.currentTarget.value,
                                                                );
                                                            })
                                                        }
                                                    />
                                                </label>
                                                <label class="field-label">
                                                    Fill color
                                                    <input
                                                        class="color-input"
                                                        type="color"
                                                        value={toHex(control().default_state.background_color, '#1e293b')}
                                                        onInput={(event) =>
                                                            mutateSelected((entry) => {
                                                                entry.default_state.background_color = fromHex(
                                                                    event.currentTarget.value,
                                                                );
                                                            })
                                                        }
                                                    />
                                                </label>
                                            </div>

                                            <label class="capability-toggle">
                                                <input
                                                    type="checkbox"
                                                    checked={control().pressed_state !== null}
                                                    onInput={(event) =>
                                                        mutateSelected((entry) => {
                                                            entry.pressed_state = event.currentTarget.checked
                                                                ? newState(true)
                                                                : null;
                                                        })
                                                    }
                                                />
                                                Pressed feedback
                                            </label>

                                            <Show when={control().pressed_state}>
                                                {(pressed) => (
                                                    <div class="pressed-fields">
                                                        <p class="section-title">Pressed state</p>
                                                        <label class="field-label">
                                                            Pressed label
                                                            <input
                                                                class="field-input"
                                                                value={pressed().text ?? ''}
                                                                placeholder="Optional alternate label"
                                                                onInput={(event) =>
                                                                    mutateSelected((entry) => {
                                                                        if (entry.pressed_state)
                                                                            entry.pressed_state.text =
                                                                                event.currentTarget.value || null;
                                                                    })
                                                                }
                                                            />
                                                        </label>
                                                        <div class="grid grid-cols-2 gap-3">
                                                            <label class="field-label">
                                                                Text color
                                                                <input
                                                                    class="color-input"
                                                                    type="color"
                                                                    value={toHex(pressed().foreground_color, '#ffffff')}
                                                                    onInput={(event) =>
                                                                        mutateSelected((entry) => {
                                                                            if (entry.pressed_state)
                                                                                entry.pressed_state.foreground_color =
                                                                                    fromHex(event.currentTarget.value);
                                                                        })
                                                                    }
                                                                />
                                                            </label>
                                                            <label class="field-label">
                                                                Fill color
                                                                <input
                                                                    class="color-input"
                                                                    type="color"
                                                                    value={toHex(pressed().background_color, '#0f172a')}
                                                                    onInput={(event) =>
                                                                        mutateSelected((entry) => {
                                                                            if (entry.pressed_state)
                                                                                entry.pressed_state.background_color =
                                                                                    fromHex(event.currentTarget.value);
                                                                        })
                                                                    }
                                                                />
                                                            </label>
                                                        </div>
                                                    </div>
                                                )}
                                            </Show>
                                        </div>
                                    )}
                                </Show>
                            </aside>
                        </div>
                    </div>
                )}
            </Show>
        </div>
    );
};

const PanelThumbnail: Component<{ panel: Panel }> = (props) => {
    const store = useInventory();
    const pressed = createMemo(() => store.pressedKeysForPanel(props.panel.panel_id));
    const cells = createMemo(() =>
        Array.from({ length: props.panel.layout.columns * props.panel.layout.rows }, (_, index) => {
            const column = index % props.panel.layout.columns;
            const row = Math.floor(index / props.panel.layout.columns);
            return (
                props.panel.controls.find(
                    (control) => control.position.column === column && control.position.row === row,
                ) ?? null
            );
        }),
    );
    return (
        <div
            class="grid gap-px bg-neutral-800"
            style={{ 'grid-template-columns': `repeat(${props.panel.layout.columns}, minmax(0, 1fr))` }}
        >
            <For each={cells()}>
                {(control, index) => {
                    const activeState = () =>
                        control === null
                            ? null
                            : pressed().has(index())
                              ? control.pressed_state ?? control.default_state
                              : control.default_state;
                    return (
                        <div class="relative aspect-square bg-black">
                            <Show when={activeState()}>{(state) => <KeyImage state={state()} />}</Show>
                        </div>
                    );
                }}
            </For>
        </div>
    );
};

const PanelCard: Component<{ panel: Panel }> = (props) => (
    <Link
        to="/panels/$panelId"
        params={{ panelId: props.panel.panel_id }}
        class="block border border-neutral-800 bg-neutral-900 p-3 no-underline transition hover:border-neutral-600"
    >
        <PanelThumbnail panel={props.panel} />
        <div class="mt-3">
            <p class="truncate text-sm font-semibold text-neutral-100">{props.panel.name}</p>
            <p class="text-xs text-neutral-500">
                {props.panel.layout.columns}×{props.panel.layout.rows} · {props.panel.controls.length} controls
            </p>
        </div>
    </Link>
);

const PanelsOverview: Component = () => {
    const store = useInventory();
    return (
        <div class="space-y-6">
            <div class="flex flex-wrap items-start justify-between gap-4">
                <div>
                    <p class="eyebrow">Panels</p>
                    <h1 class="mt-1 text-2xl font-semibold tracking-tight">Reusable control grids</h1>
                    <p class="mt-2 max-w-xl text-sm leading-6 text-neutral-400">
                        Build a panel once, then assign it wherever its grid and capabilities fit.
                    </p>
                </div>
                <CreatePanelDialog
                    trigger={
                        <button type="button" class="primary-button">
                            <span aria-hidden="true">+</span>
                            New panel
                        </button>
                    }
                />
            </div>
            <Show
                when={store.inventory().panels.length > 0}
                fallback={<p class="empty-state">No panels yet. Create one to begin.</p>}
            >
                <div class="grid gap-4 sm:grid-cols-2 xl:grid-cols-3">
                    <For each={store.inventory().panels}>{(panel) => <PanelCard panel={panel} />}</For>
                </div>
            </Show>
        </div>
    );
};
