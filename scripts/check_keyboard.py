"""Keyboard-only and reduced-motion checks against the running dev server (npm run dev).

Usage: python3 scripts/check_keyboard.py [url]
Tabs through every view, records each stop, and fails if a focused control has no visible
focus indicator or if anything keeps animating with reduced motion on.
"""
import asyncio, json, sys
from playwright.async_api import async_playwright

URL = sys.argv[1] if len(sys.argv) > 1 else "http://localhost:1420/"
VIEWS = [("Workspace", None), ("All sessions", "Meta+Shift+s"), ("Review changes", "Meta+Shift+d"), ("Permissions", "Meta+Shift+a"), ("Output gallery", "Meta+Shift+g"), ("Settings", "Meta+,")]

FOCUS_PROBE = """() => {
  const el = document.activeElement;
  if (!el || el === document.body) return null;
  const cs = getComputedStyle(el);
  const visible = (cs.outlineStyle !== 'none' && parseFloat(cs.outlineWidth) > 0) || cs.boxShadow !== 'none';
  const name = el.getAttribute('aria-label') || el.textContent.trim().slice(0, 40) || el.tagName;
  return { name, tag: el.tagName, role: el.getAttribute('role'), visible };
}"""

async def main():
    report, failures = {}, []
    async with async_playwright() as p:
        b = await p.chromium.launch()
        pg = await b.new_page(viewport={"width": 1440, "height": 900}, reduced_motion="reduce")
        await pg.add_init_script("Object.defineProperty(navigator,'platform',{get:()=>'MacIntel'})")
        await pg.goto(URL)
        await pg.wait_for_timeout(1500)
        for view, keys in VIEWS:
            if keys:
                await pg.keyboard.press("Escape")
                await pg.keyboard.press(keys)
                await pg.wait_for_timeout(1500)
            await pg.evaluate("document.activeElement && document.activeElement.blur()")
            stops = []
            for _ in range(40):
                await pg.keyboard.press("Tab")
                info = await pg.evaluate(FOCUS_PROBE)
                if info is None:
                    continue
                stops.append(info)
                if not info["visible"] and info["tag"] != "TEXTAREA":
                    failures.append(f"{view}: no focus indicator on {info['tag']} '{info['name']}'")
            report[view] = [s["name"] for s in stops]
        running = await pg.evaluate("document.getAnimations().filter(a => a.playState === 'running' && a.effect && a.effect.getTiming().iterations === Infinity).length")
        if running:
            failures.append(f"reduced motion: {running} looping animations still running")
        await b.close()
    print(json.dumps({"stopsPerView": {k: len(v) for k, v in report.items()}, "sample": {k: v[:8] for k, v in report.items()}, "failures": failures}, indent=2))
    sys.exit(1 if failures else 0)

asyncio.run(main())
