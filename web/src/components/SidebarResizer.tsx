import { Component, onCleanup } from "solid-js";

export const sidebarRailPx = 56;
const sidebarMinPx = 208;
const sidebarMaxPx = 420;
const sidebarDefaultPx = 264;
const keyboardStepPx = 16;

/** Released below the midpoint between the rail and the narrowest full sidebar, it folds to the rail. */
export const sidebarRailBelowPx = (sidebarRailPx + sidebarMinPx) / 2;
const velocityWindowMs = 80;
const widthKey = "launchpi-sidebar-width";
/** The page keeps at least this much room; a narrower window gets a narrower sidebar or the rail. */
const mainMinPx = 320;

// Storage can be blocked by browser policy or full. The width then lives in memory for the visit.
const readStoredWidthPx = (): number | null => {
  try {
    return Number(localStorage.getItem(widthKey));
  }
  catch {
    return null;
  }
};

const storeWidthPx = (widthPx: number) => {
  try {
    localStorage.setItem(widthKey, String(widthPx));
  }
  catch {
    // Kept in memory only.
  }
};

/** Read before the first render, so a stored width never jumps. Narrow windows start as the rail. */
export const initialSidebarWidthPx = (): number => {
  const stored = readStoredWidthPx();

  if (stored !== null && stored >= sidebarRailPx && stored <= sidebarMaxPx) return stored;

  return matchMedia("(max-width: 48rem)").matches ? sidebarRailPx : sidebarDefaultPx;
};

/**
 * The width the shell draws for the preferred width in a window this wide: it never leaves the page
 * less than its minimum, and it folds to the rail rather than show a full sidebar under its minimum.
 */
export const usableSidebarWidthPx = (preferredPx: number, viewportPx: number): number => {
  const limitPx = viewportPx - mainMinPx;

  if (preferredPx <= limitPx) return preferredPx;

  return limitPx < sidebarMinPx ? sidebarRailPx : limitPx;
};

const restingWidthPx = (widthPx: number): number => {
  if (widthPx < sidebarRailBelowPx) return sidebarRailPx;

  return Math.min(sidebarMaxPx, Math.max(sidebarMinPx, widthPx));
};

/** Past either end the edge resists at a quarter of the pointer's travel instead of stopping dead. */
const resistedWidthPx = (widthPx: number): number => {
  if (widthPx > sidebarMaxPx) return sidebarMaxPx + (widthPx - sidebarMaxPx) * 0.25;

  if (widthPx < sidebarRailPx) return sidebarRailPx - (sidebarRailPx - widthPx) * 0.25;

  return widthPx;
};

/**
 * Stiffness 500, damping 36, mass 1: a damping ratio of 0.8, so it overshoots by under a pixel and
 * rests within 300 ms. It integrates real elapsed time in 4 ms steps, so a slow frame cannot slow it.
 */
const animateSpring = (
  fromPx: number,
  toPx: number,
  velocityPxPerSecond: number,
  onFrame: (widthPx: number) => void,
): (() => void) => {
  let positionPx = fromPx;
  let velocity = velocityPxPerSecond;
  let lastMs = performance.now();
  let frame = 0;

  const step = (nowMs: number) => {
    const elapsedSeconds = Math.min(0.25, (nowMs - lastMs) / 1000);
    const stepCount = Math.max(1, Math.ceil(elapsedSeconds / 0.004));
    const stepSeconds = elapsedSeconds / stepCount;

    lastMs = nowMs;

    for (let index = 0; index < stepCount; index += 1) {
      velocity += (-500 * (positionPx - toPx) - 36 * velocity) * stepSeconds;
      positionPx += velocity * stepSeconds;
    }

    if (Math.abs(velocity) < 4 && Math.abs(positionPx - toPx) < 0.5) {
      onFrame(toPx);

      return;
    }

    onFrame(positionPx);
    frame = requestAnimationFrame(step);
  };

  frame = requestAnimationFrame(step);

  return () => cancelAnimationFrame(frame);
};

type Sample = { x: number; atMs: number; };

/**
 * The sidebar's edge. It follows the pointer exactly while dragged and springs to rest on release,
 * keeping the speed it was let go at. Keyboard steps are instant.
 */
export const SidebarResizer: Component<{
  sidebarId: string;
  widthPx: number;
  onWidthChange: (widthPx: number) => void;
}> = (properties) => {
  let cancelSpring: (() => void) | undefined;
  // One pointer drags the edge at a time; a second touch on the handle is ignored.
  let activePointerId: number | undefined;

  const settle = (targetPx: number, velocityPxPerSecond: number) => {
    cancelSpring?.();
    storeWidthPx(targetPx);

    if (matchMedia("(prefers-reduced-motion: reduce)").matches) {
      properties.onWidthChange(targetPx);

      return;
    }

    cancelSpring = animateSpring(properties.widthPx, targetPx, velocityPxPerSecond, properties.onWidthChange);
  };

  const place = (targetPx: number) => {
    cancelSpring?.();
    storeWidthPx(targetPx);
    properties.onWidthChange(targetPx);
  };

  const onPointerDown = (event: PointerEvent & { currentTarget: HTMLDivElement; }) => {
    if (activePointerId !== undefined || event.button !== 0) return;

    const handle = event.currentTarget;
    const startX = event.clientX;
    const startWidthPx = properties.widthPx;
    const samples: Sample[] = [{ x: event.clientX, atMs: event.timeStamp }];

    activePointerId = event.pointerId;
    cancelSpring?.();
    handle.setPointerCapture(event.pointerId);
    handle.dataset.dragging = "";
    document.body.dataset.resizing = "";

    const onMove = (moveEvent: PointerEvent) => {
      if (moveEvent.pointerId !== activePointerId) return;

      properties.onWidthChange(resistedWidthPx(startWidthPx + moveEvent.clientX - startX));
      samples.push({ x: moveEvent.clientX, atMs: moveEvent.timeStamp });
      while (samples.length > 2 && moveEvent.timeStamp - samples[0].atMs > velocityWindowMs) samples.shift();
    };

    const onUp = (upEvent: PointerEvent) => {
      if (upEvent.pointerId !== activePointerId) return;

      activePointerId = undefined;
      handle.removeEventListener("pointermove", onMove);
      handle.removeEventListener("pointerup", onUp);
      handle.removeEventListener("pointercancel", onUp);
      delete handle.dataset.dragging;
      delete document.body.dataset.resizing;

      const first = samples[0];
      const last = samples[samples.length - 1];
      const velocity = last.atMs > first.atMs ? ((last.x - first.x) / (last.atMs - first.atMs)) * 1000 : 0;

      settle(restingWidthPx(properties.widthPx), velocity);
    };

    handle.addEventListener("pointermove", onMove);
    handle.addEventListener("pointerup", onUp);
    handle.addEventListener("pointercancel", onUp);
  };

  const onKeyDown = (event: KeyboardEvent) => {
    const isRail = properties.widthPx <= sidebarRailPx + 1;
    const narrowerPx = properties.widthPx - keyboardStepPx;
    // The rail and the narrowest full sidebar are one step apart in both directions.
    const nextByKey: Partial<Record<string, number>> = {
      ArrowLeft: narrowerPx < sidebarMinPx ? sidebarRailPx : narrowerPx,
      ArrowRight: isRail ? sidebarMinPx : Math.min(sidebarMaxPx, properties.widthPx + keyboardStepPx),
      Home: sidebarRailPx,
      End: sidebarMaxPx,
      Enter: isRail ? sidebarDefaultPx : sidebarRailPx,
    };
    const nextPx = nextByKey[event.key];

    if (nextPx === undefined) return;

    event.preventDefault();
    place(nextPx);
  };

  onCleanup(() => {
    cancelSpring?.();
    delete document.body.dataset.resizing;
  });

  return (
    <div
      class="sidebar-resizer"
      role="separator"
      tabIndex={0}
      aria-orientation="vertical"
      aria-label="Resize sidebar"
      aria-controls={properties.sidebarId}
      aria-valuemin={sidebarRailPx}
      aria-valuemax={sidebarMaxPx}
      aria-valuenow={Math.round(properties.widthPx)}
      onPointerDown={onPointerDown}
      onDblClick={() => settle(properties.widthPx <= sidebarRailPx + 1 ? sidebarDefaultPx : sidebarRailPx, 0)}
      onKeyDown={onKeyDown}
    />
  );
};
