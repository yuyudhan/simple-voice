<!-- FilePath: docs/internal/design.md -->

# Signal: the Simple Voice design language

Simple Voice is a quiet instrument. It sits in the background, listens when asked, and gets out
of the way. The interface should feel like a well-made piece of audio equipment: neutral body,
precise labels, legible readouts, and one small light that tells you it is live. It must never
read as a copy of another dictation product; warm paper palettes, serif editorial headings,
gradient hero banners and chat-bubble previews are deliberately not part of this language.

The tokens live in `src/styles/tokens.css`; shared components in `src/ui/`. Code to this
document; when the language changes, change it here in the same commit.

## Principles

1. **Instrument, not stationery.** Colourless neutral greys, a crisp sans-serif, hairlines instead
   of shadows. Nothing decorative that does not carry information.
2. **One signal.** Signal orange is the only accent and means _live, selected, or focused_:
   recording, the active page, the chosen option, the focus ring, the heatmap. It is never used
   for decoration or for large fills.
3. **Ink acts.** Primary actions are filled with ink (near-black in light, near-white in dark),
   not with the accent. Secondary actions are hairline-bordered; tertiary ones are ghost.
4. **Readouts.** Figures are data, so they are set like readouts: `--font-mono`, tabular, with a
   small mono uppercase label above them.
5. **Flat and layered by hairlines.** Surfaces are separated by 1px borders and a step in
   neutral value. Shadows exist only on things that float (modals, toasts, popovers, overlay).
6. **Calm motion.** 150 ms, ease-out, opacity and small translations only. Motion confirms a
   change; it never performs. Everything respects reduced motion.

## Signature elements

- **The level line.** A 1px hairline with a short signal-orange segment at its start. It sits
  under every page header (`PageHeader`) and, turned vertical, marks the active sidebar item.
- **Meter bars.** Rounded vertical bars are the visual unit for sound: the brand mark, the
  overlay's level meter, loading shimmer, and bar-style charts.
- **Instrument labels.** Section and field captions use `.caps-label`: mono, 10.5px, uppercase,
  tracked. Labels name things; they never shout.

## Tokens

| Group      | Tokens                                                                   | Use                                                       |
| ---------- | ------------------------------------------------------------------------ | --------------------------------------------------------- |
| Neutrals   | `--bg`, `--surface`, `--surface-hover`, `--panel`, `--border(-strong)`   | Page, sidebar and wells, hover, cards, hairlines          |
| Text       | `--text`, `--muted`, `--faint`                                           | Body, secondary copy, placeholders and disabled           |
| Ink        | `--ink`, `--ink-hover`, `--on-ink`, `--on-ink-success/danger`            | Primary buttons, toasts (inverted), brand mark            |
| Signal     | `--accent`, `--accent-hover`, `--accent-2/3`, `--accent-soft/tint`       | Live state, selection, focus, charts (fills and graphics) |
| Signal ink | `--accent-text`                                                          | Accent-coloured text and links (AA contrast on light)     |
| Status     | `--danger(-soft)`, `--on-danger`, `--success(-soft)`, `--warning(-soft)` | Errors and destructive actions, confirmations, cautions   |
| Heat       | `--heat-0` … `--heat-4`                                                  | Activity heatmap ramp                                     |
| Shape      | `--radius-card` 10px, `--radius-control` 6px, `--radius-pill`            | Cards and groups, inputs and buttons, toggles             |
| Type       | `--font-display`, `--font-sans`, `--font-mono`                           | Titles, body, figures and labels                          |
| Motion     | `--dur` 150ms, `--ease`                                                  | All transitions                                           |

Components never hard-code colours. The overlay pill is the single exception: it floats over
arbitrary apps, so it keeps a fixed graphite palette in both themes. It is drawn natively by the
Swift helper, so its palette, sizes and motion live in `PillStyle`
(`engine/Sources/SimpleVoiceEngine/PillView.swift`), not in `src/styles/`; its icons are SF
Symbols.

## Type scale

| Role          | Font    | Size / weight           | Notes                             |
| ------------- | ------- | ----------------------- | --------------------------------- |
| Page title    | display | 24px / 600, -0.02em     | `.page-title`, one per page       |
| Section title | display | 15px / 600, -0.01em     | Card and group headings           |
| Body          | sans    | 13.5–14px / 400         | Line height 1.5–1.6               |
| Secondary     | sans    | 12–12.5px / 400         | `--muted`                         |
| Label         | mono    | 10.5px / 500, uppercase | `.caps-label`, tracking 0.08em    |
| Readout       | mono    | 20–36px / 500, tabular  | `.readout`, figures only, -0.02em |

## Layout

- The window is split into a sidebar (`--surface`, hairline right edge) and a content area on
  `--bg`. There is no inset floating panel.
- Content is centred at a 760px measure with 32–40px gutters. Every page starts with
  `PageHeader` (title, one-line description, optional actions, the level line).
- Lists are hairline tables: rows separated by `--border`, no per-row cards.
- Cards (`Card`) are `--panel` with a hairline border and `--radius-card`; no shadow.

## Components

- **Buttons.** `primary` = ink fill; `secondary` = hairline on panel; `ghost` = text only;
  `danger` = danger fill. 6px radius, 32px (md) / 26px (sm) tall.
- **Selection** (segmented controls, option cards, radio lists): the selected item is lifted to
  `--panel` with a strong hairline (or an ink/signal outline for cards) plus a small signal
  marker such as an accent icon or dot; never a colour-flooded card.
- **Badges.** Small 4px-radius tags on a soft status tint; not pills.
- **Toasts.** Inverted: ink surface, `--on-ink` text, status icons in `--on-ink-success/danger`.
- **Toggle.** Pill track; on = `--accent`.
- **Focus.** `--focus-ring`: a 2px signal ring offset by 1px of panel.
- **Empty states.** A meter-bar or line icon in a hairline square, a short title, one sentence.

## Themes

Users pick **System**, **Light** or **Dark** in Settings → General → Appearance (setting
`theme`), or flip between light and dark with the icon button in the window's top-right corner
(`src/app/ThemeToggle.tsx`: a moon while light, a sun while dark; from System it pins the
opposite of what is showing). `src/app/theme.ts` resolves the choice to
`data-theme="light" | "dark"` on `<html>`,
follows the OS live while the choice is System, sets the native window appearance so the title
bar matches, and caches the choice so the first frame paints in the right theme. Styles select
the dark palette with `:root[data-theme="dark"]` only; components never use
`prefers-color-scheme` directly.
