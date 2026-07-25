import { TbFillClipboard as TbCopy } from "solid-icons/tb";
import { Component, For, JSX, Show } from "solid-js";

import { Control, Panel, panelDial, panelDialCount } from "../api/inventory";
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

const hasDials = (panel: Panel): boolean => panelDialCount(panel) > 0;

// A live level from the hardware wins over the level the panel configures.
const dialLevel = (panel: Panel, index: number, liveLevels?: Array<number | null>): number =>
  liveLevels?.[index] ?? panelDial(panel, index).level;

const gridStyle = (panel: Panel): JSX.CSSProperties => ({
  "--columns": String(panel.layout.columns),
  "--rows": String(panel.layout.rows),
});

const DialCell: Component<{ side: "left" | "right"; children: JSX.Element; }> = properties => (
  <div classList={{ "dial-cell": true, "dial-cell-left": properties.side === "left", "dial-cell-right": properties.side === "right" }}>
    {properties.children}
  </div>
);

export const PanelThumbnail: Component<{
  panel: Panel;
  pressedKeys?: Set<number>;
  dialLevels?: Array<number | null>;
  pressedDials?: Set<number>;
}> = properties => (
  <div class="stage stage-compact">
    <div
      classList={{
        "key-grid": true,
        "key-grid-compact": true,
        "key-grid-dials": hasDials(properties.panel),
      }}
      style={gridStyle(properties.panel)}
    >
      <Show when={hasDials(properties.panel)}>
        <DialCell side="left">
          <DialIndicator
            index={0}
            color={panelDial(properties.panel, 0).color}
            level={dialLevel(properties.panel, 0, properties.dialLevels)}
            isPressed={properties.pressedDials?.has(0)}
          />
        </DialCell>
        <DialCell side="right">
          <DialIndicator
            index={1}
            color={panelDial(properties.panel, 1).color}
            level={dialLevel(properties.panel, 1, properties.dialLevels)}
            isPressed={properties.pressedDials?.has(1)}
          />
        </DialCell>
      </Show>
      <For each={cellsOf(properties.panel)}>
        {(cell) => {
          const isPressed = () => properties.pressedKeys?.has(cell.keyIndex) ?? false;
          const keyed = () => controlAt(properties.panel, cell);

          return (
            <div classList={{ "key": true, "key-pressed": isPressed() }}>
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

type PanelStageProperties = {
  panel: Panel;
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
  index: number;
  panel: Panel;
  activeIndex?: number | null;
  dialLevels?: Array<number | null>;
  isPressed?: boolean;
  onClick: () => void;
}> = properties => (
  <button
    type="button"
    class="dial-button"
    data-selected={properties.activeIndex === properties.index}
    onClick={properties.onClick}
    aria-label={`Edit dial ${properties.index + 1}`}
  >
    <DialIndicator
      index={properties.index}
      color={panelDial(properties.panel, properties.index).color}
      level={dialLevel(properties.panel, properties.index, properties.dialLevels)}
      isPressed={properties.isPressed}
    />
  </button>
);

export const PanelStage: Component<PanelStageProperties> = properties => (
  <div class="stage">
    <div
      classList={{ "key-grid": true, "key-grid-dials": hasDials(properties.panel) }}
      style={gridStyle(properties.panel)}
    >
      <Show when={hasDials(properties.panel)}>
        <DialCell side="left">
          <StageDial
            index={0}
            panel={properties.panel}
            activeIndex={properties.activeDialIndex}
            dialLevels={properties.dialLevels}
            isPressed={properties.pressedDials?.has(0)}
            onClick={() => properties.onDialClick(0)}
          />
        </DialCell>
        <DialCell side="right">
          <StageDial
            index={1}
            panel={properties.panel}
            activeIndex={properties.activeDialIndex}
            dialLevels={properties.dialLevels}
            isPressed={properties.pressedDials?.has(1)}
            onClick={() => properties.onDialClick(1)}
          />
        </DialCell>
      </Show>
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
