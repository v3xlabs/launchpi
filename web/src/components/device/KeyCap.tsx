import { Component, JSX, Show } from "solid-js";

import { Control } from "../../api/inventory";
import { KeyImage } from "../KeyImage";

type KeyCapProperties = {
  label: string;
  control: Control | undefined;
  isPressed: boolean;
  isDimmed: boolean;
  isDisabled: boolean;
  style: JSX.CSSProperties;
  capStyle: JSX.CSSProperties;
  onPress: (isPressed: boolean) => void;
};

const isPressKey = (event: KeyboardEvent) => event.key === " " || event.key === "Enter";

/**
 * One key: a dark well and the cap inside it. The screen sits behind a clear cap, so the rendered
 * image fills the whole cap face and the cap's corners clip it.
 */
export const KeyCap: Component<KeyCapProperties> = properties => (
  <button
    type="button"
    class="device-key"
    style={properties.style}
    aria-label={properties.label}
    aria-pressed={properties.isPressed}
    disabled={properties.isDisabled}
    data-pressed={properties.isPressed ? "" : undefined}
    onPointerDown={(event) => {
      if (event.button !== 0) return;

      event.currentTarget.setPointerCapture(event.pointerId);
      properties.onPress(true);
    }}
    onPointerUp={() => properties.onPress(false)}
    onPointerCancel={() => properties.onPress(false)}
    onLostPointerCapture={() => properties.onPress(false)}
    onKeyDown={(event) => {
      if (event.repeat || !isPressKey(event)) return;

      event.preventDefault();
      properties.onPress(true);
    }}
    onKeyUp={(event) => {
      if (isPressKey(event)) properties.onPress(false);
    }}
    onBlur={() => properties.onPress(false)}
  >
    <span class="device-cap" style={properties.capStyle} data-dimmed={properties.isDimmed ? "" : undefined}>
      <Show when={properties.control}>
        {control => <KeyImage control={control()} isPressed={properties.isPressed} />}
      </Show>
    </span>
  </button>
);
