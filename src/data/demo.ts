/**
 * DEMO DATA — clearly labeled in the UI. Nothing here came from a live provider.
 * Events are written in the normalized format and go through the same validation as live output.
 */
import type { ProviderId } from "../lib/events";
import type { Bot, BotTask } from "../lib/live";
import type { PlanLimits } from "../lib/limits";
import { loginPreviewSrc } from "./loginPreview";

export type SessionStatus = "running" | "waiting" | "idle" | "error" | "done";

export interface Project {
  id: string;
  name: string;
  path: string;
  workspaceId: WorkspaceId;
  /** "chats": sessions without a project. */
  kind?: "folder" | "chats";
}

export type WorkspaceId = "local" | "demo";

export interface Session {
  id: string;
  projectId: string;
  title: string;
  provider: ProviderId;
  model: string;
  status: SessionStatus;
  snippet: string;
  startedAt: number;
  updatedAt: number;
  raw: unknown[];
  demo: boolean;
  /** Backed by a real provider process and the local store. */
  live?: boolean;
  /** "plan": the agent may read but AgentDeck refuses changes. */
  mode?: "default" | "plan";
  /** Custom model profile (DeepSeek, Kimi, a local model, …) the session runs on. */
  profileId?: string | null;
  /** The git worktree it works in (instead of the project folder), and its branch. */
  worktree?: string | null;
  branch?: string | null;
  /** Reasoning effort picked for the session; null/absent = the agent's default. */
  effort?: string | null;
}

export interface Workspace {
  id: WorkspaceId;
  name: string;
}

export const workspaces: Workspace[] = /* i18n */ [
  { id: "local", name: "This Mac" },
  { id: "demo", name: "Demo" },
];

export const demoProjects: Project[] = [
  { id: "p-app", name: "harbor_app", path: "~/code/harbor/harbor_app", workspaceId: "demo" },
  { id: "p-api", name: "harbor_api", path: "~/code/harbor/harbor_api", workspaceId: "demo" },
  { id: "p-docs", name: "harbor_docs", path: "~/code/harbor/harbor_docs", workspaceId: "demo" },
];

const NOW = Date.now();
/** `at(m, s)` is m minutes and s seconds ago. */
const at = (minAgo: number, sec = 0) => NOW - minAgo * 60_000 - sec * 1000;

let seq = 0;
function ev(sessionId: string, provider: ProviderId, ts: number, type: string, payload: unknown) {
  seq += 1;
  return { eventId: `${sessionId}-e${seq}`, sessionId, ts, provider, type, payload };
}

/* ---------------------------------------------------------------- diffs */

const loginScreenDiff = `@@ -1,40 +1,26 @@
 import 'package:flutter/material.dart';
 import 'package:flutter_riverpod/flutter_riverpod.dart';
-import '../../core/validators.dart';
+import 'login_controller.dart';
+import 'widgets/login_form.dart';

-class LoginScreen extends StatefulWidget {
+class LoginScreen extends ConsumerWidget {
   const LoginScreen({super.key});

-  @override
-  State<LoginScreen> createState() => _LoginScreenState();
-}
-
-class _LoginScreenState extends State<LoginScreen> {
-  final _formKey = GlobalKey<FormState>();
-  final _email = TextEditingController();
-  final _password = TextEditingController();
-  bool _loading = false;
-
   @override
-  Widget build(BuildContext context) {
+  Widget build(BuildContext context, WidgetRef ref) {
+    final state = ref.watch(loginControllerProvider);
     return Scaffold(
       body: SafeArea(
-        child: Padding(
-          padding: const EdgeInsets.all(24),
-          child: Form(
-            key: _formKey,
-            child: Column(
-              children: [
-                TextFormField(controller: _email, validator: Validators.email),
-                TextFormField(controller: _password, obscureText: true),
-                ElevatedButton(
-                  onPressed: _loading ? null : _submit,
-                  child: const Text('Sign in'),
-                ),
-              ],
-            ),
+        child: Center(
+          child: ConstrainedBox(
+            constraints: const BoxConstraints(maxWidth: 420),
+            child: LoginForm(
+              isLoading: state.isLoading,
+              errorText: state.errorText,
+              onSubmit: ref.read(loginControllerProvider.notifier).submit,
+            ),
           ),
         ),
       ),
     );
   }
 }
`;

const loginFormSource = `import 'package:flutter/material.dart';

import '../login_controller.dart' show LoginValidators;

class LoginForm extends StatefulWidget {
  const LoginForm({
    super.key,
    required this.onSubmit,
    this.isLoading = false,
    this.errorText,
  });

  final Future<void> Function(String email, String password) onSubmit;
  final bool isLoading;
  final String? errorText;

  @override
  State<LoginForm> createState() => _LoginFormState();
}

class _LoginFormState extends State<LoginForm> {
  final _formKey = GlobalKey<FormState>();
  final _email = TextEditingController();
  final _password = TextEditingController();

  @override
  void dispose() {
    _email.dispose();
    _password.dispose();
    super.dispose();
  }

  Future<void> _submit() async {
    if (!_formKey.currentState!.validate()) return;
    await widget.onSubmit(_email.text.trim(), _password.text);
  }

  @override
  Widget build(BuildContext context) {
    return Form(
      key: _formKey,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          TextFormField(
            controller: _email,
            keyboardType: TextInputType.emailAddress,
            autofillHints: const [AutofillHints.email],
            decoration: const InputDecoration(labelText: 'Email'),
            validator: LoginValidators.email,
          ),
          const SizedBox(height: 16),
          TextFormField(
            controller: _password,
            obscureText: true,
            autofillHints: const [AutofillHints.password],
            decoration: const InputDecoration(labelText: 'Password'),
            validator: LoginValidators.password,
          ),
          if (widget.errorText != null) ...[
            const SizedBox(height: 12),
            Text(
              widget.errorText!,
              style: TextStyle(color: Theme.of(context).colorScheme.error),
            ),
          ],
          const SizedBox(height: 24),
          FilledButton(
            onPressed: widget.isLoading ? null : _submit,
            child: widget.isLoading
                ? const SizedBox.square(
                    dimension: 18,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  )
                : const Text('Sign in'),
          ),
        ],
      ),
    );
  }
}`;
const loginFormLines = loginFormSource.split("\n");
const loginFormDiff = `@@ -0,0 +1,${loginFormLines.length} @@\n${loginFormLines.map((l) => `+${l}`).join("\n")}\n`;

const controllerDiff = `@@ -1,20 +1,44 @@
 import 'package:flutter_riverpod/flutter_riverpod.dart';
 import '../../data/auth_repository.dart';

-class LoginController extends StateNotifier<AsyncValue<void>> {
-  LoginController(this._repo) : super(const AsyncData(null));
-  final AuthRepository _repo;
+class LoginState {
+  const LoginState({this.isLoading = false, this.errorText});
+  final bool isLoading;
+  final String? errorText;
+}
+
+abstract final class LoginValidators {
+  static String? email(String? value) {
+    final v = value?.trim() ?? '';
+    if (v.isEmpty) return 'Enter your email';
+    if (!v.contains('@')) return 'Enter a valid email';
+    return null;
+  }
+
+  static String? password(String? value) =>
+      (value == null || value.length < 8) ? 'Use at least 8 characters' : null;
+}
+
+class LoginController extends Notifier<LoginState> {
+  @override
+  LoginState build() => const LoginState();

-  Future<void> login(String email, String password) async {
-    state = const AsyncLoading();
-    state = await AsyncValue.guard(() => _repo.signIn(email, password));
+  Future<void> submit(String email, String password) async {
+    state = const LoginState(isLoading: true);
+    try {
+      await ref.read(authRepositoryProvider).signIn(email, password);
+      state = const LoginState();
+    } on AuthException catch (e) {
+      state = LoginState(errorText: e.message);
+    }
   }
 }

-final loginControllerProvider =
-    StateNotifierProvider<LoginController, AsyncValue<void>>(
-  (ref) => LoginController(ref.watch(authRepositoryProvider)),
-);
+final loginControllerProvider =
+    NotifierProvider<LoginController, LoginState>(LoginController.new);
`;

const testDiff = `@@ -1,21 +1,36 @@
 import 'package:flutter/material.dart';
 import 'package:flutter_riverpod/flutter_riverpod.dart';
 import 'package:flutter_test/flutter_test.dart';
 import 'package:harbor_app/data/auth_repository.dart';
 import 'package:harbor_app/features/auth/login_screen.dart';
 
 void main() {
   testWidgets('shows validation errors on empty submit', (tester) async {
-    await tester.pumpWidget(const MaterialApp(home: LoginScreen()));
+    await tester.pumpWidget(
+      const ProviderScope(child: MaterialApp(home: LoginScreen())),
+    );
     await tester.tap(find.text('Sign in'));
     await tester.pump();
     expect(find.text('Enter your email'), findsOneWidget);
   });
+
+  testWidgets('disables the button while signing in', (tester) async {
+    await tester.pumpWidget(
+      ProviderScope(
+        overrides: [authRepositoryProvider.overrideWithValue(SlowAuthRepository())],
+        child: const MaterialApp(home: LoginScreen()),
+      ),
+    );
+    await tester.enterText(find.byType(TextFormField).first, 'ada@harbor.dev');
+    await tester.enterText(find.byType(TextFormField).last, 'correct-horse');
+    await tester.tap(find.text('Sign in'));
+    await tester.pump();
+    final button = tester.widget<FilledButton>(find.byType(FilledButton));
+    expect(button.onPressed, isNull);
+  });
 }
`;

export const demoProjectFiles: string[] = [
  "lib/main.dart",
  "lib/app/router.dart",
  "lib/app/theme.dart",
  "lib/core/validators.dart",
  "lib/data/auth_repository.dart",
  "lib/features/auth/login_controller.dart",
  "lib/features/auth/login_screen.dart",
  "lib/features/auth/widgets/login_form.dart",
  "lib/features/bookings/bookings_screen.dart",
  "lib/features/bookings/booking_card.dart",
  "test/features/auth/login_screen_test.dart",
  "test/goldens/login_screen.png",
  "test/features/bookings/bookings_screen_test.dart",
  "analysis_options.yaml",
  "pubspec.yaml",
];

/* ------------------------------------------------------------- sessions */

const S1 = "s-login";
const s1Events = [
  ev(S1, "claude-code", at(38), "user_message", {
    text: "Refactor the login screen: pull the form into its own widget, move validation into the controller, and keep the existing tests green. Add a test for the loading state.",
  }),
  ev(S1, "claude-code", at(37, 52), "assistant_text", {
    markdown:
      "I'll start by reading the current screen, controller and tests so the refactor keeps behavior identical.\n\n- [x] Map where validation and loading state live today\n- [x] Extract `LoginForm` into `widgets/`\n- [x] Move validators into the controller layer\n- [x] Update and extend widget tests\n- [ ] Regenerate code for the new providers",
  }),
  ev(S1, "claude-code", at(37, 20), "tool_group", {
    title: "Explored 5 files",
    items: [
      { id: "t1", kind: "read", label: "Read", target: "lib/features/auth/login_screen.dart", status: "ok", meta: "112 lines", durationMs: 40 },
      { id: "t2", kind: "read", label: "Read", target: "lib/features/auth/login_controller.dart", status: "ok", meta: "38 lines", durationMs: 31 },
      { id: "t3", kind: "search", label: "Searched", target: "Validators\\.", status: "ok", meta: "9 matches in 4 files", durationMs: 88 },
      { id: "t4", kind: "read", label: "Read", target: "test/features/auth/login_screen_test.dart", status: "ok", meta: "61 lines", durationMs: 29 },
      { id: "t5", kind: "read", label: "Read", target: "pubspec.yaml", status: "ok", meta: "flutter_riverpod 2.6", durationMs: 22 },
    ],
  }),
  ev(S1, "claude-code", at(35, 10), "assistant_text", {
    markdown:
      "The screen currently owns form state, validation and the loading flag, which is why the tests need a full `MaterialApp` to check anything. Here is how responsibility shifts:\n\n| Concern | Before | After |\n| --- | --- | --- |\n| Form fields and keys | `_LoginScreenState` | `LoginForm` widget |\n| Validation rules | `core/validators.dart` | `LoginValidators` |\n| Loading and errors | local `bool _loading` | `LoginState` via Riverpod |\n| Submit | `setState` + repository | `LoginController.submit` |\n\n`core/validators.dart` is still used by the signup flow, so I'm leaving it in place.",
  }),
  ev(S1, "claude-code", at(31, 4), "tool_group", {
    title: "Edited 4 files",
    items: [
      { id: "t6", kind: "edit", label: "Edited", target: "lib/features/auth/login_screen.dart", status: "ok" },
      { id: "t7", kind: "create", label: "Created", target: "lib/features/auth/widgets/login_form.dart", status: "ok", meta: "new file" },
      { id: "t8", kind: "edit", label: "Edited", target: "lib/features/auth/login_controller.dart", status: "ok" },
      { id: "t9", kind: "edit", label: "Edited", target: "test/features/auth/login_screen_test.dart", status: "ok" },
    ],
  }),
  ev(S1, "claude-code", at(31, 1), "changeset", {
    previewPath: "lib/features/auth/widgets/login_form.dart",
    files: [
      { path: "lib/features/auth/login_screen.dart", change: "modified", diff: loginScreenDiff },
      { path: "lib/features/auth/widgets/login_form.dart", change: "added", diff: loginFormDiff },
      { path: "lib/features/auth/login_controller.dart", change: "modified", diff: controllerDiff },
      { path: "test/features/auth/login_screen_test.dart", change: "modified", diff: testDiff },
    ],
  }),
  ev(S1, "claude-code", at(29, 40), "tool_group", {
    title: "Checked the project",
    items: [
      {
        id: "t10",
        kind: "analyze",
        label: "Ran",
        target: "flutter analyze --format=json lib/features/auth",
        status: "warning",
        meta: "1 info",
        durationMs: 4200,
        json: {
          version: 1,
          diagnostics: [
            {
              code: "deprecated_member_use",
              severity: "INFO",
              location: { file: "lib/features/auth/widgets/login_form.dart", line: 58, column: 22 },
              problemMessage: "'withOpacity' is deprecated. Use .withValues() to avoid precision loss.",
            },
          ],
        },
      },
      {
        id: "t11",
        kind: "test",
        label: "Ran",
        target: "flutter test test/features/auth",
        status: "ok",
        durationMs: 9800,
        output:
          "\u001b[32m00:03 +14 ~1: All tests passed!\u001b[0m\n00:01 +3: LoginScreen shows validation errors on empty submit\n00:02 +9: LoginScreen disables the button while signing in\n00:03 +14 ~1: Skipped: golden test (run with --update-goldens)",
      },
    ],
  }),
  ev(S1, "claude-code", at(29, 20), "test_result", {
    command: "flutter test test/features/auth",
    passed: 14,
    failed: 0,
    skipped: 1,
    durationMs: 3800,
    cases: [
      { name: "LoginScreen shows validation errors on empty submit", status: "passed" },
      { name: "LoginScreen disables the button while signing in", status: "passed" },
      { name: "LoginScreen shows repository error text", status: "passed" },
      { name: "LoginValidators.email rejects missing @", status: "passed" },
      { name: "LoginScreen golden (light)", status: "skipped", message: "Run with --update-goldens to regenerate." },
    ],
  }),
  ev(S1, "claude-code", at(28, 30), "assistant_text", {
    markdown:
      "All 14 auth tests pass, including the new loading-state test. The one skipped case is the golden, which needs regenerating because the layout now centers at a 420px max width. Here's the rendered screen from the updated golden harness:",
  }),
  ev(S1, "claude-code", at(28, 20), "image", {
    src: loginPreviewSrc,
    alt: "Rendered login screen with email and password fields and a Sign in button",
    caption: "test/goldens/login_screen.png",
    width: 390,
    height: 844,
  }),
  ev(S1, "claude-code", at(27, 50), "assistant_text", {
    markdown:
      "`login_controller.dart` now uses a `Notifier`, so the generated provider files are stale. I need to regenerate them before the analyzer is fully clean.",
  }),
  ev(S1, "claude-code", at(27, 40), "permission", {
    requestId: "perm-1",
    tool: "Bash",
    command: "dart run build_runner build --delete-conflicting-outputs",
    cwd: "~/code/harbor/harbor_app",
    paths: ["lib/**/*.g.dart", "lib/**/*.freezed.dart", ".dart_tool/build/"],
    risk: "medium",
    reason: "Rewrites every generated *.g.dart and *.freezed.dart file under lib/. Hand edits in generated files would be lost.",
    supportsPersistentAllow: false,
    state: "pending",
  }),
];

const S2 = "s-keyboard";
const s2Events = [
  ev(S2, "codex", at(95), "user_message", {
    text: "On iOS the keyboard covers the Sign in button on small phones. Find out why.",
  }),
  ev(S2, "codex", at(94), "tool_group", {
    title: "Explored 3 files",
    items: [
      { id: "k1", kind: "read", label: "Read", target: "lib/features/auth/login_screen.dart", status: "ok" },
      { id: "k2", kind: "search", label: "Searched", target: "resizeToAvoidBottomInset", status: "ok", meta: "2 matches" },
      { id: "k3", kind: "read", label: "Read", target: "lib/app/theme.dart", status: "ok" },
    ],
  }),
  ev(S2, "codex", at(92), "assistant_text", {
    markdown:
      "The form sits in a plain `Column` with no scroll view, so when the keyboard opens there is nowhere for the button to go. Wrapping the form in a `SingleChildScrollView` with `keyboardDismissBehavior: onDrag` fixes it.\n\nI'll hold off on editing until the login refactor in the other session lands, since both touch `login_screen.dart`.",
  }),
  ev(S2, "codex", at(24), "user_message", { text: "The refactor is in. Go ahead with the scroll fix." }),
  ev(S2, "codex", at(23, 30), "tool_group", {
    title: "Explored 2 files",
    items: [
      { id: "k4", kind: "read", label: "Read", target: "lib/features/auth/login_screen.dart", status: "ok", meta: "45 lines" },
      { id: "k5", kind: "read", label: "Read", target: "lib/features/auth/widgets/login_form.dart", status: "ok", meta: "84 lines" },
    ],
  }),
  ev(S2, "codex", at(23), "permission", {
    requestId: "perm-2",
    tool: "apply_patch",
    command: "Edit lib/features/auth/login_screen.dart",
    cwd: "~/code/harbor/harbor_app",
    paths: ["lib/features/auth/login_screen.dart"],
    risk: "low",
    reason: "Changes one file inside the project. The edit is shown above and can be reverted with git.",
    diff: `@@ -14,10 +14,13 @@
     return Scaffold(
       body: SafeArea(
-        child: Center(
-          child: ConstrainedBox(
-            constraints: const BoxConstraints(maxWidth: 420),
-            child: LoginForm(
+        child: SingleChildScrollView(
+          keyboardDismissBehavior: ScrollViewKeyboardDismissBehavior.onDrag,
+          padding: const EdgeInsets.all(24),
+          child: Center(
+            child: ConstrainedBox(
+              constraints: const BoxConstraints(maxWidth: 420),
+              child: LoginForm(
`,
    supportsPersistentAllow: true,
    state: "pending",
  }),
];

const S3 = "s-ratelimit";
const s3Events = [
  ev(S3, "codex", at(54), "user_message", { text: "Add per-IP rate limiting to POST /auth/login. 5 attempts per minute." }),
  ev(S3, "codex", at(53), "tool_group", {
    title: "Ran 2 commands",
    items: [
      { id: "r1", kind: "edit", label: "Edited", target: "bin/server.dart", status: "ok" },
      {
        id: "r2",
        kind: "command",
        label: "Ran",
        target: "dart test",
        status: "error",
        durationMs: 6100,
        output:
          "00:02 +11 -1: test/auth_test.dart: rate limit returns 429 after 5 attempts [E]\n  Expected: <429>\n    Actual: <200>\n\u001b[31mSome tests failed.\u001b[0m",
      },
    ],
  }),
  ev(S3, "codex", at(52, 30), "error", {
    title: "Session stopped",
    message: "The agent process exited with code 1 before finishing the task.",
    detail: "codex: turn aborted after test failure; no retry configured",
  }),
  // Deliberately malformed to show how invalid provider output is contained.
  { eventId: "s-ratelimit-bad", sessionId: S3, ts: at(52, 31), provider: "codex", type: "image", payload: { src: "javascript:alert(1)", alt: "x", width: 1, height: 1 } },
];

const S4 = "s-riverpod";
const s4Events = [
  ev(S4, "claude-code", at(190), "user_message", { text: "Upgrade the app to Riverpod 3 and fix any breaking changes." }),
  ev(S4, "claude-code", at(170), "assistant_text", {
    markdown:
      "Done. `flutter_riverpod` is on 3.x, all `StateNotifierProvider` uses outside auth are migrated, and `flutter test` passes (128 tests). Auth was left alone because it's being refactored in another session.",
  }),
];

const S5 = "s-guide";
const s5Events = [
  ev(S5, "claude-code", at(240), "user_message", { text: "Draft a migration guide for partners moving from API v1 to v2." }),
  ev(S5, "claude-code", at(236), "assistant_text", {
    markdown:
      "## Draft outline\n\n1. What changes in v2\n2. Authentication: API keys become scoped tokens\n3. Endpoint mapping table\n4. Timeline and deprecation dates\n\nTell me the v1 shutdown date and I'll fill in section 4.",
  }),
];

/* ---------------------------------------------------------------- bots */

const SB = "s-bot-reviewer";
const sbEvents = [
  ev(SB, "claude-code", at(42), "user_message", { text: "Review the rate limit change in auth/login.ts", label: "Task" }),
  ev(SB, "claude-code", at(38), "assistant_text", {
    markdown:
      "Reviewed the rate limit on `/auth/login`.\n\n- The limiter keys on IP only, so one office behind NAT shares a budget. Suggest IP + account.\n- `429` responses don't set `Retry-After`.\n\nLeft: no test for the reset window. Added both notes to my memory file.",
  }),
  ev(SB, "claude-code", at(3), "user_message", { text: "Check the new /auth/refresh route for missing tests", label: "Task" }),
  ev(SB, "claude-code", at(2), "assistant_text", { markdown: "Reading `routes/auth/refresh.ts` and its tests." }),
];

export const demoBotSession: Session = {
  id: SB,
  projectId: "p-api",
  title: "API Reviewer",
  provider: "claude-code",
  model: "Sonnet 5.5",
  status: "running",
  snippet: "Reading routes/auth/refresh.ts",
  startedAt: at(42),
  updatedAt: at(2),
  raw: sbEvents,
  demo: true,
};

export const demoBots: Bot[] = [
  {
    id: "b-demo-reviewer",
    name: "API Reviewer",
    role: "Reviews changes in harbor_api before they merge",
    instructions: "Point out bugs and missing tests. Don't change code yourself.",
    provider: "claude-code",
    model: "sonnet",
    effort: null,
    profileId: null,
    projectId: "p-api",
    sessionId: SB,
    color: "blue",
    paused: false,
    createdAt: at(600),
    updatedAt: at(2),
    queued: 2,
    runningTaskId: "t-demo-3",
    runningText: "Check the new /auth/refresh route for missing tests",
    needsYou: false,
  },
  {
    id: "b-demo-docs",
    name: "Docs Writer",
    role: "Keeps harbor_docs in step with the API",
    instructions: "",
    provider: "codex",
    model: "default",
    effort: null,
    profileId: null,
    projectId: "p-docs",
    sessionId: null,
    color: "green",
    paused: true,
    createdAt: at(900),
    updatedAt: at(120),
    queued: 1,
    runningTaskId: null,
    runningText: null,
    needsYou: false,
  },
];

const task = (id: string, botId: string, text: string, status: BotTask["status"], created: number, extra: Partial<BotTask> = {}): BotTask => ({
  id,
  botId,
  text,
  source: "you",
  status,
  result: null,
  error: null,
  createdAt: created,
  startedAt: null,
  finishedAt: null,
  parentId: null,
  ...extra,
});

export const demoBotTasks: Record<string, BotTask[]> = {
  "b-demo-reviewer": [
    task("t-demo-3", "b-demo-reviewer", "Check the new /auth/refresh route for missing tests", "running", at(5), { startedAt: at(3) }),
    task("t-demo-4", "b-demo-reviewer", "Review the CORS settings in server.ts", "queued", at(4)),
    task("t-demo-5", "b-demo-reviewer", "Look for N+1 queries in the orders endpoints", "queued", at(1)),
    task("t-demo-1", "b-demo-reviewer", "Review the rate limit change in auth/login.ts", "done", at(44), {
      startedAt: at(42),
      finishedAt: at(38),
      result:
        "Reviewed the rate limit on `/auth/login`.\n\n- The limiter keys on IP only, so one office behind NAT shares a budget. Suggest IP + account.\n- `429` responses don't set `Retry-After`.\n\nLeft: no test for the reset window.",
    }),
    task("t-demo-2", "b-demo-reviewer", "Review the migration that adds sessions.expires_at", "failed", at(80), {
      startedAt: at(79),
      finishedAt: at(77),
      error: "the turn ended with an error",
    }),
  ],
  "b-demo-docs": [task("t-demo-6", "b-demo-docs", "Document the /auth/refresh route", "queued", at(120))],
};

export const demoBotMemory: Record<string, string> = {
  "b-demo-reviewer":
    "# API Reviewer memory\n\n- Tests: `npm test`; integration tests need `docker compose up db`.\n- Rate limits live in `src/limits.ts`; the team wants IP + account keys.\n- The user prefers short reviews: bugs first, style last.\n",
  "b-demo-docs": "# Docs Writer memory\n\nWhat Docs Writer keeps for later tasks.\n",
};

export const demoSessions: Session[] = [
  {
    id: S1,
    projectId: "p-app",
    title: "Refactor login screen",
    provider: "claude-code",
    model: "Sonnet 5.5",
    status: "waiting",
    snippet: "Wants to run build_runner",
    startedAt: at(38),
    updatedAt: at(27, 40),
    raw: s1Events,
    demo: true,
  },
  {
    id: S2,
    projectId: "p-app",
    title: "Keyboard covers Sign in on iOS",
    provider: "codex",
    model: "gpt-5-codex",
    status: "waiting",
    snippet: "Wants to edit login_screen.dart",
    startedAt: at(95),
    updatedAt: at(23),
    raw: s2Events,
    demo: true,
  },
  {
    id: S4,
    projectId: "p-app",
    title: "Upgrade to Riverpod 3",
    provider: "claude-code",
    model: "Sonnet 5.5",
    status: "done",
    snippet: "128 tests passing",
    startedAt: at(190),
    updatedAt: at(170),
    raw: s4Events,
    demo: true,
  },
  {
    id: S3,
    projectId: "p-api",
    title: "Rate limit /auth/login",
    provider: "codex",
    model: "gpt-5-codex",
    status: "error",
    snippet: "Exited with code 1",
    startedAt: at(54),
    updatedAt: at(52, 30),
    raw: s3Events,
    demo: true,
  },
  {
    id: S5,
    projectId: "p-docs",
    title: "API v2 migration guide",
    provider: "claude-code",
    model: "Opus 5.5",
    status: "idle",
    snippet: "Needs the v1 shutdown date",
    startedAt: at(240),
    updatedAt: at(236),
    raw: s5Events,
    demo: true,
  },
  demoBotSession,
];

export const providerModels: Record<ProviderId, string[]> = {
  "claude-code": ["Sonnet 5.5", "Opus 5.5", "Haiku 5.5"],
  codex: ["gpt-5-codex"],
  "gemini-cli": [],
  "grok-build": [],
  "kimi-code": [],
  antigravity: [],
  api: [],
};

/** Model choices passed to the real CLIs. "default" lets the CLI use its own configured model. */
export const liveModels: [string, string][] = [
  ["default", "Default model"],
  ["sonnet", "Sonnet"],
  ["opus", "Opus"],
  ["haiku", "Haiku"],
];

export const liveModelsFor = (provider: ProviderId): [string, string][] => (provider === "claude-code" ? liveModels : [["default", "Default model"]]);

export const providerLabel: Record<ProviderId, string> = {
  "claude-code": "Claude Code",
  codex: "Codex",
  "gemini-cli": "Gemini CLI",
  "grok-build": "Grok Build",
  "kimi-code": "Kimi Code",
  antigravity: "Antigravity",
  api: "Model API",
};

/** Sample plan limits for the demo's Claude Code and Codex sessions (made up, relative to now). */
export function sampleLimits(provider: ProviderId, now: number): PlanLimits | undefined {
  const min = 60_000;
  const read = now - 3 * min;
  if (provider === "claude-code")
    return {
      provider,
      updatedAt: read,
      windows: [
        { kind: "five_hour", usedPercent: 34, resetsAt: now + 2 * 60 * min + 15 * min, status: "allowed" },
        { kind: "weekly", usedPercent: 61, resetsAt: now + 3 * 1440 * min + 5 * 60 * min, status: "allowed" },
      ],
    };
  if (provider === "codex")
    return {
      provider,
      updatedAt: read,
      windows: [
        { kind: "five_hour", usedPercent: 86, resetsAt: now + 48 * min },
        { kind: "weekly", usedPercent: 42, resetsAt: now + 5 * 1440 * min },
      ],
    };
  return undefined;
}

/** Agents that can run as live sessions, in menu order. */
export const liveProviders: ProviderId[] = ["claude-code", "codex", "gemini-cli", "grok-build", "kimi-code", "antigravity"];
