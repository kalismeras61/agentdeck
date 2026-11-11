import { describe, expect, it } from "vitest";
import { demoTeam } from "../data/teamDemo";
import { checkoutCommand, filterBranches, hueOf, initials, isOnline, overlaps, overlapsWith, people, reviewPrompt, ONLINE_MS } from "../lib/team";

const NOW = 1_800_000_000_000;
const team = () => demoTeam(NOW);

describe("team", () => {
  it("lists people from branch authors and shared presence, the active first", () => {
    const ps = people(team(), NOW);
    expect(ps.map((p) => p.name)).toEqual(["Ayşe Yılmaz", "Mert Kaya", "You", "Zeynep Arslan"]);
    const ayse = ps[0]!;
    expect(ayse.online).toBe(true);
    expect(ayse.working).toBe(2);
    expect(ayse.branches.map((b) => b.short)).toEqual(["feature/login-redesign"]);
    expect(ps.find((p) => p.name === "You")!.me).toBe(true);
    // Zeynep shared five hours ago: away, still listed with her branches.
    const z = ps.find((p) => p.name === "Zeynep Arslan")!;
    expect(z.online).toBe(false);
    expect(z.branches.map((b) => b.short).sort()).toEqual(["chore/deps", "fix/payment-timeout"]);
  });

  it("presence goes stale", () => {
    const p = team().presence[0]!;
    expect(isOnline({ ...p, updatedAt: NOW - ONLINE_MS + 1 }, NOW)).toBe(true);
    expect(isOnline({ ...p, updatedAt: NOW - ONLINE_MS }, NOW)).toBe(false);
  });

  it("finds files changed on more than one unmerged branch", () => {
    const o = overlaps(team().branches);
    expect(o.map((x) => [x.path, x.branches.map((b) => b.short)])).toEqual([
      ["lib/checkout/cart.dart", ["feature/checkout-v2", "spike/offline-cart"]],
      ["lib/payment/client.dart", ["fix/payment-timeout", "feature/checkout-v2"]],
      ["lib/theme/colors.dart", ["feature/login-redesign", "fix/payment-timeout"]],
    ]);
    const all = team().branches;
    const pay = all.find((b) => b.short === "fix/payment-timeout")!;
    expect(overlapsWith(pay, all).map((x) => [x.branch.short, x.files])).toEqual([
      ["feature/login-redesign", ["lib/theme/colors.dart"]],
      ["feature/checkout-v2", ["lib/payment/client.dart"]],
    ]);
    // Merged branches don't count.
    expect(overlapsWith(all.find((b) => b.short === "release/2.4")!, all)).toEqual([]);
  });

  it("filters branches", () => {
    const o = team();
    const names = (f: Parameters<typeof filterBranches>[1], q = "") => filterBranches(o, f, q, NOW).map((b) => b.short);
    expect(names("all")).toHaveLength(6);
    expect(names("active")).toEqual(["feature/login-redesign", "fix/payment-timeout", "feature/checkout-v2", "chore/deps", "spike/offline-cart"]);
    expect(names("mine")).toEqual(["feature/checkout-v2", "spike/offline-cart"]);
    expect(names("conflicts")).not.toContain("chore/deps");
    expect(names("all", "zeynep")).toEqual(["fix/payment-timeout", "chore/deps"]);
    expect(names("all", "retry")).toEqual(["fix/payment-timeout"]);
  });

  it("small parts", () => {
    expect(initials("Ayşe Yılmaz")).toBe("AY");
    expect(initials("mert.kaya@x.dev")).toBe("MK");
    expect(initials("deniz")).toBe("DE");
    expect(hueOf("A@x.dev")).toBe(hueOf("a@x.dev"));
    expect(checkoutCommand("origin", { short: "fix/x", localOnly: false })).toBe("git fetch origin && git switch --track origin/fix/x");
    expect(checkoutCommand("origin", { short: "spike", localOnly: true })).toBe("git switch spike");
    const b = team().branches[1]!;
    const p = reviewPrompt(b, "origin/main");
    expect(p).toContain("origin/fix/payment-timeout (by Mert Kaya, Zeynep Arslan)");
    expect(p).toContain("git diff origin/main...HEAD");
  });
});
