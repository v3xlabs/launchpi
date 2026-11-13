import { Component, For, JSX, onCleanup } from "solid-js";

import { litRingSegments, ringSegments } from "../DialIndicator";

type DialKnobProperties = {
  label: string;
  colorHex: string;
  level: number;
  isPressed: boolean;
  isDisabled: boolean;
  style: JSX.CSSProperties;
  knobStyle: JSX.CSSProperties;
  onTurn: (detents: number) => void;
  onPush: (isPressed: boolean) => void;
};

const dragPxPerDetent = 12;
const wheelDeltaPerDetent = 100;
const wheelLinePx = 33;
/** A knob held this long without moving counts as pushed; moving first makes the gesture a turn. */
const pushDelayMs = 150;
const dragSlopPx = 4;
const segmentIndexes = Array.from({ length: ringSegments }, (_, index) => index);

/** A Studio dial: the LED ring lit to the level, and the knob inside it that turns and pushes. */
export const DialKnob: Component<DialKnobProperties> = (properties) => {
  let wheelRemainder = 0;
  let pushTimer: number | undefined;
  let isPushSent = false;

  const releasePush = () => {
    globalThis.clearTimeout(pushTimer);
    pushTimer = undefined;

    if (!isPushSent) return;

    isPushSent = false;
    properties.onPush(false);
  };

  // The gesture in progress, if any. Leaving the window or the tab ends it with no push left behind.
  let endGesture: (() => void) | undefined;

  const onVisibilityChange = () => {
    if (document.visibilityState === "hidden") endGesture?.();
  };

  const onWindowBlur = () => endGesture?.();

  globalThis.addEventListener("blur", onWindowBlur);
  document.addEventListener("visibilitychange", onVisibilityChange);
  onCleanup(() => {
    globalThis.removeEventListener("blur", onWindowBlur);
    document.removeEventListener("visibilitychange", onVisibilityChange);
    endGesture?.();
    releasePush();
  });

  const onPointerDown = (event: PointerEvent & { currentTarget: HTMLDivElement; }) => {
    if (properties.isDisabled || event.button !== 0) return;

    const element = event.currentTarget;
    const pointerId = event.pointerId;
    const startY = event.clientY;
    let appliedDetents = 0;
    let isTurning = false;

    element.setPointerCapture(pointerId);
    pushTimer = globalThis.setTimeout(() => {
      pushTimer = undefined;
      isPushSent = true;
      properties.onPush(true);
    }, pushDelayMs);

    const onMove = (moveEvent: PointerEvent) => {
      if (isPushSent || moveEvent.pointerId !== pointerId) return;

      const travelPx = startY - moveEvent.clientY;

      if (!isTurning && Math.abs(travelPx) < dragSlopPx) return;

      if (!isTurning) {
        isTurning = true;
        globalThis.clearTimeout(pushTimer);
        pushTimer = undefined;
      }

      // Up turns clockwise, as on the hardware where up is the far side of the knob.
      const detents = Math.trunc(travelPx / dragPxPerDetent);

      if (detents !== appliedDetents) {
        properties.onTurn(detents - appliedDetents);
        appliedDetents = detents;
      }
    };

    const detach = () => {
      endGesture = undefined;
      element.removeEventListener("pointermove", onMove);
      element.removeEventListener("pointerup", onUp);
      element.removeEventListener("pointercancel", onUp);
      element.removeEventListener("lostpointercapture", onUp);
    };

    const onUp = (upEvent: PointerEvent) => {
      if (upEvent.pointerId !== pointerId) return;

      detach();

      // A quick click never reached the push delay: it still pushes and releases the knob once.
      if (!isTurning && pushTimer !== undefined && upEvent.type === "pointerup") {
        globalThis.clearTimeout(pushTimer);
        pushTimer = undefined;
        properties.onPush(true);
        properties.onPush(false);

        return;
      }

      releasePush();
    };

    endGesture = () => {
      detach();
      releasePush();
    };
    element.addEventListener("pointermove", onMove);
    element.addEventListener("pointerup", onUp);
    element.addEventListener("pointercancel", onUp);
    element.addEventListener("lostpointercapture", onUp);
  };

  const onWheel = (event: WheelEvent) => {
    if (properties.isDisabled) return;

    event.preventDefault();
    wheelRemainder -= event.deltaMode === WheelEvent.DOM_DELTA_LINE ? event.deltaY * wheelLinePx : event.deltaY;

    const detents = Math.trunc(wheelRemainder / wheelDeltaPerDetent);

    if (detents === 0) return;

    wheelRemainder -= detents * wheelDeltaPerDetent;
    properties.onTurn(detents);
  };

  const onKeyDown = (event: KeyboardEvent) => {
    const stepByKey: Partial<Record<string, number>> = { ArrowUp: 1, ArrowRight: 1, ArrowDown: -1, ArrowLeft: -1 };
    const step = stepByKey[event.key];

    if (step !== undefined) {
      event.preventDefault();
      properties.onTurn(step);

      return;
    }

    if (!isPushSent && !event.repeat && (event.key === "Enter" || event.key === " ")) {
      event.preventDefault();
      isPushSent = true;
      properties.onPush(true);
    }
  };

  const onKeyUp = (event: KeyboardEvent) => {
    if (event.key === "Enter" || event.key === " ") releasePush();
  };

  return (
    <div
      class="device-dial"
      role="slider"
      tabIndex={properties.isDisabled ? -1 : 0}
      aria-label={properties.label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={properties.level}
      aria-valuetext={`${properties.level}%`}
      aria-disabled={properties.isDisabled}
      data-pressed={properties.isPressed ? "" : undefined}
      style={{ ...properties.style, "--dial-color": properties.colorHex }}
      onPointerDown={onPointerDown}
      onWheel={onWheel}
      onKeyDown={onKeyDown}
      onKeyUp={onKeyUp}
      onBlur={releasePush}
    >
      <svg class="device-ring" viewBox="-50 -50 100 100" aria-hidden="true">
        <For each={segmentIndexes}>
          {segment => (
            <rect
              x="-5"
              y="-50"
              width="10"
              height="8"
              rx="1.5"
              transform={`rotate(${180 + (segment + 0.5) * (360 / ringSegments)})`}
              data-lit={segment < litRingSegments(properties.level) ? "" : undefined}
            />
          )}
        </For>
      </svg>
      <span class="device-knob" style={properties.knobStyle} />
    </div>
  );
};
