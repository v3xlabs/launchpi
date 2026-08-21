import { request } from "./guards";

export const setSurfaceDisplay = (surfaceIds: string[], isDisplayOff: boolean): Promise<Response> =>
  request("/api/surfaces/display", "PUT", { surface_ids: surfaceIds, is_display_off: isDisplayOff });

export const setSurfaceBrightness = (surfaceIds: string[], brightness: number): Promise<Response> =>
  request("/api/surfaces/brightness", "PUT", { surface_ids: surfaceIds, brightness });
