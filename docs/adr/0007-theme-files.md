# Themes are small data files resolved over a read-only built-in theme

Status: accepted
Ledger: D-043, D-044 (with R-030 to R-033)

The Owner wants themes that can be edited freely, while the Launcher must keep its R-006 budgets on a software renderer. A custom theme is therefore plain data: one TOML file in `%APPDATA%\NaobiCommand\themes`, named by a restricted id, that names a read-only base (`paper` or `stone`) and overrides only the values it sets. `naobi-core` parses and resolves it without Win32 or Slint, and `naobi-ui` receives a finished theme.

## Decision

- **Format.** `name`, `base`, and the tables `[colors]`, `[shape]`, `[list]` and `[fonts]`. Every key is optional, so a theme can be one line long. A bad value takes the base value and is reported with its line (R-033).
- **No executable or path values.** A theme holds colors, numbers, one density word and font family names. It cannot name a file, a URL or a command.
- **Fonts by family name only.** A theme uses fonts Windows already has. The app never loads a font file a theme points at. A family that is not installed falls back to the embedded font with a Notice.
- **Slots live in `settings.toml`.** `[theme] light` and `dark` name a theme. Enter in the Themes view rewrites only that one line, so the Owner's comments and order survive. A settings file that does not parse is never rewritten.
- **Preview is not persisted.** Moving the selection shows a theme for that moment. Nothing is written until Enter.
- **Window corners stay with Windows.** The Launcher window radius is 0, 4 or 8 px because DWM draws it. Radii inside the window are free within their limits.

## Considered options

- **Editor with color fields inside the Launcher.** Rejected for now: it adds a form system to a keyboard-first list and duplicates what any text editor does. The Themes view gives live preview, Duplicate and Edit file instead.
- **Themes inside `settings.toml`.** Rejected: one broken color would put the whole settings file at risk, and themes could not be copied or shared as single files.
- **Slint style files or scripted themes.** Rejected: they would need a compiler or interpreter at run time and would turn a color file into code.

## Consequences

- `naobi-core::tokens` stays the source of the built-in values, and the contrast check moves out of the test into code the app can call.
- A custom theme may be hard to read. Its Notice also goes through the tray and the Windows balloon, which do not use theme colors (T-039).
- Installed-font support is the one part that may be dropped: it ships only if the idle budget of R-006 still passes with it (R-030).

## Amendment 2026-11-20

- There are eleven Built-in themes: `paper`, `stone`, `tokyo-haze`, `neon-night`, `washi`, `sakura-dawn`, `matcha`, `dune`, `midnight-ink`, `kyoto-dusk` and `deep-sea` (D-046 v2, R-032 v3). All are read-only consts in `naobi-core`, meet WCAG 2.2 AA, and their ids are reserved against theme files (D-043 v3). `base` in a theme file stays `paper` or `stone`; the nine added themes are not bases, and Duplicate writes `base = "paper"` for the four light ones (`washi`, `sakura-dawn`, `matcha`, `dune`) and `base = "stone"` for the dark ones.
