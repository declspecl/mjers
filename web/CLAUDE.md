# mjers web

Single Vite and React app that plays the engine in the browser through `@mjers/mjers` (linked from `../npm`).

## Commands

```sh
pnpm install
pnpm dev                # rebuilds the wasm (predev), then serves on :5173
pnpm build              # rebuilds the wasm (prebuild), then vite build
pnpm test               # engine test and design rules
pnpm run lint:check     # eslint, zero warnings
pnpm run check-types
pnpm run prettier       # write; prettier:check to verify
```

## Layout

`src/app.tsx`, `main.tsx`, `components/`, `engine/` (wasm loading), `styles/tokens.css` (all colors and fonts), `index.css`.
Files in `src/` are kebab-case or PascalCase. Keep game logic in the engine; the UI only calls `Game`, `Bot`, and `advise`.
Run `advise` and `analyze` with playouts in a Web Worker.

## Tiles

Tile faces are real artwork: 42 transparent PNGs in `src/assets/tiles/` from Anisimova's Mahjong game pack (terms in `NOTICE.md` there). `tileAssetName` in
`src/engine/tiles.ts` maps engine notation to files, and a test fails if any dealt tile lacks artwork. Do not redraw tiles as SVG or CSS. Concealed tiles use the
CSS `TileBack`. `scripts/make-tiles.py` rebuilds the PNGs from the pack's EPS.

## Design

Use the `frontend-design` skill (`.agents/skills/frontend-design`) before building or reshaping any screen. Write a short design plan first and check it against the subject: a mahjong table, tiles, a hand in progress.

### Forbidden

`pnpm test` and `pnpm run lint:check` enforce the items marked (enforced). The rest are review rules, and they hold equally.

- All caps text (enforced). No `text-transform: uppercase`, no `uppercase` class, no small caps, and no JSX text or string in capitals. Labels use sentence case. Wind names and tile notation follow the engine's own notation; spell a word like `East` normally.
- Tracked-out eyebrow labels above headings, and extra labels that restate the heading.
- Gradients (enforced): gradient backgrounds, gradient text, gradient buttons. Use flat colors from `tokens.css`.
- Glassmorphism (enforced): `backdrop-filter`, blur panels.
- Glow (enforced): colored glow shadows, neon accents, `drop-shadow`.
- Cards that lift or scale on hover (enforced). Show hover and focus with a border, color, or outline change.
- Emoji as icons or decoration (enforced). Draw tiles and icons as SVG.
- Template punctuation in copy (enforced): `→` on links and buttons, middle-dot meta strings (`A · B · C`), spaced em dashes in labels.
- Hype copy (enforced): seamless, elevate, unleash, supercharge, effortless, and similar. Name what the control does: "Draw tile", not "Take your game further".
- The SaaS card kit: identical rounded cards for every piece of content, one radius on everything, the same soft grey shadow under each.
- Hero formula: big number, small label, row of stats, gradient accent.
- Numbered markers (01, 02, 03) on content that is not a sequence.
- Pill badges and chips on text that needs no status.
- Fade-and-slide-up entrances on every section. Motion answers an action (a tile drawn, a claim made) or marks one moment.
- Defaults the skill lists as tells: cream background with a terracotta accent, near-black with one acid-green accent, tinted near-black standing in for black, a monospace face on small labels.
- Placeholder copy such as "Lorem ipsum", "Feature one", or "Get started".

### Required

- Responsive down to a phone. Visible keyboard focus. Respect `prefers-reduced-motion`.
- Colors and fonts come from `styles/tokens.css`; support light and dark.
- Buttons say what they do and keep the same name through the flow ("Discard", then "Discarded 5m").
- Errors say what happened and what to do next.
