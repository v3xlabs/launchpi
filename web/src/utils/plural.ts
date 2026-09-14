/** A count and its noun, agreeing. Written once because six readouts spelled it out differently. */
export const countOf = (count: number, noun: string): string =>
  `${count} ${noun}${count === 1 ? "" : "s"}`;
