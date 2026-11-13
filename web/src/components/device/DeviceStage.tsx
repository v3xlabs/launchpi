import { Component, createMemo, createResource, createSignal, For, Index, JSX, onCleanup, Show } from "solid-js";

import { DeviceInput, sendDeviceInput } from "../../api/input";
import { Device, fetchDevicePresentation, Panel, panelDial } from "../../api/inventory";
import { useInventory } from "../../context/InventoryContext";
import { toHex } from "../../utils/rendered";
import { readStorage, writeStorage } from "../../utils/storage";
import { dialLevel } from "../PanelPreview";
import { DialKnob } from "./DialKnob";
import { Hardware, keyCentreMm, MmRect } from "./hardware";
import { KeyCap } from "./KeyCap";

/** 96 CSS px per inch is 3.78 px per mm: the drawing never shows a device larger than it is. */
const lifeSizePxPerMm = 96 / 25.4;
/** Below this cap width the drawing stops shrinking and the desk scrolls sideways instead. */
const minimumCapPx = 32;
const rackEarsKey = "launchpi-rack-ears";
const unlitDialColor = "#3f3f46";
/** Holds are renewed this often while held; the daemon lets go of a hold it has not heard of for 3 s. */
const holdRenewalMs = 1000;
/** A request that takes longer is given up; the daemon's lease covers a press whose release is lost. */
const requestTimeoutMs = 2000;
/** Presses and turns beyond this many waiting requests are dropped. Releases are always queued. */
const pendingLimit = 16;

const isRelease = (input: DeviceInput) => input.kind !== "dial_turn" && !input.is_pressed;

type DeviceStageProperties = { device: Device; panel: Panel; hardware: Hardware; };

const mm = (value: number) => `calc(${value} * var(--mm))`;

const placed = (rect: MmRect, originXMm: number, originYMm: number): JSX.CSSProperties => ({
  left: mm(rect.xMm - originXMm),
  top: mm(rect.yMm - originYMm),
  width: mm(rect.widthMm),
  height: mm(rect.heightMm),
});

type Frame = { originXMm: number; originYMm: number; widthMm: number; heightMm: number; };

const frameOf = (hardware: Hardware, isShowingEars: boolean): Frame =>
  (isShowingEars && hardware.ears !== null
    ? { originXMm: 0, originYMm: 0, widthMm: hardware.widthMm, heightMm: hardware.heightMm }
    : {
        originXMm: hardware.plate.xMm,
        originYMm: hardware.plate.yMm,
        widthMm: hardware.plate.widthMm,
        heightMm: hardware.plate.heightMm,
      });

/**
 * The device as it sits on the desk, to scale, with live key images. A click presses a key through
 * the daemon, exactly like a finger on the hardware.
 */
const LiveStage: Component<DeviceStageProperties> = (properties) => {
  const store = useInventory();
  // Fixed for the stage's life: `DeviceStage` mounts a new one for another device.
  const surfaceId = properties.device.surface_id;
  const presentationKey = () => `${surfaceId}:${store.presentationVersionFor(surfaceId)}`;
  const [presentation] = createResource(presentationKey, () => fetchDevicePresentation(surfaceId));
  const controls = createMemo(
    () => new Map((presentation.latest?.controls ?? []).map(entry => [entry.key_index, entry])),
  );
  const [isShowingEars, setIsShowingEars] = createSignal(readStorage(rackEarsKey) !== "off");
  const frame = createMemo(() => frameOf(properties.hardware, isShowingEars()));
  const [deskWidthPx, setDeskWidthPx] = createSignal(0);
  const pxPerMm = () =>
    Math.max(
      minimumCapPx / properties.hardware.keys.capWidthMm,
      Math.min(deskWidthPx() / frame().widthMm, lifeSizePxPerMm),
    );
  const canPress = () => properties.device.is_enabled && properties.device.status === "connected";

  // Keys and pushes this browser holds down. Each one is released before the page lets go of it.
  const [heldKeys, setHeldKeys] = createSignal<ReadonlySet<number>>(new Set());
  const [heldDials, setHeldDials] = createSignal<ReadonlySet<number>>(new Set());
  const hardwarePressedKeys = createMemo(() => new Set(store.pressedKeysFor(surfaceId)));

  // One request at a time and in order, so a release never reaches the daemon before its press.
  let queue: DeviceInput[] = [];
  let isDraining = false;
  let isMounted = true;

  const drain = async () => {
    if (isDraining) return;

    isDraining = true;
    let next = queue.shift();

    while (next !== undefined) {
      try {
        await sendDeviceInput(surfaceId, next, AbortSignal.timeout(requestTimeoutMs));
      }
      catch (error) {
        const isTimeout = error instanceof DOMException && error.name === "TimeoutError";

        if (isMounted && !isTimeout) {
          store.setError(error instanceof Error ? error.message : "Unable to send the input to the device.");
        }
      }

      next = queue.shift();
    }
    isDraining = false;
  };

  const send = (input: DeviceInput) => {
    if (!isRelease(input) && queue.length >= pendingLimit) return;

    queue.push(input);
    void drain();
  };

  const pressKey = (keyIndex: number, isPressed: boolean) => {
    if (heldKeys().has(keyIndex) === isPressed) return;

    const next = new Set(heldKeys());

    if (isPressed) next.add(keyIndex);
    else next.delete(keyIndex);

    setHeldKeys(next);
    send({ kind: "key", key_index: keyIndex, is_pressed: isPressed });
  };

  const pushDial = (dialIndex: number, isPressed: boolean) => {
    const next = new Set(heldDials());

    if (isPressed) next.add(dialIndex);
    else next.delete(dialIndex);

    setHeldDials(next);
    send({ kind: "dial_press", dial_index: dialIndex, is_pressed: isPressed });
  };

  const releaseAll = () => {
    for (const keyIndex of heldKeys()) pressKey(keyIndex, false);

    for (const dialIndex of heldDials()) pushDial(dialIndex, false);
  };

  const onVisibilityChange = () => {
    if (document.visibilityState === "hidden") releaseAll();
  };

  const renewal = globalThis.setInterval(() => {
    for (const keyIndex of heldKeys()) send({ kind: "key", key_index: keyIndex, is_pressed: true });

    for (const dialIndex of heldDials()) send({ kind: "dial_press", dial_index: dialIndex, is_pressed: true });
  }, holdRenewalMs);

  window.addEventListener("blur", releaseAll);
  document.addEventListener("visibilitychange", onVisibilityChange);
  onCleanup(() => {
    isMounted = false;
    globalThis.clearInterval(renewal);
    window.removeEventListener("blur", releaseAll);
    document.removeEventListener("visibilitychange", onVisibilityChange);
    // Presses and turns made on this stage are stale once it is gone; its releases still go out.
    queue = queue.filter(isRelease);
    releaseAll();
  });

  let deskObserver: ResizeObserver | undefined;

  onCleanup(() => deskObserver?.disconnect());

  const observeDesk = (element: HTMLElement) => {
    deskObserver = new ResizeObserver(([entry]) => setDeskWidthPx(entry.contentRect.width));
    deskObserver.observe(element);
  };

  const toggleEars = () => {
    const isNext = !isShowingEars();

    setIsShowingEars(isNext);
    writeStorage(rackEarsKey, isNext ? "on" : "off");
  };

  const keys = () => properties.hardware.keys;
  const keyIndexes = () => Array.from({ length: keys().columns * keys().rows }, (_, index) => index);
  const keyLabel = (keyIndex: number) => {
    const column = keyIndex % keys().columns;
    const row = Math.floor(keyIndex / keys().columns);
    const name = controls().get(keyIndex)?.control.name;

    return `Key ${row + 1}:${column + 1}${name === undefined ? ", empty" : `, ${name}`}`;
  };

  return (
    <div class="grid">
      <section class="device-desk" ref={observeDesk} aria-label={properties.device.model}>
        <div
          class="device-body"
          style={{
            "--mm": `${pxPerMm()}px`,
            "width": mm(frame().widthMm),
            "height": mm(frame().heightMm),
            "border-radius": mm(
              isShowingEars() && properties.hardware.ears !== null
                ? properties.hardware.ears.cornerRadiusMm
                : properties.hardware.plateRadiusMm,
            ),
          }}
        >
          <Show when={isShowingEars() && properties.hardware.ears}>
            {ears => (
              <For each={[0, properties.hardware.widthMm - ears().widthMm]}>
                {(earXMm, side) => (
                  <div
                    class="device-ear"
                    style={{
                      ...placed({ xMm: earXMm, yMm: 0, widthMm: ears().widthMm, heightMm: properties.hardware.heightMm }, 0, 0),
                      "border-radius": side() === 0
                        ? `${mm(ears().cornerRadiusMm)} 0 0 ${mm(ears().cornerRadiusMm)}`
                        : `0 ${mm(ears().cornerRadiusMm)} ${mm(ears().cornerRadiusMm)} 0`,
                    }}
                  >
                    <For each={ears().slots}>
                      {slot => (
                        <i
                          class="device-slot"
                          style={placed(
                            {
                              xMm: (side() === 0 ? slot.xMm : ears().widthMm - slot.xMm) - ears().slotWidthMm / 2,
                              yMm: slot.yMm - ears().slotHeightMm / 2,
                              widthMm: ears().slotWidthMm,
                              heightMm: ears().slotHeightMm,
                            },
                            0,
                            0,
                          )}
                        />
                      )}
                    </For>
                  </div>
                )}
              </For>
            )}
          </Show>
          <div
            class="device-plate"
            style={{
              ...placed(properties.hardware.plate, frame().originXMm, frame().originYMm),
              "border-radius": isShowingEars() && properties.hardware.ears !== null
                ? "0"
                : mm(properties.hardware.plateRadiusMm),
            }}
          />
          <For each={properties.hardware.insets}>
            {inset => (
              <div
                class="device-inset"
                style={{
                  ...placed(inset, frame().originXMm, frame().originYMm),
                  "border-radius": mm(properties.hardware.insetRadiusMm),
                }}
              />
            )}
          </For>
          <Index each={keyIndexes()}>
            {(keyIndex) => {
              const centre = () =>
                keyCentreMm(keys(), keyIndex() % keys().columns, Math.floor(keyIndex() / keys().columns));
              const entry = () => controls().get(keyIndex());
              const isPressed = () => heldKeys().has(keyIndex()) || hardwarePressedKeys().has(keyIndex());

              return (
                <KeyCap
                  label={keyLabel(keyIndex())}
                  control={entry()?.control}
                  isPressed={isPressed()}
                  isDimmed={entry()?.is_dimmed ?? false}
                  isDisabled={!canPress()}
                  style={{
                    ...placed(
                      {
                        xMm: centre().xMm - keys().capWidthMm / 2 - keys().wellMm,
                        yMm: centre().yMm - keys().capHeightMm / 2 - keys().wellMm,
                        widthMm: keys().capWidthMm + 2 * keys().wellMm,
                        heightMm: keys().capHeightMm + 2 * keys().wellMm,
                      },
                      frame().originXMm,
                      frame().originYMm,
                    ),
                    "border-radius": mm(keys().capRadiusMm + keys().wellMm),
                  }}
                  capStyle={{ "inset": mm(keys().wellMm), "border-radius": mm(keys().capRadiusMm) }}
                  onPress={isNextPressed => pressKey(keyIndex(), isNextPressed)}
                />
              );
            }}
          </Index>
          <For each={properties.hardware.dials}>
            {(placement) => {
              const dial = () => panelDial(properties.panel, placement.index);
              const level = () => {
                const current = dial();

                if (current === null) return 0;

                const liveLevels = store.dialLevelsFor(surfaceId);

                return Math.round(dialLevel(current, liveLevels));
              };
              const isPushed = () =>
                heldDials().has(placement.index) || store.pressedDialsFor(surfaceId).has(placement.index);
              const ringMm = properties.hardware.ringOuterMm;
              const knobInsetMm = (ringMm - properties.hardware.knobMm) / 2;

              return (
                <DialKnob
                  label={`Dial ${placement.index + 1}`}
                  colorHex={toHex(dial()?.color ?? null, unlitDialColor)}
                  level={level()}
                  isPressed={isPushed()}
                  isDisabled={!canPress()}
                  style={placed(
                    { xMm: placement.xMm - ringMm / 2, yMm: placement.yMm - ringMm / 2, widthMm: ringMm, heightMm: ringMm },
                    frame().originXMm,
                    frame().originYMm,
                  )}
                  knobStyle={{ inset: mm(knobInsetMm) }}
                  onTurn={detents => send({ kind: "dial_turn", dial_index: placement.index, detents })}
                  onPush={isPressed => pushDial(placement.index, isPressed)}
                />
              );
            }}
          </For>
        </div>
      </section>
      <div class="device-hint">
        <span>
          <Show when={canPress()} fallback="Keys can be pressed here once the device is connected and enabled.">
            {properties.hardware.dials.length > 0
              ? "Click a key to press it. Scroll or drag a dial to turn it, click it to push."
              : "Click a key to press it."}
          </Show>
        </span>
        <Show when={properties.hardware.ears !== null}>
          <button
            type="button"
            class="toggle-chip"
            aria-pressed={isShowingEars()}
            onClick={toggleEars}
          >
            Rack ears
          </button>
        </Show>
      </div>
    </div>
  );
};

/**
 * One stage per device: moving to another device unmounts the stage, which releases its keys there,
 * and every request a stage makes goes to the device it was made for.
 */
export const DeviceStage: Component<DeviceStageProperties> = properties => (
  <Show when={properties.device.surface_id} keyed>
    <LiveStage device={properties.device} panel={properties.panel} hardware={properties.hardware} />
  </Show>
);
