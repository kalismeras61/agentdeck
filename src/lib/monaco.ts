import type * as MonacoApi from "monaco-editor/editor/editor.api";

export type Monaco = typeof MonacoApi;

let loading: Promise<Monaco> | null = null;

/**
 * Monaco is bundled with the app (never fetched from a CDN) and loaded on first use,
 * so the workspace opens without paying for it. The diff worker is inlined as a blob.
 */
export function loadMonaco(): Promise<Monaco> {
  loading ??= (async () => {
    const [monaco, worker] = await Promise.all([import("monaco-editor/editor/editor.api"), import("monaco-editor/editor/editor.worker?worker&inline")]);
    (self as unknown as { MonacoEnvironment: unknown }).MonacoEnvironment = { getWorker: () => new worker.default() };
    await import("monaco-editor/languages/definitions/dart/register");
    defineThemes(monaco);
    return monaco;
  })();
  return loading;
}

function defineThemes(monaco: Monaco) {
  monaco.editor.defineTheme("agentdeck-dark", {
    base: "vs-dark",
    inherit: true,
    rules: [
      { token: "keyword", foreground: "C4B5FD" },
      { token: "string", foreground: "6EE7B7" },
      { token: "comment", foreground: "6B7287", fontStyle: "italic" },
      { token: "number", foreground: "FCD34D" },
      { token: "type", foreground: "93C5FD" },
      { token: "type.identifier", foreground: "93C5FD" },
      { token: "annotation", foreground: "FCD34D" },
    ],
    colors: {
      "editor.background": "#0B0D14",
      "editor.foreground": "#E4E6EF",
      "editorGutter.background": "#0B0D14",
      "editorLineNumber.foreground": "#5D6479",
      "editorLineNumber.activeForeground": "#9CA3B8",
      "editor.selectionBackground": "#8B5CF655",
      "editor.lineHighlightBackground": "#171A27",
      "editor.lineHighlightBorder": "#00000000",
      "diffEditor.insertedTextBackground": "#34D39933",
      "diffEditor.removedTextBackground": "#FB718533",
      "diffEditor.insertedLineBackground": "#34D39914",
      "diffEditor.removedLineBackground": "#FB718514",
      "diffEditorGutter.insertedLineBackground": "#34D39922",
      "diffEditorGutter.removedLineBackground": "#FB718522",
      "diffEditor.diagonalFill": "#2B304466",
      "diffEditor.border": "#2B3044",
      "scrollbarSlider.background": "#2B304488",
      "scrollbarSlider.hoverBackground": "#3A405888",
      "editorOverviewRuler.border": "#00000000",
      focusBorder: "#8B5CF6",
    },
  });
  monaco.editor.defineTheme("agentdeck-light", {
    base: "vs",
    inherit: true,
    rules: [
      { token: "keyword", foreground: "6D28D9" },
      { token: "string", foreground: "047857" },
      { token: "comment", foreground: "8A90A3", fontStyle: "italic" },
      { token: "number", foreground: "A16207" },
      { token: "type", foreground: "1D4ED8" },
      { token: "type.identifier", foreground: "1D4ED8" },
      { token: "annotation", foreground: "A16207" },
    ],
    colors: {
      "editor.background": "#FFFFFF",
      "editor.foreground": "#1F2333",
      "editorGutter.background": "#FFFFFF",
      "editorLineNumber.foreground": "#9AA0B2",
      "editorLineNumber.activeForeground": "#4F566B",
      "editor.selectionBackground": "#7C3AED33",
      "editor.lineHighlightBackground": "#F6F7FB",
      "diffEditor.insertedTextBackground": "#04785726",
      "diffEditor.removedTextBackground": "#BE123C22",
      "diffEditor.insertedLineBackground": "#04785712",
      "diffEditor.removedLineBackground": "#BE123C10",
      "diffEditor.diagonalFill": "#D9DCE866",
      "diffEditor.border": "#D9DCE8",
      "editorOverviewRuler.border": "#00000000",
      focusBorder: "#7C3AED",
    },
  });
}
