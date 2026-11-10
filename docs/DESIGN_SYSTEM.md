# GatherForge — Collection Studio (v3.10)

## Point of view
A calm, music-first filing desk built from translucent glass panels: a deep midnight rail, pale
blue-grey canvas, teal signal accents for active state, and one warm copper record motif on the
overview. Not a SaaS dashboard or a neon gradient entertainment app.

## Tokens
Every colour comes from a function in `src/theme.rs`; screens must not hard-code hex values.
Light and dark variants are both defined there.

- `SIDEBAR` `#101B2B` (always-dark navigation rail)
- `INK` `#F5F9FD` / `#1E2F41` (text on the hero / dark text)
- `SIGNAL` `#8DD4DC` (active navigation, focus, record label)
- `base()` `#EDF3F8` light, `#0D1623` dark (canvas)
- `surface()` translucent white in light, translucent slate in dark (panels)
- `surface_raised()` `#FFFFFF` / `#1F2D3E` (windows, popups)
- `white()` `#1E2F41` light, `#F1F7FC` dark (body text)
- `muted()` `#5A6D7E` / `#ACBFCE` (secondary text)
- `lime()` `#2D75A6` / `#61C1DE` (primary emphasis)
- `lavender()` `#2E7774` / `#91D6C5` (secondary emphasis)
- `coral()` `#B25841` / `#FFAB93` (warnings, record rim)
- `hero()` `#2B4B69` / `#233950` (overview banner)
- `stroke()`, `glass_edge()`, `glass_hover()` translucent borders and hover fills

## Structure
- 226 px left navigation (`theme::SIDEBAR_WIDTH`); no generic eyebrow headings and no unnecessary numbered navigation markers.
- Only one dramatic element: the record graphic in the overview hero (hidden below 565 px of
  available width); operations remain clean and familiar.
- Library list is row-dense; artist group and details are editable, not hidden behind popup gimmicks.
- Feature hierarchy: review/keep decisions > staging > verified publish > finalize.
- Downloads save to `_Incoming`; they do not publish automatically.

## Craft rules from source skills
- Anthropic frontend-design: subject-specific character, distinctive typography and disciplined use of a single memorable element.
- Impeccable Operate: native task affordances, contrast, explicit empty/error/loading states, no card soup.
- better-ui: optical alignment, consistent nested corner radii, meaningful hover and selected states.
- Avoid all-uppercase decorative labels, gradient text, fake analytics, and unrelated design embellishment.

## Typography
Default egui proportional font; 24 px screen title, 19 px section title, 13 px body, 11–12 px captions (the overview hero heading in `src/app.rs` is 30 px). Native system font fallback is intentional to keep cross-platform builds dependable.

## Accessibility
Keyboard focus must remain visible; primary actions and status use both words and color; avoid motion without state change. Condense the overview on narrow windows rather than clipping core workflows.


## V3.3 density and alignment rules
- Music-library track cards and their artwork frames: 8 px radius. Other interface rectangles, controls, cards and navigation items use 2 px and stay strictly below 4 px (`theme::RADIUS`, GATHERFORGE_CORNER_RULE_V1).
- Controls and navigation: 36 px / 40 px respective heights; 58 px track rows (`theme::ROW_HEIGHT`).
- Page title: 24 px; section: 19 px; body: 13 px; metadata: 11–12 px.
- Horizontal grid: equal-width KPI columns; list/inspector 64/36 with a single gap.
- The hero hides decorative artwork when space is insufficient, never expands the page.
- Track row reserves format and decision columns before truncating song titles.
- Avoid unbounded minimum widths and arbitrary per-card geometry.
