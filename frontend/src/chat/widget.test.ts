import { describe, expect, it } from "vitest";

import { merge, SENDER_LABEL, type ChatMessage } from "./widget";

const m = (id: string, seq: number, sender: ChatMessage["sender"] = "CUSTOMER"): ChatMessage =>
  ({ id, seq, sender, body: id, created_at: "2026-11-13T00:00:00Z" });

describe("chat widget", () => {
  it("menggabungkan pesan (respons + SSE) tanpa duplikat, urut seq", () => {
    const out = merge([m("a", 1), m("c", 3)], [m("b", 2), m("c", 3), m("d", 4, "AI")]);
    expect(out.map((x) => x.id)).toEqual(["a", "b", "c", "d"]);
  });
  it("label untuk keempat sender type", () => {
    expect(Object.keys(SENDER_LABEL).sort()).toEqual(["ADMIN", "AI", "CUSTOMER", "SYSTEM"]);
    expect(SENDER_LABEL.AI).toBe("Asisten AI");
  });
});
