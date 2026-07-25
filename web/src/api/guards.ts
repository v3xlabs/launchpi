/**
 * Runtime shape checks shared by every response validator. The daemon's types are mirrored by
 * hand, so these are the only thing standing between a schema drift and a blank page.
 */
export const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null;
export const isString = (value: unknown): value is string => typeof value === "string";
export const isOptionalString = (value: unknown): value is string | null =>
  value === null || isString(value);
export const isNumber = (value: unknown): value is number =>
  typeof value === "number" && Number.isFinite(value);
export const isBoolean = (value: unknown): value is boolean => typeof value === "boolean";

export const getErrorMessage = async (response: Response): Promise<string> => {
  let body: unknown;

  try {
    body = await response.json();
  }
  catch {
    body = null;
  }

  return isRecord(body) && isString(body.error)
    ? body.error
    : `Request failed with status ${response.status}`;
};

export const request = async (
  path: string,
  method: "POST" | "PATCH" | "PUT" | "DELETE",
  body?: unknown,
): Promise<Response> => {
  const response = await fetch(path, {
    method,
    headers: body === undefined ? undefined : { "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });

  if (!response.ok) throw new Error(await getErrorMessage(response));

  return response;
};

export const fetchText = async (path: string): Promise<string> => {
  const response = await fetch(path);

  if (!response.ok) throw new Error(await getErrorMessage(response));

  return response.text();
};
