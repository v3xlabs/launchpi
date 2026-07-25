import { Component, For, Match, Show, Switch } from 'solid-js';
import { TbFillClipboard as TbCopy, TbFillTrash as TbTrash } from 'solid-icons/tb';

import { capabilityLabels, Control, Panel, panelDial, RgbaColor } from '../api/inventory';
import { fromHex, newState, toHex } from '../utils/rendered';
import { DialIndicator, litRingSegments, totalRingSegments } from './DialIndicator';

export type PanelSelection = { kind: 'control'; controlId: string } | { kind: 'dial'; index: number };

const TextField: Component<{
    label: string;
    value: string;
    placeholder?: string;
    onChange: (value: string) => void;
}> = (props) => (
    <label class="field-label">
        {props.label}
        <input
            class="field-input"
            value={props.value}
            placeholder={props.placeholder}
            onInput={(event) => props.onChange(event.currentTarget.value)}
        />
    </label>
);

const ColorField: Component<{
    label: string;
    value: RgbaColor | null;
    fallback: string;
    onChange: (color: RgbaColor) => void;
}> = (props) => (
    <label class="field-label">
        {props.label}
        <input
            class="color-input"
            type="color"
            value={toHex(props.value, props.fallback)}
            onInput={(event) => props.onChange(fromHex(event.currentTarget.value))}
        />
    </label>
);

const PanelSettings: Component<{ panel: Panel; onMutate: (mutate: (panel: Panel) => void) => void }> = (props) => (
    <>
        <div class="card-head">
            <p class="card-title">Panel</p>
        </div>
        <div class="card-body">
            <TextField
                label="Name"
                value={props.panel.name}
                onChange={(value) =>
                    props.onMutate((panel) => {
                        panel.name = value;
                    })
                }
            />
            <fieldset class="grid gap-1">
                <legend class="field-label">Required capabilities</legend>
                <div class="mt-1 grid grid-cols-2 gap-1.5">
                    <For each={capabilityLabels}>
                        {({ key, label }) => (
                            <label class="check-tile">
                                <input
                                    type="checkbox"
                                    checked={props.panel.capabilities[key]}
                                    onInput={(event) =>
                                        props.onMutate((panel) => {
                                            panel.capabilities[key] = event.currentTarget.checked;
                                        })
                                    }
                                />
                                {label}
                            </label>
                        )}
                    </For>
                </div>
            </fieldset>
        </div>
    </>
);

const ControlEditor: Component<{
    control: Control;
    onMutate: (mutate: (control: Control) => void) => void;
    onCopy: () => void;
    onRemove: () => void;
}> = (props) => (
    <>
        <div class="card-head">
            <p class="card-title">
                Key {props.control.position.row + 1}:{props.control.position.column + 1}
            </p>
            <div class="flex gap-1.5">
                <button
                    type="button"
                    class="icon-button"
                    onClick={props.onCopy}
                    aria-label="Copy control"
                    title="Copy (Ctrl/Cmd+C)"
                >
                    <TbCopy class="h-3.5 w-3.5" />
                </button>
                <button
                    type="button"
                    class="danger-button"
                    onClick={props.onRemove}
                    aria-label="Remove control"
                    title="Remove control"
                >
                    <TbTrash class="h-3.5 w-3.5" />
                </button>
            </div>
        </div>
        <div class="card-body">
            <TextField
                label="Name"
                value={props.control.name}
                onChange={(value) =>
                    props.onMutate((control) => {
                        control.name = value;
                    })
                }
            />
            <TextField
                label="Label"
                value={props.control.default_state.text ?? ''}
                placeholder="Shown on the key"
                onChange={(value) =>
                    props.onMutate((control) => {
                        control.default_state.text = value || null;
                    })
                }
            />
            <div class="grid grid-cols-2 gap-2">
                <ColorField
                    label="Text"
                    value={props.control.default_state.foreground_color}
                    fallback="#ffffff"
                    onChange={(color) =>
                        props.onMutate((control) => {
                            control.default_state.foreground_color = color;
                        })
                    }
                />
                <ColorField
                    label="Fill"
                    value={props.control.default_state.background_color}
                    fallback="#1e293b"
                    onChange={(color) =>
                        props.onMutate((control) => {
                            control.default_state.background_color = color;
                        })
                    }
                />
            </div>

            <label class="check-tile">
                <input
                    type="checkbox"
                    checked={props.control.pressed_state !== null}
                    onInput={(event) =>
                        props.onMutate((control) => {
                            control.pressed_state = event.currentTarget.checked ? newState(true) : null;
                        })
                    }
                />
                Pressed feedback
            </label>

            <Show when={props.control.pressed_state}>
                {(pressed) => (
                    <div class="pressed-fields">
                        <TextField
                            label="Pressed label"
                            value={pressed().text ?? ''}
                            placeholder="Optional"
                            onChange={(value) =>
                                props.onMutate((control) => {
                                    if (control.pressed_state) control.pressed_state.text = value || null;
                                })
                            }
                        />
                        <div class="grid grid-cols-2 gap-2">
                            <ColorField
                                label="Text"
                                value={pressed().foreground_color}
                                fallback="#ffffff"
                                onChange={(color) =>
                                    props.onMutate((control) => {
                                        if (control.pressed_state) control.pressed_state.foreground_color = color;
                                    })
                                }
                            />
                            <ColorField
                                label="Fill"
                                value={pressed().background_color}
                                fallback="#0f172a"
                                onChange={(color) =>
                                    props.onMutate((control) => {
                                        if (control.pressed_state) control.pressed_state.background_color = color;
                                    })
                                }
                            />
                        </div>
                    </div>
                )}
            </Show>
        </div>
    </>
);

const DialEditor: Component<{
    panel: Panel;
    index: number;
    onColorChange: (index: number, color: RgbaColor) => void;
    onLevelChange: (index: number, level: number) => void;
}> = (props) => {
    const dial = () => panelDial(props.panel, props.index);
    return (
        <>
            <div class="card-head">
                <p class="card-title">Dial {props.index + 1}</p>
                <span class="chip chip-muted">{props.index === 0 ? 'left' : 'right'}</span>
            </div>
            <div class="card-body">
                <div class="flex items-center gap-4 bg-neutral-950 p-3">
                    <div class="w-14 shrink-0">
                        <DialIndicator index={props.index} color={dial().color} level={dial().level} />
                    </div>
                    <p class="mono">
                        {toHex(dial().color, 'unset')} · {dial().level}% ·{' '}
                        {litRingSegments(dial().level)}/{totalRingSegments} segments
                    </p>
                </div>
                <ColorField
                    label="Colour"
                    value={dial().color}
                    fallback="#1e293b"
                    onChange={(color) => props.onColorChange(props.index, color)}
                />
                <label class="field-label">
                    Ring level · {dial().level}%
                    <input
                        class="range-input"
                        type="range"
                        min="0"
                        max="100"
                        value={dial().level}
                        onInput={(event) => props.onLevelChange(props.index, Number(event.currentTarget.value))}
                    />
                </label>
            </div>
        </>
    );
};

type PanelInspectorProps = {
    panel: Panel;
    selection: PanelSelection | null;
    control: Control | null;
    onPanelMutate: (mutate: (panel: Panel) => void) => void;
    onControlMutate: (mutate: (control: Control) => void) => void;
    onCopyControl: () => void;
    onRemoveControl: () => void;
    onDialColorChange: (index: number, color: RgbaColor) => void;
    onDialLevelChange: (index: number, level: number) => void;
};

export const PanelInspector: Component<PanelInspectorProps> = (props) => {
    // Guard on the selection object, not the index — dial 0 is a falsy value.
    const dialSelection = () => (props.selection?.kind === 'dial' ? props.selection : null);

    return (
        <div class="card">
            <Switch fallback={<PanelSettings panel={props.panel} onMutate={props.onPanelMutate} />}>
                <Match when={dialSelection()}>
                    {(selection) => (
                        <DialEditor
                            panel={props.panel}
                            index={selection().index}
                            onColorChange={props.onDialColorChange}
                            onLevelChange={props.onDialLevelChange}
                        />
                    )}
                </Match>
                <Match when={props.control}>
                    {(control) => (
                        <ControlEditor
                            control={control()}
                            onMutate={props.onControlMutate}
                            onCopy={props.onCopyControl}
                            onRemove={props.onRemoveControl}
                        />
                    )}
                </Match>
            </Switch>
        </div>
    );
};
