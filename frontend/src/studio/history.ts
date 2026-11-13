// Riwayat Undo/Redo murni (prototipe ADR-0017). Perubahan beruntun dengan `key` sama dalam
// jendela waktu pendek digabung menjadi satu langkah (mis. satu kali seret = satu undo,
// mengetik satu kalimat = satu undo).

export interface History<T> { present: T; past: T[]; future: T[]; key: string; at: number }

export const LIMIT = 100;
export const COALESCE_MS = 800;

export const start = <T,>(present: T): History<T> => ({ present, past: [], future: [], key: "", at: 0 });

export function commit<T>(h: History<T>, next: T, key: string, now: number): History<T> {
  if (next === h.present) return h;
  const merge = key !== "" && key === h.key && now - h.at < COALESCE_MS;
  return { present: next, past: merge ? h.past : [...h.past, h.present].slice(-LIMIT), future: [], key, at: now };
}

/** Perubahan sistem (mis. tinggi otomatis): tidak masuk riwayat. */
export const silent = <T,>(h: History<T>, next: T): History<T> => (next === h.present ? h : { ...h, present: next });

export function undo<T>(h: History<T>): History<T> {
  if (h.past.length === 0) return h;
  return { present: h.past[h.past.length - 1]!, past: h.past.slice(0, -1), future: [h.present, ...h.future], key: "", at: 0 };
}

export function redo<T>(h: History<T>): History<T> {
  if (h.future.length === 0) return h;
  return { present: h.future[0]!, past: [...h.past, h.present], future: h.future.slice(1), key: "", at: 0 };
}
