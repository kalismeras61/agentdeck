import { ALT, MOD } from "./format";

export const SHORTCUTS: { group: string; items: [string, string][] }[] = /* i18n */ [
  {
    group: "Everywhere",
    items: [
      ["New session", `${MOD}N`],
      ["Search sessions", `${MOD}K`],
      ["Toggle sidebar", `${MOD}B`],
      ["All sessions", `${MOD}⇧S`],
      ["Review changes", `${MOD}⇧D`],
      ["Permissions", `${MOD}⇧A`],
      ["Git", `${MOD}⇧H`],
      ["Agent setup", `${MOD}⇧E`],
      ["Output gallery", `${MOD}⇧G`],
      ["Settings", `${MOD},`],
      ["Back to the session", "Esc"],
    ],
  },
  {
    group: "In a session",
    items: [
      ["Toggle context panel", `${MOD}.`],
      ["Context tabs", `${MOD}1…4`],
      ["Previous / next session", `${ALT}↑ / ↓`],
      ["Focus the message box", `${MOD}L`],
      ["Rich view / Terminal view", `${MOD}⇧R / T`],
      ["Send message", "↵"],
      ["New line in message", "⇧↵"],
    ],
  },
  {
    group: "Review changes",
    items: [
      ["Next / previous file", "J / K"],
      ["Next / previous change", "N / P"],
      ["Mark file viewed", "V"],
      ["Next change inside the editor", "F7"],
    ],
  },
];
