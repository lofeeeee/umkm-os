import { describe, expect, it } from "vitest";
import { deviceName } from "./labels";

describe("deviceName()", () => {
  it("mengenali peramban & OS umum", () => {
    expect(deviceName("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36")).toBe("Chrome di Windows");
    expect(deviceName("Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1")).toBe("Safari di iOS");
    expect(deviceName("Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Mobile Safari/537.36 Edg/130.0")).toBe("Edge di Android");
    expect(deviceName(null)).toBe("Perangkat tidak dikenal");
    expect(deviceName("curl/8.0")).toBe("curl/8.0");
  });
});
