import { useEffect, useRef, useState } from "react";
import type * as MonacoApi from "monaco-editor/editor/editor.api";
import { loadMonaco, type Monaco } from "../../lib/monaco";

let langs: Promise<unknown> | null = null;
const loadLanguages = () =>
  (langs ??= Promise.all([import("monaco-editor/languages/definitions/markdown/register"), import("monaco-editor/languages/definitions/ini/register")]));

/**
 * An editable file. `docKey` identifies the document: a new key replaces the text (and the undo
 * history); the same key keeps what is being typed.
 */
export function TextEditor({
  docKey,
  value,
  language,
  dark,
  label,
  onChange,
  onSave,
}: {
  docKey: string;
  value: string;
  language: "markdown" | "ini";
  dark: boolean;
  label: string;
  onChange: (text: string) => void;
  onSave: () => void;
}) {
  const host = useRef<HTMLDivElement>(null);
  const editor = useRef<MonacoApi.editor.IStandaloneCodeEditor | null>(null);
  const [monaco, setMonaco] = useState<Monaco | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  const handlers = useRef({ onChange, onSave });
  handlers.current = { onChange, onSave };

  useEffect(() => {
    let alive = true;
    Promise.all([loadMonaco(), loadLanguages()])
      .then(([m]) => alive && setMonaco(m))
      .catch((e: unknown) => alive && setFailed(e instanceof Error ? e.message : "The editor failed to load"));
    return () => {
      alive = false;
    };
  }, []);

  useEffect(() => {
    if (!monaco || !host.current) return;
    const ed = monaco.editor.create(host.current, {
      automaticLayout: true,
      minimap: { enabled: false },
      fontFamily: '"JetBrains Mono", ui-monospace, Menlo, monospace',
      fontSize: 13,
      lineHeight: 21,
      wordWrap: "on",
      wrappingIndent: "same",
      scrollBeyondLastLine: false,
      renderLineHighlight: "line",
      padding: { top: 14, bottom: 14 },
      lineNumbersMinChars: 3,
      scrollbar: { verticalScrollbarSize: 10, horizontalScrollbarSize: 10 },
      guides: { indentation: false },
      stickyScroll: { enabled: false },
      unicodeHighlight: { ambiguousCharacters: false },
      ariaLabel: label,
    });
    ed.onDidChangeModelContent(() => handlers.current.onChange(ed.getValue()));
    ed.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => handlers.current.onSave());
    editor.current = ed;
    return () => {
      ed.getModel()?.dispose();
      ed.dispose();
      editor.current = null;
    };
  }, [monaco]); // eslint-disable-line react-hooks/exhaustive-deps

  // A new document: new model. Same document: only replace text that differs (e.g. Revert).
  const shownKey = useRef<string | null>(null);
  useEffect(() => {
    const ed = editor.current;
    if (!monaco || !ed) return;
    if (shownKey.current !== docKey) {
      shownKey.current = docKey;
      const prev = ed.getModel();
      ed.setModel(monaco.editor.createModel(value, language));
      prev?.dispose();
      ed.setScrollTop(0);
    } else if (ed.getValue() !== value) {
      ed.getModel()?.pushEditOperations([], [{ range: ed.getModel()!.getFullModelRange(), text: value }], () => null);
    }
  }, [monaco, docKey, value, language]);

  useEffect(() => {
    monaco?.editor.setTheme(dark ? "agentdeck-dark" : "agentdeck-light");
  }, [monaco, dark]);

  useEffect(() => {
    editor.current?.updateOptions({ ariaLabel: label });
  }, [label]);

  if (failed) return <p className="p-6 text-[13px] text-error">{failed}</p>;
  return (
    <div ref={host} className="absolute inset-0">
      {!monaco && <p className="p-6 text-[13px] text-fg-3">Opening the editor…</p>}
    </div>
  );
}
