import { request } from "./guards";

/**
 * Names this page to the daemon. A press is a hold owned by this page: the daemon keeps it while the
 * page renews it, and lets go when the page releases it or stops renewing.
 */
export const inputClientId = `web-${Date.now().toString(36)}-${Math.random().toString(36)
  .slice(2, 10)}`;

/** What a person did to a device on screen. The daemon handles it exactly like the hardware report. */
export type DeviceInput
  = | { kind: "key"; key_index: number; is_pressed: boolean; }
    | { kind: "dial_turn"; dial_index: number; detents: number; }
    | { kind: "dial_press"; dial_index: number; is_pressed: boolean; };

export const sendDeviceInput = async (surfaceId: string, input: DeviceInput, signal: AbortSignal): Promise<void> => {
  const body = input.kind === "dial_turn" ? input : { ...input, client_id: inputClientId };

  await request(`/api/devices/${encodeURIComponent(surfaceId)}/input`, "POST", body, signal);
};
