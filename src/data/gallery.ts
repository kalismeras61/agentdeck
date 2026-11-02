/**
 * Samples for the Output gallery. Every item is raw, untrusted input that goes through the same
 * validation as provider output; some are malformed on purpose.
 */
import { loginPreviewSrc } from "./loginPreview";

const SID = "gallery";
let n = 0;
const T0 = Date.UTC(2026, 10, 1, 17, 0, 0);
function ev(type: string, payload: unknown, provider = "claude-code") {
  n += 1;
  return { eventId: `g-${n}`, sessionId: SID, ts: T0 + n * 1000, provider, type, payload };
}

export interface GallerySection {
  id: string;
  title: string;
  note: string;
  events: unknown[];
}

const deepJson = (() => {
  let o: Record<string, unknown> = { leaf: "bottom" };
  for (let i = 18; i > 0; i--) o = { [`level${i}`]: o };
  return o;
})();

const bigLog = Array.from({ length: 3200 }, (_, i) =>
  i % 250 === 0 ? `\u001b[33mwarn\u001b[0m  ${String(i).padStart(4, "0")} slow frame in BookingsScreen (24ms)` : `info  ${String(i).padStart(4, "0")} rebuilt BookingCard #${i % 40}`,
).join("\n");

export const gallerySections: GallerySection[] = [
  {
    id: "markdown",
    title: "Markdown",
    note: "Headings, lists, task lists, tables, quotes, inline code and fenced code with syntax colors.",
    events: [
      ev("assistant_text", {
        markdown: `# Login refactor summary

The form moved out of the screen, so widget tests no longer need a full app shell.

## What changed

- Validation lives in \`LoginValidators\`
- Loading state comes from \`LoginState\`
- The screen is now a \`ConsumerWidget\`

### Checklist

- [x] Extract \`LoginForm\`
- [x] Keep existing tests green
- [ ] Regenerate goldens

| Check | Before | After |
| --- | --- | --- |
| Empty email | Rejected | Rejected |
| Short password | Accepted | Rejected |
| Submit while busy | Enabled | Disabled |

> Goldens are skipped until they're regenerated with \`--update-goldens\`.

\`\`\`dart
class LoginState {
  const LoginState({this.isLoading = false, this.errorText});
  final bool isLoading;
  final String? errorText;
}
\`\`\`

\`\`\`bash
flutter test test/features/auth --reporter expanded
\`\`\`

\`\`\`yaml
dependencies:
  flutter_riverpod: ^3.0.0
\`\`\``,
      }),
    ],
  },
  {
    id: "untrusted",
    title: "Untrusted markdown",
    note: "Raw HTML stays text, remote images are not fetched, and only http(s) and mailto links become links.",
    events: [
      ev("assistant_text", {
        markdown: `A raw tag is shown as text: <script>alert("hi")</script> and <img src=x onerror=alert(1)>.

A remote image is not loaded: ![tracking pixel](https://example.com/pixel.png)

Links: [docs](https://code.claude.com/docs), [script link](javascript:alert(1)), [file link](file:///etc/passwd).`,
      }),
    ],
  },
  {
    id: "tools",
    title: "Tool activity and logs",
    note: "Grouped tool steps with status, ANSI colors, stripped control sequences, and a 3,200-line log that renders only its tail.",
    events: [
      ev("tool_group", {
        title: "Ran 3 commands",
        items: [
          {
            id: "a",
            kind: "command",
            label: "Ran",
            target: "flutter pub get",
            status: "ok",
            durationMs: 2100,
            output: "\u001b]0;evil window title\u0007Resolving dependencies...\n\u001b[32m+ flutter_riverpod 3.0.0\u001b[0m\n\u001b[2K\u001b[1GGot dependencies!",
          },
          { id: "b", kind: "test", label: "Ran", target: "flutter run --profile (log)", status: "warning", durationMs: 64000, meta: "13 warnings", output: bigLog },
          {
            id: "c",
            kind: "command",
            label: "Ran",
            target: "dart format --set-exit-if-changed .",
            status: "error",
            durationMs: 900,
            output: "\u001b[1m\u001b[31mChanged lib/features/auth/login_form.dart\u001b[0m\nFormatted 42 files (1 changed) in 0.31 seconds.",
          },
        ],
      }),
    ],
  },
  {
    id: "json",
    title: "JSON tree",
    note: "Collapsible, typed values. Nesting deeper than 12 levels and arrays past 200 items are cut off instead of freezing the view.",
    events: [
      ev("tool_group", {
        title: "Read analyzer output",
        items: [
          {
            id: "j",
            kind: "analyze",
            label: "Ran",
            target: "flutter analyze --format=json",
            status: "warning",
            meta: "1 info",
            json: {
              version: 1,
              ok: false,
              diagnostics: [{ code: "deprecated_member_use", severity: "INFO", location: { file: "lib/features/auth/widgets/login_form.dart", line: 58 } }],
              deep: deepJson,
              many: Array.from({ length: 260 }, (_, i) => i),
              nothing: null,
            },
          },
        ],
      }),
    ],
  },
  {
    id: "progress",
    title: "Progress",
    note: "Plans reported by the agent, including a failed step.",
    events: [
      ev("progress", {
        title: "Refactor login screen",
        steps: [
          { label: "Map validation and loading state", status: "done" },
          { label: "Extract LoginForm", status: "done" },
          { label: "Update widget tests", status: "active" },
          { label: "Regenerate goldens", status: "pending" },
        ],
      }),
      ev("progress", {
        title: "Release build",
        steps: [
          { label: "Bump version to 2.4.0", status: "done" },
          { label: "Build iOS archive", status: "failed" },
          { label: "Upload to TestFlight", status: "pending" },
        ],
      }),
    ],
  },
  {
    id: "tests",
    title: "Test summaries",
    note: "Passing and failing runs with per-case messages.",
    events: [
      ev("test_result", {
        command: "flutter test test/features/auth",
        passed: 14,
        failed: 0,
        skipped: 1,
        durationMs: 3800,
        cases: [
          { name: "LoginScreen shows validation errors on empty submit", status: "passed" },
          { name: "LoginScreen golden (light)", status: "skipped", message: "Run with --update-goldens to regenerate." },
        ],
      }),
      ev(
        "test_result",
        {
          command: "dart test",
          passed: 11,
          failed: 1,
          skipped: 0,
          durationMs: 6100,
          cases: [{ name: "rate limit returns 429 after 5 attempts", status: "failed", message: "Expected: <429>\n  Actual: <200>" }],
        },
        "codex",
      ),
    ],
  },
  {
    id: "images",
    title: "Images",
    note: "Inline images open full size with zoom. Remote URLs and script URLs are rejected before rendering.",
    events: [
      ev("image", { src: loginPreviewSrc, alt: "Rendered login screen", caption: "test/goldens/login_screen.png", width: 390, height: 844 }),
      ev("image", { src: "https://example.com/screenshot.png", alt: "Remote screenshot", width: 100, height: 100 }),
    ],
  },
  {
    id: "changes",
    title: "Code changes",
    note: "A changeset card with per-file counts and a highlighted preview.",
    events: [
      ev("changeset", {
        files: [
          {
            path: "lib/app/theme.dart",
            change: "modified",
            diff: "@@ -12,4 +12,6 @@\n   final colorScheme = ColorScheme.fromSeed(\n-    seedColor: Colors.teal,\n+    seedColor: const Color(0xFF0E6E73),\n+    brightness: Brightness.light,\n   );\n+  static const radius = 12.0;\n",
          },
        ],
      }),
    ],
  },
  {
    id: "permissions",
    title: "Decided permissions",
    note: "Resolved requests collapse to one line. Live requests are handled in Permissions.",
    events: [
      ev("permission", {
        requestId: "g-p1",
        tool: "Bash",
        command: "flutter test test/features/auth",
        cwd: "~/code/harbor/harbor_app",
        paths: ["test/features/auth/"],
        risk: "low",
        reason: "Runs tests.",
        supportsPersistentAllow: false,
        state: "allowed",
      }),
      ev(
        "permission",
        {
          requestId: "g-p2",
          tool: "apply_patch",
          command: "Edit lib/app/theme.dart",
          cwd: "~/code/harbor/harbor_app",
          paths: ["lib/app/theme.dart"],
          risk: "low",
          reason: "One file.",
          supportsPersistentAllow: true,
          state: "allowed_session",
        },
        "codex",
      ),
      ev("permission", {
        requestId: "g-p3",
        tool: "Bash",
        command: "rm -rf build/ ios/Pods",
        cwd: "~/code/harbor/harbor_app",
        paths: ["build/", "ios/Pods/"],
        risk: "high",
        reason: "Deletes build output and CocoaPods.",
        supportsPersistentAllow: false,
        state: "denied",
      }),
    ],
  },
  {
    id: "errors",
    title: "Errors and malformed events",
    note: "Provider errors with detail, and events that fail validation: an unknown type, missing fields, a path outside the project, and non-object input.",
    events: [
      ev(
        "error",
        {
          title: "Session stopped",
          message: "The agent process exited with code 1 before finishing the task.",
          detail: "codex: turn aborted after test failure; no retry configured",
        },
        "codex",
      ),
      ev("tool_call_v9", { anything: true }),
      ev("test_result", { command: "flutter test", passed: "fourteen" }),
      ev("changeset", { files: [{ path: "../../.ssh/id_ed25519", change: "modified", diff: "" }] }),
      "not an event",
    ],
  },
];
