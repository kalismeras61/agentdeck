import { useMemo } from "react";
import { InvalidEventBlock } from "../components/blocks/EventBlocks";
import { EventBody } from "../components/Conversation";
import { gallerySections } from "../data/gallery";
import type { Session } from "../data/demo";
import { parseEvents } from "../lib/events";
import { ViewBody, ViewHeader } from "./ViewHeader";
import { t } from "../lib/i18n";

const sessionFor = (provider: Session["provider"]): Session => ({
  id: "gallery",
  projectId: "p-app",
  title: "Output gallery",
  provider,
  model: "",
  status: "idle",
  snippet: "",
  startedAt: 0,
  updatedAt: 0,
  raw: [],
  demo: true,
});

export function Gallery() {
  const sections = useMemo(() => gallerySections.map((s) => ({ ...s, parsed: parseEvents(s.events) })), []);
  const stats = sections.reduce((acc, s) => ({ ok: acc.ok + s.parsed.filter((p) => p.ok).length, bad: acc.bad + s.parsed.filter((p) => !p.ok).length }), { ok: 0, bad: 0 });

  return (
    <div className="flex h-full flex-col">
      <ViewHeader title={t("Output gallery")} />
      <ViewBody
        heading={t("Output gallery")}
        lede={t("Every renderer AgentDeck uses, fed with raw sample input through the same validation as live provider output. {ok} samples rendered, {bad} rejected on purpose.", stats)}
      >
        <nav aria-label={t("Gallery sections")} className="mb-10 flex flex-wrap gap-1.5">
          {sections.map((s) => (
            <a
              key={s.id}
              href={`#g-${s.id}`}
              onClick={(e) => {
                e.preventDefault();
                document.getElementById(`g-${s.id}`)?.scrollIntoView({ behavior: document.documentElement.dataset.motion === "reduced" ? "auto" : "smooth", block: "start" });
              }}
              className="rounded-full border border-line px-3 py-1 text-[12.5px] text-fg-2 hover:bg-elevated hover:text-fg"
            >
              {t(s.title)}
            </a>
          ))}
        </nav>
        <div className="space-y-14">
          {sections.map((s) => (
            <section key={s.id} id={`g-${s.id}`} aria-labelledby={`gh-${s.id}`} className="scroll-mt-6">
              <h3 id={`gh-${s.id}`} className="text-[15px] font-semibold">
                {t(s.title)}
              </h3>
              <p className="mb-4 mt-1 max-w-[68ch] text-[13px] leading-[1.6] text-fg-3">{t(s.note)}</p>
              <div className="space-y-4">
                {s.parsed.map((p, i) =>
                  p.ok ? (
                    <div key={p.event.eventId}>
                      <EventBody e={p.event} session={sessionFor(p.event.provider)} />
                    </div>
                  ) : (
                    <InvalidEventBlock key={`${p.eventId}-${i}`} reason={p.reason} />
                  ),
                )}
              </div>
            </section>
          ))}
        </div>
      </ViewBody>
    </div>
  );
}
