import { Component, For, JSX, Show } from 'solid-js';

import { Control, Panel, panelDial, panelDialCount, RenderedState } from '../api/inventory';
import { DialIndicator } from './DialIndicator';
import { KeyImage } from './KeyImage';

type Cell = { column: number; row: number; keyIndex: number };

const cellsOf = (panel: Panel): Cell[] =>
    Array.from({ length: panel.layout.columns * panel.layout.rows }, (_, index) => ({
        column: index % panel.layout.columns,
        row: Math.floor(index / panel.layout.columns),
        keyIndex: index,
    }));

const controlAt = (panel: Panel, cell: Cell): Control | undefined =>
    panel.controls.find(
        (control) => control.position.column === cell.column && control.position.row === cell.row,
    );

const stateOf = (control: Control | undefined, isPressed: boolean): RenderedState | undefined => {
    if (control === undefined) return undefined;
    return isPressed ? control.pressed_state ?? control.default_state : control.default_state;
};

const hasDials = (panel: Panel): boolean => panelDialCount(panel) > 0;

// A live level from the hardware wins over the level the panel configures.
const dialLevel = (panel: Panel, index: number, liveLevels?: Array<number | null>): number =>
    liveLevels?.[index] ?? panelDial(panel, index).level;

const gridStyle = (panel: Panel): JSX.CSSProperties => ({
    '--columns': String(panel.layout.columns),
    '--rows': String(panel.layout.rows),
});

const DialCell: Component<{ side: 'left' | 'right'; children: JSX.Element }> = (props) => (
    <div classList={{ 'dial-cell': true, 'dial-cell-left': props.side === 'left', 'dial-cell-right': props.side === 'right' }}>
        {props.children}
    </div>
);

export const PanelThumbnail: Component<{
    panel: Panel;
    pressedKeys?: Set<number>;
    dialLevels?: Array<number | null>;
    pressedDials?: Set<number>;
}> = (props) => (
    <div class="stage stage-compact">
        <div
            classList={{
                'key-grid': true,
                'key-grid-compact': true,
                'key-grid-dials': hasDials(props.panel),
            }}
            style={gridStyle(props.panel)}
        >
            <Show when={hasDials(props.panel)}>
                <DialCell side="left">
                    <DialIndicator
                        index={0}
                        color={panelDial(props.panel, 0).color}
                        level={dialLevel(props.panel, 0, props.dialLevels)}
                        isPressed={props.pressedDials?.has(0)}
                    />
                </DialCell>
                <DialCell side="right">
                    <DialIndicator
                        index={1}
                        color={panelDial(props.panel, 1).color}
                        level={dialLevel(props.panel, 1, props.dialLevels)}
                        isPressed={props.pressedDials?.has(1)}
                    />
                </DialCell>
            </Show>
            <For each={cellsOf(props.panel)}>
                {(cell) => {
                    const isPressed = () => props.pressedKeys?.has(cell.keyIndex) ?? false;
                    const state = () => stateOf(controlAt(props.panel, cell), isPressed());
                    return (
                        <div classList={{ key: true, 'key-pressed': isPressed() }}>
                            <Show when={state()}>{(active) => <KeyImage state={active()} />}</Show>
                        </div>
                    );
                }}
            </For>
        </div>
    </div>
);

type PanelStageProps = {
    panel: Panel;
    pressedKeys?: Set<number>;
    activeControlId?: string | null;
    activeDialIndex?: number | null;
    dialLevels?: Array<number | null>;
    pressedDials?: Set<number>;
    pasteMode?: boolean;
    onCellClick: (control: Control | undefined, column: number, row: number) => void;
    onDialClick: (index: number) => void;
};

const StageDial: Component<{
    index: number;
    panel: Panel;
    activeIndex?: number | null;
    dialLevels?: Array<number | null>;
    isPressed?: boolean;
    onClick: () => void;
}> = (props) => (
    <button
        type="button"
        class="dial-button"
        data-selected={props.activeIndex === props.index}
        onClick={props.onClick}
        aria-label={`Edit dial ${props.index + 1}`}
    >
        <DialIndicator
            index={props.index}
            color={panelDial(props.panel, props.index).color}
            level={dialLevel(props.panel, props.index, props.dialLevels)}
            isPressed={props.isPressed}
        />
    </button>
);

export const PanelStage: Component<PanelStageProps> = (props) => (
    <div class="stage">
        <div
            classList={{ 'key-grid': true, 'key-grid-dials': hasDials(props.panel) }}
            style={gridStyle(props.panel)}
        >
            <Show when={hasDials(props.panel)}>
                <DialCell side="left">
                    <StageDial
                        index={0}
                        panel={props.panel}
                        activeIndex={props.activeDialIndex}
                        dialLevels={props.dialLevels}
                        isPressed={props.pressedDials?.has(0)}
                        onClick={() => props.onDialClick(0)}
                    />
                </DialCell>
                <DialCell side="right">
                    <StageDial
                        index={1}
                        panel={props.panel}
                        activeIndex={props.activeDialIndex}
                        dialLevels={props.dialLevels}
                        isPressed={props.pressedDials?.has(1)}
                        onClick={() => props.onDialClick(1)}
                    />
                </DialCell>
            </Show>
            <For each={cellsOf(props.panel)}>
                {(cell) => {
                    const control = () => controlAt(props.panel, cell);
                    const isPressed = () => props.pressedKeys?.has(cell.keyIndex) ?? false;
                    const state = () => stateOf(control(), isPressed());
                    return (
                        <button
                            type="button"
                            classList={{
                                key: true,
                                'key-button': true,
                                'key-empty': control() === undefined,
                                'key-paste': control() === undefined && Boolean(props.pasteMode),
                                'key-selected': control()?.control_id === props.activeControlId,
                                'key-pressed': isPressed(),
                            }}
                            onClick={() => props.onCellClick(control(), cell.column, cell.row)}
                            aria-label={
                                control() === undefined
                                    ? `${props.pasteMode ? 'Paste into' : 'Add control at'} row ${
                                          cell.row + 1
                                      }, column ${cell.column + 1}`
                                    : `Edit ${control()?.name}`
                            }
                        >
                            <Show
                                when={state()}
                                fallback={<span class="text-xs">{props.pasteMode ? '⎘' : '+'}</span>}
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
