import { Device, deviceGridLayout } from "../../api/inventory";

/*
 * Stream Deck models drawn to scale, in millimetres from the top-left corner of the device (the
 * rack ears included). Elgato publishes body sizes but no key sizes, so cap, pitch, radius and dial
 * numbers are measured on Elgato's straight-on product renders and scaled by the published body
 * size. Each number says where it comes from; "measured" numbers are good to about 0.5 mm.
 */

export type MmRect = { xMm: number; yMm: number; widthMm: number; heightMm: number; };

export type KeyGrid = {
  columns: number;
  rows: number;
  firstCentreXMm: number;
  firstCentreYMm: number;
  pitchXMm: number;
  pitchYMm: number;
  capWidthMm: number;
  capHeightMm: number;
  capRadiusMm: number;
  /** The dark recess around a cap, the space the cap travels into when pressed. */
  wellMm: number;
  /** Extra space after this column, for decks that split their keys into two groups. */
  groupAfterColumn: number | null;
  groupGapMm: number;
};

export type RackEars = {
  widthMm: number;
  cornerRadiusMm: number;
  /** Slot centres on the left ear; the right ear mirrors them. */
  slots: Array<{ xMm: number; yMm: number; }>;
  slotWidthMm: number;
  slotHeightMm: number;
};

export type Hardware = {
  widthMm: number;
  heightMm: number;
  /** The front plate. On a rack device it sits between the ears; otherwise it is the whole body. */
  plate: MmRect;
  plateRadiusMm: number;
  ears: RackEars | null;
  /** Recessed panels behind the dials. */
  insets: MmRect[];
  insetRadiusMm: number;
  keys: KeyGrid;
  dials: Array<{ index: number; xMm: number; yMm: number; }>;
  ringOuterMm: number;
  ringInnerMm: number;
  knobMm: number;
};

/*
 * Studio. Rack size: EIA-310, a 19 inch 1U faceplate is 482.6 by 44.45 mm and its holes sit 8.75 mm
 * from each end at 6.35 and 38.1 mm. Plate: 447 mm wide (Elgato's product page compare table), 1U
 * panel height 43.66 mm. Keys and dials: measured on Elgato's 1202 px front render and a dial
 * close-up, scaled by the 482.6 mm faceplate.
 */
const studioEarMm = (482.6 - 447) / 2;
const studioPlateTopMm = (44.45 - 43.7) / 2;
const studioInsetHeightMm = 38.2;
const studioInsetTopMm = studioPlateTopMm + (43.7 - studioInsetHeightMm) / 2;

const streamDeckStudio: Hardware = {
  widthMm: 482.6,
  heightMm: 44.45,
  plate: { xMm: studioEarMm, yMm: studioPlateTopMm, widthMm: 447, heightMm: 43.7 },
  // Square where it meets the ears in the render; measured 1.8 mm at the ears' outer corners.
  plateRadiusMm: 1.8,
  ears: {
    widthMm: studioEarMm,
    cornerRadiusMm: 1.8,
    slots: [{ xMm: 8.75, yMm: 6.35 }, { xMm: 8.75, yMm: 38.1 }],
    // Measured: stadium-shaped slots in the render's alpha channel.
    slotWidthMm: 10.8,
    slotHeightMm: 5.5,
  },
  // Measured: the recessed panel behind each dial, 43.6 by 38.2 mm, 21.1 mm from the left edge.
  insets: [
    { xMm: 21.1, yMm: studioInsetTopMm, widthMm: 43.6, heightMm: studioInsetHeightMm },
    { xMm: 482.6 - 21.1 - 43.6, yMm: studioInsetTopMm, widthMm: 43.6, heightMm: studioInsetHeightMm },
  ],
  insetRadiusMm: 2,
  keys: {
    columns: 16,
    rows: 2,
    // Measured: key 0 centre 76.4 mm from the left ear tip, rows at 13.2 and 31.3 mm.
    firstCentreXMm: 76.4,
    firstCentreYMm: 13.2,
    // Measured: 21.75 mm across inside a group, 18.1 mm down, 3.8 mm extra between the groups.
    pitchXMm: 21.75,
    pitchYMm: 18.1,
    // Measured: the cap rim's outer edge is 18.9 by 14.7 mm, so Studio caps are 4:3, radius 3.2 mm.
    capWidthMm: 18.9,
    capHeightMm: 14.7,
    capRadiusMm: 3.2,
    // Measured: the shadow ring around a cap is 19.6 by 15.9 mm.
    wellMm: 0.4,
    groupAfterColumn: 7,
    groupGapMm: 3.8,
  },
  // Measured: dial centres 35.0 and 447.6 mm from the left ear tip, centred on the plate.
  dials: [{ index: 0, xMm: 35, yMm: 22.2 }, { index: 1, xMm: 447.6, yMm: 22.2 }],
  // Measured: the LED ring is 24.6 mm outside and 20.7 mm inside, the knob top 15.8 mm.
  ringOuterMm: 24.6,
  ringInnerMm: 20.7,
  knobMm: 15.8,
};

/*
 * XL. Body: 182 by 112 mm, Elgato's XL technical specifications. Keys: measured on Elgato's only
 * straight-on XL render (ATF-06), scaled by the 182 mm body width.
 */
const streamDeckXl: Hardware = {
  widthMm: 182,
  heightMm: 112,
  plate: { xMm: 0, yMm: 0, widthMm: 182, heightMm: 112 },
  // Measured: a circle fit to the body's bottom corners.
  plateRadiusMm: 4.7,
  ears: null,
  insets: [],
  insetRadiusMm: 0,
  keys: {
    columns: 8,
    rows: 4,
    // Measured: key 0 centre 23.25 mm from the left edge and 29.8 mm from the top. The vertical
    // position comes from one image, so it is the least certain number here.
    firstCentreXMm: 23.25,
    firstCentreYMm: 29.8,
    pitchXMm: 19.37,
    pitchYMm: 19.31,
    // Measured: the raised rim's outer edge is 16.0 mm square, radius 3.2 mm; its shadow ring 16.8 mm.
    capWidthMm: 16,
    capHeightMm: 16,
    capRadiusMm: 3.2,
    wellMm: 0.4,
    groupAfterColumn: null,
    groupGapMm: 0,
  },
  dials: [],
  ringOuterMm: 0,
  ringInnerMm: 0,
  knobMm: 0,
};

const hardwareByModel: Partial<Record<string, Hardware>> = {
  "Stream Deck Studio": streamDeckStudio,
  "Stream Deck XL": streamDeckXl,
};

// Models without measurements get XL keys on a plain body sized to their grid.
const plainMarginMm = 15.25;

const plainHardware = (columns: number, rows: number): Hardware => {
  const keys = { ...streamDeckXl.keys, columns, rows, firstCentreXMm: plainMarginMm + 8, firstCentreYMm: plainMarginMm + 8 };
  const widthMm = 2 * plainMarginMm + (columns - 1) * keys.pitchXMm + keys.capWidthMm;
  const heightMm = 2 * plainMarginMm + (rows - 1) * keys.pitchYMm + keys.capHeightMm;

  return {
    ...streamDeckXl,
    widthMm,
    heightMm,
    plate: { xMm: 0, yMm: 0, widthMm, heightMm },
    keys,
  };
};

/** The drawing for a device, or null for a device without keys of its own, such as a dock. */
export const hardwareFor = (device: Device): Hardware | null => {
  const layout = deviceGridLayout(device.layout);

  if (layout === null) return null;

  const known = hardwareByModel[device.model];

  if (known !== undefined && known.keys.columns === layout.columns && known.keys.rows === layout.rows) return known;

  return plainHardware(layout.columns, layout.rows);
};

export const keyCentreMm = (keys: KeyGrid, column: number, row: number): { xMm: number; yMm: number; } => ({
  xMm:
    keys.firstCentreXMm
    + column * keys.pitchXMm
    + (keys.groupAfterColumn !== null && column > keys.groupAfterColumn ? keys.groupGapMm : 0),
  yMm: keys.firstCentreYMm + row * keys.pitchYMm,
});
