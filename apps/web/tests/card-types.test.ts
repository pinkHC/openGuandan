// @vitest-environment node
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { CARD_TYPE_NAMES } from "../app/lib/card-types";

describe("牌型中英文名称", () => {
  it("与 docs/card-types.json 保持一致并使用 joker-bomb", () => {
    const source = JSON.parse(
      readFileSync(resolve(process.cwd(), "../../docs/card-types.json"), "utf8"),
    ) as { cardTypes: Array<{ id: keyof typeof CARD_TYPE_NAMES; zh: string; en: string }> };
    expect(source.cardTypes).toEqual(
      Object.entries(CARD_TYPE_NAMES).map(([id, names]) => ({ id, ...names })),
    );
    expect(source.cardTypes.at(-1)).toEqual({ id: "joker-bomb", zh: "天王炸", en: "joker bomb" });
  });
});
