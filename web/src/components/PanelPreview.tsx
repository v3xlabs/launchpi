import { TbFillClipboard as TbCopy } from "solid-icons/tb";
import { Component, createMemo, For, JSX, Show } from "solid-js";

import { Control, DialPlacement, Panel, PanelDial, panelDial } from "../api/inventory";
import { DialIndicator } from "./DialIndicator";
import { KeyImage } from "./KeyImage";

type Cell = { column: number; row: number; keyIndex: number; };

const cellsOf = (panel: Panel): Cell[] =>
  Array.from({ length: panel.layout.columns * panel.layout.rows }, (_, index) => ({
    column: index % panel.layout.columns,
    row: Math.floor(index / panel.layout.columns),
    keyIndex: index,
  }));

const controlAt = (panel: Panel, cell: Cell): Control | undefined =>
  panel.controls.find(
    control => control.position.column === cell.column && control.position.row === cell.row,
  );

// Which state a press shows is resolution, and resolution lives in the daemon; the preview passes
// both states along with whether the key is down and lets it decide.

// A live level from the hardware wins over the level the panel declares.
const dialLevel = (dial: PanelDial, liveLevels?: Array<number | null>): number =>
  liveLevels?.[dial.index] ?? dial.level;

// The knobs are placed in key-grid coordinates, which can be negative or past the last column, so
// the drawn grid is the key grid grown to cover them and every cell is positioned explicitly.
type SurfaceGrid = { columns: number; rows: number; originColumn: number; originRow: number; };

const surfaceGrid = (panel: Panel, dials: DialPlacement[]): SurfaceGrid => {
  const columns = [0, panel.layout.columns - 1, ...dials.map(dial => dial.column)];
  const rows = [
    0,
    panel.layout.rows - 1,
    ...dials.flatMap(dial => [dial.row, dial.row + dial.row_span - 1]),
  ];
  const originColumn = Math.min(...columns);
  const originRow = Math.min(...rows);

  return {
    columns: Math.max(...columns) - originColumn + 1,
    rows: Math.max(...rows) - originRow + 1,
    originColumn,
    originRow,
  };
};

// A column with no keys in it holds a knob, which is wider than a key on every model that has one.
const gutterWidth = 1.5;

const gridStyle = (panel: Panel, grid: SurfaceGrid): JSX.CSSProperties => {
  const columns = Array.from({ length: grid.columns }, (_, index) => index + grid.originColumn);
  const isKeyColumn = (column: number) => column >= 0 && column < panel.layout.columns;

  return {
    "grid-template-columns": columns
      .map(column => (isKeyColumn(column) ? "minmax(0, 1fr)" : `${gutterWidth}fr`))
      .join(" "),
    "--cells": String(
      columns.reduce((total, column) => total + (isKeyColumn(column) ? 1 : gutterWidth), 0),
    ),
    "--rows": String(grid.rows),
  };
};

const cellStyle = (grid: SurfaceGrid, column: number, row: number, rowSpan = 1): JSX.CSSProperties => ({
  "grid-column": String(column - grid.originColumn + 1),
  "grid-row": `${row - grid.originRow + 1} / span ${rowSpan}`,
});

export const PanelThumbnail: Component<{
  panel: Panel;
  dials: DialPlacement[];
  pressedKeys?: Set<number>;
  dialLevels?: Array<number | null>;
  pressedDials?: Set<number>;
}> = (properties) => {
  const grid = createMemo(() => surfaceGrid(properties.panel, properties.dials));

  return (
    <div class="stage stage-compact">
      <div
        classList={{
          "key-grid": true,
          "key-grid-compact": true,
          "key-grid-dials": properties.dials.length > 0,
        }}
        style={gridStyle(properties.panel, grid())}
      >
        <For each={properties.dials}>
          {placement => (
            <div
              class="dial-cell"
              style={cellStyle(grid(), placement.column, placement.row, placement.row_span)}
            >
              <Show when={panelDial(properties.panel, placement.index)}>
                {dial => (
                  <DialIndicator
                    index={dial().index}
                    color={dial().color}
                    level={dialLevel(dial(), properties.dialLevels)}
                    isPressed={properties.pressedDials?.has(dial().index)}
                  />
                )}
              </Show>
            </div>
          )}
        </For>
        <For each={cellsOf(properties.panel)}>
          {(cell) => {
            const isPressed = () => properties.pressedKeys?.has(cell.keyIndex) ?? false;
            const keyed = () => controlAt(properties.panel, cell);

            return (
              <div
                classList={{ "key": true, "key-pressed": isPressed() }}
                style={cellStyle(grid(), cell.column, cell.row)}
              >
                <Show when={keyed()}>
                  {control => <KeyImage control={control()} isPressed={isPressed()} />}
                </Show>
              </div>
            );
          }}
        </For>
      </div>
    </div>
  );
};

type PanelStageProperties = {
  panel: Panel;
  dials: DialPlacement[];
  pressedKeys?: Set<number>;
  activeControlId?: string | null;
  activeDialIndex?: number | null;
  dialLevels?: Array<number | null>;
  pressedDials?: Set<number>;
  pasteMode?: boolean;
  onCellClick: (control: Control | undefined, column: number, row: number) => void;
  onCellFocus: (control: Control | undefined, column: number, row: number) => void;
  onDialClick: (index: number) => void;
};

const StageDial: Component<{
  dial: PanelDial;
  activeIndex?: number | null;
  dialLevels?: Array<number | null>;
  isPressed?: boolean;
  onClick: () => void;
}> = properties => (
  <button
    type="button"
    class="dial-button"
    data-selected={properties.activeIndex === properties.dial.index}
    onClick={properties.onClick}
    aria-label={`Edit dial ${properties.dial.index + 1}`}
  >
    <DialIndicator
      index={properties.dial.index}
      color={properties.dial.color}
      level={dialLevel(properties.dial, properties.dialLevels)}
      isPressed={properties.isPressed}
    />
  </button>
);

export const PanelStage: Component<PanelStageProperties> = (properties) => {
  const grid = createMemo(() => surfaceGrid(properties.panel, properties.dials));

  return (
    <div class="stage">
      <div
        classList={{ "key-grid": true, "key-grid-dials": properties.dials.length > 0 }}
        style={gridStyle(properties.panel, grid())}
      >
        <For each={properties.dials}>
          {placement => (
            <div
              class="dial-cell"
              style={cellStyle(grid(), placement.column, placement.row, placement.row_span)}
            >
              <Show when={panelDial(properties.panel, placement.index)}>
                {dial => (
                  <StageDial
                    dial={dial()}
                    activeIndex={properties.activeDialIndex}
                    dialLevels={properties.dialLevels}
                    isPressed={properties.pressedDials?.has(dial().index)}
                    onClick={() => properties.onDialClick(dial().index)}
                  />
                )}
              </Show>
            </div>
          )}
        </For>
        <For each={cellsOf(properties.panel)}>
          {(cell) => {
            const control = () => controlAt(properties.panel, cell);
            const isPressed = () => properties.pressedKeys?.has(cell.keyIndex) ?? false;

            return (
              <button
                type="button"
                classList={{
                  "key": true,
                  "key-button": true,
                  "key-empty": control() === undefined,
                  "key-paste": control() === undefined && Boolean(properties.pasteMode),
                  "key-selected": control()?.control_id === properties.activeControlId,
                  "key-pressed": isPressed(),
                }}
                style={cellStyle(grid(), cell.column, cell.row)}
                onClick={() => properties.onCellClick(control(), cell.column, cell.row)}
                onFocus={() => properties.onCellFocus(control(), cell.column, cell.row)}
                aria-label={
                  control() === undefined
                    ? `${properties.pasteMode ? "Paste into" : "Add control at"} row ${
                      cell.row + 1
                    }, column ${cell.column + 1}`
                    : `Edit ${control()?.name}`
                }
              >
                <Show
                  when={control()}
                  fallback={properties.pasteMode
                    ? <TbCopy class="h-3 w-3" />
                    : <span class="text-xs">+</span>}
                >
                  {keyed => <KeyImage control={keyed()} isPressed={isPressed()} />}
                </Show>
              </button>
            );
          }}
        </For>
      </div>
    </div>
  );
};
