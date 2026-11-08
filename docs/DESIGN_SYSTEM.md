# GatherForge — Collection Studio (v3.2)

## Point of view
An editorial, music-first filing desk: paper content surfaces, midnight navigation, cobalt primary actions and a single copper record-label motif. Not a SaaS dashboard or neon gradient entertainment app.

## Tokens
- Paper `#F4F1EB` (main workspace)
- Porcelain `#FFFEFA` (interactive content)
- Midnight ink `#1B2430` (navigation and record hero)
- Text `#222A34`
- Cobalt `#364BBD` (interactive emphasis only)
- Copper `#EE965D` (record artwork)
- Secondary text `#5C636D`
- Divider `#D6D3CB`

## Structure
- 222 px left navigation; no generic eyebrow headings and no unnecessary numbered navigation markers.
- Only one dramatic element: vinyl disc on the overview; operations remain clean and familiar.
- Library list is row-dense; artist group and details are editable, not hidden behind popup gimmicks.
- Feature hierarchy: review/keep decisions > staging > verified publish > finalize.
- Downloads save to `_Incoming`; they do not publish automatically.

## Craft rules from source skills
- Anthropic frontend-design: subject-specific character, distinctive typography and disciplined use of a single memorable element.
- Impeccable Operate: native task affordances, contrast, explicit empty/error/loading states, no card soup.
- better-ui: optical alignment, consistent nested corner radii, meaningful hover and selected states.
- Avoid all-uppercase decorative labels, gradient text, fake analytics, and unrelated design embellishment.

## Typography
Default egui proportional font; 28 px screen title, 19 px section title, 13 px body, 11–12 px captions. Native system font fallback is intentional to keep cross-platform builds dependable.

## Accessibility
Keyboard focus must remain visible; primary actions and status use both words and color; avoid motion without state change. Condense the overview on narrow windows rather than clipping core workflows.
