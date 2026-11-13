import { describe, expect, it } from "vitest";

import { commit, LIMIT, redo, silent, start, undo } from "./history";

describe("riwayat undo/redo", () => {
  it("undo lalu redo mengembalikan keadaan", () => {
    let h = start(1);
    h = commit(h, 2, "a", 0);
    h = commit(h, 3, "b", 10_000);
    h = undo(h);
    expect(h.present).toBe(2);
    h = undo(h);
    expect(h.present).toBe(1);
    expect(undo(h)).toBe(h);
    h = redo(redo(h));
    expect(h.present).toBe(3);
  });
  it("perubahan beruntun dengan kunci sama digabung (satu seret = satu undo)", () => {
    let h = start(0);
    for (let i = 1; i <= 20; i++) h = commit(h, i, "drag", i * 50);
    expect(h.past).toEqual([0]);
    expect(undo(h).present).toBe(0);
  });
  it("jeda lama atau kunci beda = langkah baru; perubahan baru menghapus redo", () => {
    let h = start(0);
    h = commit(h, 1, "t", 0);
    h = commit(h, 2, "t", 5_000);
    expect(h.past).toEqual([0, 1]);
    h = undo(h);
    h = commit(h, 9, "x", 6_000);
    expect(h.future).toEqual([]);
  });
  it("perubahan senyap tidak masuk riwayat dan riwayat dibatasi", () => {
    let h = start(0);
    h = silent(h, 5);
    expect(h.past).toEqual([]);
    for (let i = 1; i <= LIMIT + 20; i++) h = commit(h, i, `k${i}`, i);
    expect(h.past.length).toBe(LIMIT);
  });
});
