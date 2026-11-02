import { useEffect, useRef, useState } from "react";
import { t } from "../lib/i18n";
import { useStore } from "../state/store";
import { cx } from "./ui";

/** An in-place text field for a session's name. Enter or leaving the field saves; Escape cancels. */
export function RenameField({ sessionId, title, className }: { sessionId: string; title: string; className?: string }) {
  const renameSession = useStore((s) => s.renameSession);
  const setRenaming = useStore((s) => s.setRenaming);
  const [value, setValue] = useState(title);
  const [error, setError] = useState<string | null>(null);
  const ref = useRef<HTMLInputElement>(null);
  const done = useRef(false);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.focus();
    el.select();
  }, []);

  const finish = async (save: boolean) => {
    if (done.current) return;
    done.current = true;
    if (save && value.trim() && value.trim() !== title) {
      const err = await renameSession(sessionId, value);
      if (err) {
        done.current = false;
        setError(err);
        ref.current?.focus();
        return;
      }
    }
    setRenaming(null);
  };

  return (
    <span className="relative flex min-w-0 flex-1 flex-col">
      <input
        ref={ref}
        value={value}
        maxLength={120}
        aria-label={t("Session name")}
        aria-invalid={error ? true : undefined}
        onChange={(e) => {
          setValue(e.target.value);
          setError(null);
        }}
        onClick={(e) => e.stopPropagation()}
        onDoubleClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          e.stopPropagation();
          if (e.key === "Enter") void finish(true);
          else if (e.key === "Escape") void finish(false);
        }}
        onBlur={() => void finish(true)}
        className={cx(
          "select-text h-7 min-w-0 rounded-md border bg-bg px-2 text-[13px] text-fg outline-none",
          error ? "border-error" : "border-violet focus:ring-2 focus:ring-violet/30",
          className,
        )}
      />
      {error && <span className="absolute left-0 top-full z-10 mt-1 rounded-md bg-elevated px-2 py-1 text-[11.5px] text-error shadow-pop">{error}</span>}
    </span>
  );
}
