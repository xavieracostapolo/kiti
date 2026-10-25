import { describe, expect, it } from "vitest";
import { displayName } from "./names";

describe("displayName", () => {
  it("usa el último segmento del cwd", () => {
    expect(displayName("/Users/dev/kiti")).toBe("kiti");
    expect(displayName("/Users/dev/kiti/")).toBe("kiti");
    expect(displayName("C:\\src\\billing")).toBe("billing");
  });

  it("recorta nombres largos con elipsis", () => {
    expect(displayName("/w/un-proyecto-con-nombre-larguisimo")).toBe("un-proyecto-con-n…");
  });

  it("acepta un máximo propio", () => {
    expect(displayName("/w/notifications-service", 15)).toBe("notifications-…");
  });

  it("cwd vacío o raíz", () => {
    expect(displayName("")).toBe("?");
    expect(displayName("/")).toBe("/");
  });
});
