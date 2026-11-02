import { describe, expect, it } from "vitest";
import { signInNeed } from "../components/blocks/SignIn";

describe("signInNeed", () => {
  it("tells signing in apart from Google's personal-account block and ordinary errors", () => {
    expect(signInNeed("claude-code", "Not logged in · Please run /login")).toBe("sign_in");
    expect(signInNeed("codex", "unexpected status 401 Unauthorized")).toBe("sign_in");
    expect(signInNeed("gemini-cli", "The agent needs you to sign in. Authentication required")).toBe("sign_in");
    expect(signInNeed("gemini-cli", "This client is no longer supported for Gemini Code Assist for individuals.")).toBe("gemini_blocked");
    expect(signInNeed("codex", "Couldn't start Codex: No such file or directory")).toBe("none");
    expect(signInNeed("claude-code", "The agent returned an error: rate limited")).toBe("none");
    expect(signInNeed("api", "401 Unauthorized")).toBe("none");
  });
});
