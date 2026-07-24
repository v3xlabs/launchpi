import { Component, createMemo, createResource, For, Show } from 'solid-js';

import { Control, Panel, RenderedState } from '../api/inventory';
import { renderedKeyImageUrl } from '../api/render';
import { toHex } from '../utils/rendered';

type GridPreviewProps = {
    panel: Panel;
    activeControlId?: string | null;
    pasteMode?: boolean;
    pressedKeys?: Set<number>;
    onCellClick: (control: Control | undefined, column: number, row: number) => void;
};

export const KeyImage: Component<{ state: RenderedState }> = (props) => {
    const renderKey = () =>
        JSON.stringify([props.state.text, props.state.foreground_color, props.state.background_color]);
    const [url] = createResource(renderKey, () => renderedKeyImageUrl(props.state));
    return (
        <Show when={url()}>
            {(src) => (
                <img src={src()} alt="" class="pointer-events-none absolute inset-0 h-full w-full object-cover" />
            )}
        </Show>
    );
};

export const GridPreview: Component<GridPreviewProps> = (props) => {
    const positions = createMemo(() =>
        Array.from({ length: props.panel.layout.columns * props.panel.layout.rows }, (_, index) => ({
            column: index % props.panel.layout.columns,
            row: Math.floor(index / props.panel.layout.columns),
        })),
    );
    const controlAt = (column: number, row: number): Control | undefined =>
        props.panel.controls.find(
            (control) => control.position.column === column && control.position.row === row,
        );
    const keyIndexAt = (column: number, row: number): number => row * props.panel.layout.columns + column;
    const isPressed = (column: number, row: number): boolean =>
        props.pressedKeys?.has(keyIndexAt(column, row)) ?? false;

    return (
        <div class="grid-frame">
            <div class="control-grid" style={{ '--columns': String(props.panel.layout.columns) }}>
                <For each={positions()}>
                    {(position) => {
                        const control = () => controlAt(position.column, position.row);
                        const pressed = () => control() !== undefined && isPressed(position.column, position.row);
                        const state = () =>
                            pressed()
                                ? control()?.pressed_state ?? control()?.default_state
                                : control()?.default_state;
                        return (
                            <button
                                type="button"
                                classList={{
                                    'grid-control': true,
                                    'grid-control-selected': control()?.control_id === props.activeControlId,
                                    'grid-control-empty': control() === undefined,
                                    'grid-control-paste': control() === undefined && Boolean(props.pasteMode),
                                }}
                                style={{
                                    'background-color': state()?.background_color
                                        ? toHex(state()?.background_color ?? null, '#000000')
                                        : '#000000',
                                }}
                                onClick={() => props.onCellClick(control(), position.column, position.row)}
                                aria-label={
                                    control() === undefined
                                        ? `${props.pasteMode ? 'Paste into' : 'Add control at'} row ${
                                              position.row + 1
                                          }, column ${position.column + 1}`
                                        : `Edit ${control()?.name}`
                                }
                            >
                                <Show
                                    when={state()}
                                    fallback={
                                        <span class="text-neutral-600">{props.pasteMode ? '⎘' : '+'}</span>
                                    }
                                >
                                    {(active) => <KeyImage state={active()} />}
                                </Show>
                            </button>
                        );
                    }}
                </For>
            </div>
        </div>
    );
};
