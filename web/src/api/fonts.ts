import { getErrorMessage, isString } from "./guards";

export const fetchFontFamilies = async (): Promise<string[]> => {
  const response = await fetch("/api/fonts");

  if (!response.ok) throw new Error(await getErrorMessage(response));

  const data: unknown = await response.json();

  if (!Array.isArray(data) || !data.every(isString)) {
    throw new Error("The daemon returned an invalid font list.");
  }

  return data;
};
