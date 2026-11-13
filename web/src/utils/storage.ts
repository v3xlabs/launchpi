// Storage can be blocked by browser policy or be full. A preference then lives in memory for the visit.

export const readStorage = (key: string): string | null => {
  try {
    return localStorage.getItem(key);
  }
  catch {
    return null;
  }
};

export const writeStorage = (key: string, value: string) => {
  try {
    localStorage.setItem(key, value);
  }
  catch {
    // Kept in memory only.
  }
};
