// Demo artifact: an SVG stand-in for the golden screenshot an agent would attach after a UI test.
const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="390" height="844" viewBox="0 0 390 844">
<defs>
  <linearGradient id="g" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#F5F7FB"/><stop offset="1" stop-color="#EEF1F7"/></linearGradient>
</defs>
<rect width="390" height="844" fill="url(#g)"/>
<g font-family="-apple-system, Roboto, Segoe UI, Helvetica, Arial, sans-serif">
  <text x="28" y="34" font-size="15" font-weight="600" fill="#1B1F2A">9:41</text>
  <rect x="330" y="23" width="26" height="12" rx="3.5" fill="none" stroke="#1B1F2A" stroke-width="1.4"/>
  <rect x="332.5" y="25.5" width="18" height="7" rx="2" fill="#1B1F2A"/>
  <circle cx="195" cy="196" r="34" fill="#0E6E73"/>
  <path d="M195 178v34M183 196c0 9 5 16 12 16s12-7 12-16M188 184h14" stroke="#fff" stroke-width="3.4" fill="none" stroke-linecap="round"/>
  <text x="195" y="276" text-anchor="middle" font-size="28" font-weight="700" fill="#141824">Welcome back</text>
  <text x="195" y="304" text-anchor="middle" font-size="15" fill="#5B6275">Sign in to manage your harbor bookings</text>
  <rect x="33" y="352" width="324" height="58" rx="12" fill="#fff" stroke="#C9CFDB" stroke-width="1.2"/>
  <text x="50" y="376" font-size="12" fill="#5B6275">Email</text>
  <text x="50" y="398" font-size="16" fill="#141824">ada@harbor.dev</text>
  <rect x="33" y="426" width="324" height="58" rx="12" fill="#fff" stroke="#0E6E73" stroke-width="2"/>
  <text x="50" y="450" font-size="12" fill="#0E6E73">Password</text>
  <text x="50" y="473" font-size="18" letter-spacing="3" fill="#141824">••••••••••••</text>
  <rect x="33" y="512" width="324" height="52" rx="26" fill="#0E6E73"/>
  <text x="195" y="543" text-anchor="middle" font-size="16" font-weight="600" fill="#fff">Sign in</text>
  <text x="195" y="602" text-anchor="middle" font-size="15" font-weight="500" fill="#0E6E73">Forgot password?</text>
  <text x="195" y="790" text-anchor="middle" font-size="14" fill="#5B6275">New here? <tspan fill="#0E6E73" font-weight="600">Create an account</tspan></text>
  <rect x="128" y="820" width="134" height="5" rx="2.5" fill="#141824"/>
</g>
</svg>`;

export const loginPreviewSrc = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
