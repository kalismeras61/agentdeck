import { Check, ImageOff } from "lucide-react";
import { isValidElement, memo, type ReactElement, type ReactNode } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import type { Lang } from "../../lib/highlight";
import { CodeBlock } from "./code";

const LANG_ALIASES: Record<string, Lang> = {
  dart: "dart",
  json: "json",
  yaml: "yaml",
  yml: "yaml",
  sh: "shellscript",
  bash: "shellscript",
  shell: "shellscript",
  zsh: "shellscript",
};

function textOf(node: ReactNode): string {
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(textOf).join("");
  if (isValidElement(node)) return textOf((node.props as { children?: ReactNode }).children);
  return "";
}

const components: Components = {
  pre({ children }) {
    const child = (Array.isArray(children) ? children[0] : children) as ReactElement<{ className?: string; children?: ReactNode }>;
    const cls = isValidElement(child) ? (child.props.className ?? "") : "";
    const lang = /language-(\w+)/.exec(cls)?.[1]?.toLowerCase() ?? "";
    const code = textOf(isValidElement(child) ? child.props.children : children).replace(/\n$/, "");
    return (
      <div className="my-3">
        <CodeBlock code={code} lang={LANG_ALIASES[lang] ?? "text"} title={lang || "text"} />
      </div>
    );
  },
  a({ href, children }) {
    const safe = typeof href === "string" && /^(https?:|mailto:)/i.test(href);
    if (!safe) return <span>{children}</span>;
    return (
      <a href={href} target="_blank" rel="noopener noreferrer nofollow">
        {children}
      </a>
    );
  },
  // Remote images in agent markdown are not fetched. Images arrive as validated image events instead.
  img({ alt }) {
    return (
      <span className="inline-flex items-center gap-1.5 rounded-md border border-line px-2 py-0.5 text-xs text-fg-2">
        <ImageOff size={12} /> {alt || "Image not loaded"}
      </span>
    );
  },
  input({ checked, type }) {
    if (type !== "checkbox") return null;
    return (
      <span
        role="img"
        aria-label={checked ? "Done" : "Not done"}
        className={
          checked
            ? "mt-[3px] inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-[5px] bg-violet text-white"
            : "mt-[3px] inline-flex h-4 w-4 shrink-0 rounded-[5px] border border-line"
        }
      >
        {checked && <Check size={11} strokeWidth={3} />}
      </span>
    );
  },
};

/** Untrusted markdown: no raw HTML (react-markdown escapes it), no remote images, only http(s)/mailto links. */
export const Markdown = memo(function Markdown({ text }: { text: string }) {
  return (
    <div className="md text-[14.5px] leading-[1.7] text-fg/90">
      <ReactMarkdown remarkPlugins={[remarkGfm]} components={components} skipHtml>
        {text}
      </ReactMarkdown>
    </div>
  );
});
