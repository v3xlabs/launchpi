/**
 * Mirrors the daemon's `$(instance:name)` parser so the browser preview shows what the hardware
 * shows. `$$` is a literal dollar; anything that is not a well-formed reference is left as written,
 * and a malformed `$(` never swallows a valid reference after it.
 */
const isReferencePart = (part: string): boolean =>
  part.length > 0 && /^[\w.-]+$/.test(part);

export const interpolateVariables = (
  template: string,
  lookup: (reference: string) => string | undefined,
): string => {
  let rendered = "";
  let rest = template;

  while (rest.includes("$")) {
    const dollar = rest.indexOf("$");

    rendered += rest.slice(0, dollar);

    const afterDollar = rest.slice(dollar + 1);

    if (afterDollar.startsWith("$")) {
      rendered += "$";
      rest = afterDollar.slice(1);
      continue;
    }

    if (!afterDollar.startsWith("(")) {
      rendered += "$";
      rest = afterDollar;
      continue;
    }

    const inside = afterDollar.slice(1);
    const close = inside.indexOf(")");

    if (close === -1) {
      rendered += "$(";
      rest = inside;
      continue;
    }

    const body = inside.slice(0, close);
    const separator = body.indexOf(":");
    const integrationId = separator === -1 ? "" : body.slice(0, separator);
    const name = separator === -1 ? "" : body.slice(separator + 1);

    if (separator === -1 || !isReferencePart(integrationId) || !isReferencePart(name)) {
      rendered += "$(";
      rest = inside;
      continue;
    }

    rendered += lookup(`${integrationId}:${name}`) ?? "";
    rest = inside.slice(close + 1);
  }

  return rendered + rest;
};
