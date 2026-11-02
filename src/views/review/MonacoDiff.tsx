import { forwardRef, useEffect, useImperativeHandle, useLayoutEffect, useRef, useState } from "react";
import type * as MonacoApi from "monaco-editor/editor/editor.api";
import { loadMonaco, type Monaco } from "../../lib/monaco";

export interface DiffHandle {
  next: () => void;
  previous: () => void;
}

interface Props {
  original: string;
  modified: string;
  language: string;
  sideBySide: boolean;
  /** Show the new version as a plain file instead of a diff. */
  fileOnly: boolean;
  dark: boolean;
  label: string;
}

const baseOptions = {
  readOnly: true,
  automaticLayout: true,
  minimap: { enabled: false },
  fontFamily: '"JetBrains Mono", ui-monospace, Menlo, monospace',
  fontSize: 12.5,
  lineHeight: 20,
  scrollBeyondLastLine: false,
  renderLineHighlight: "none" as const,
  padding: { top: 10, bottom: 10 },
  scrollbar: { verticalScrollbarSize: 10, horizontalScrollbarSize: 10 },
  guides: { indentation: false },
  stickyScroll: { enabled: false },
};

export const MonacoDiff = forwardRef<DiffHandle, Props>(function MonacoDiff({ original, modified, language, sideBySide, fileOnly, dark, label }, ref) {
  const host = useRef<HTMLDivElement>(null);
  const [monaco, setMonaco] = useState<Monaco | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  const diffEditor = useRef<MonacoApi.editor.IStandaloneDiffEditor | null>(null);
  const fileEditor = useRef<MonacoApi.editor.IStandaloneCodeEditor | null>(null);
  // A new file has no "before" side, so it always reads better unified.
  const split = sideBySide && original.length > 0;
  const splitRef = useRef(split);
  useLayoutEffect(() => {
    splitRef.current = split;
  }, [split]);

  useEffect(() => {
    let alive = true;
    loadMonaco()
      .then((m) => alive && setMonaco(m))
      .catch((e: unknown) => alive && setFailed(e instanceof Error ? e.message : "Monaco failed to load"));
    return () => {
      alive = false;
    };
  }, []);

  // (Re)create the editor when the mode changes; models are swapped below without recreating it.
  useEffect(() => {
    if (!monaco || !host.current) return;
    const el = host.current;
    if (fileOnly) {
      fileEditor.current = monaco.editor.create(el, { ...baseOptions });
    } else {
      diffEditor.current = monaco.editor.createDiffEditor(el, {
        ...baseOptions,
        renderSideBySide: splitRef.current,
        useInlineViewWhenSpaceIsLimited: false,
        ignoreTrimWhitespace: false,
        renderOverviewRuler: true,
        renderIndicators: false,
        renderMarginRevertIcon: false,
        glyphMargin: false,
        originalEditable: false,
        enableSplitViewResizing: true,
      });
    }
    return () => {
      const d = diffEditor.current;
      const f = fileEditor.current;
      const models = d ? [d.getModel()?.original, d.getModel()?.modified] : [f?.getModel()];
      d?.dispose();
      f?.dispose();
      models.forEach((m) => m?.dispose());
      diffEditor.current = null;
      fileEditor.current = null;
    };
  }, [monaco, fileOnly]);

  useEffect(() => {
    if (!monaco) return;
    if (diffEditor.current) {
      const prev = diffEditor.current.getModel();
      diffEditor.current.setModel({
        original: monaco.editor.createModel(original, language),
        modified: monaco.editor.createModel(modified, language),
      });
      prev?.original.dispose();
      prev?.modified.dispose();
      void diffEditor.current.revealFirstDiff();
    }
    if (fileEditor.current) {
      const prev = fileEditor.current.getModel();
      fileEditor.current.setModel(monaco.editor.createModel(modified, language));
      prev?.dispose();
    }
  }, [monaco, original, modified, language, fileOnly]);

  useEffect(() => {
    diffEditor.current?.updateOptions({ renderSideBySide: split });
  }, [split, monaco, fileOnly]);

  useEffect(() => {
    diffEditor.current?.getModifiedEditor().updateOptions({ ariaLabel: `${label}, new version` });
    diffEditor.current?.getOriginalEditor().updateOptions({ ariaLabel: `${label}, previous version` });
    fileEditor.current?.updateOptions({ ariaLabel: `${label}, new version` });
  }, [label, monaco, fileOnly]);

  useEffect(() => {
    monaco?.editor.setTheme(dark ? "agentdeck-dark" : "agentdeck-light");
  }, [monaco, dark]);

  useImperativeHandle(ref, () => ({
    next: () => diffEditor.current?.goToDiff("next"),
    previous: () => diffEditor.current?.goToDiff("previous"),
  }));

  if (failed) {
    return (
      <div className="flex h-full items-center justify-center px-8 text-center text-[13px] text-fg-2">
        The diff editor couldn't start: {failed}
      </div>
    );
  }
  return (
    <div className="relative h-full">
      {!monaco && <div className="absolute inset-0 flex items-center justify-center text-xs text-fg-3">Loading the diff editor…</div>}
      <div ref={host} className="h-full" />
    </div>
  );
});
