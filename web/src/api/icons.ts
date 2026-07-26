import { isString } from "./guards";

/** Icon names matching a search, ranked by the daemon and capped so the grid stays renderable. */
export const fetchIcons = async (query: string): Promise<string[]> => {
  const response = await fetch(`/api/icons?q=${encodeURIComponent(query)}`);

  if (!response.ok) return [];

  const data: unknown = await response.json();

  return Array.isArray(data) && data.every(isString) ? data : [];
};
