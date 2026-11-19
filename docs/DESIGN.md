# screencast — UI Design System (v2.0)

> **Normative UI specification.** Every color, spacing, motion token, and component state lives here.
> Any new UI token must be added to this document before modifying CSS or Python code.
> Both the TV web player (`assets/player/`) and Linux TUI/GTK3 GUI (`assets/gui/`, `screencast-ui`) share this system.

---

## 1. Design principles

1. **One screen, one focus.** Video fills the player stage; controls live in lightweight overlays, never permanent sidebars.
2. **Quiet canvas, focused accent.** Pure black `#000000` background, subtle grey hierarchy, single electric-blue accent `#0A84FF`.
3. **Purposeful motion.** 120-250 ms smooth easing; zero decorative animation; full `prefers-reduced-motion` support.
4. **10-foot legibility.** Readable on TVs from 2.5-4 m: body text >= 22 px (1080p scale >= 32 px), hit targets >= 48 px, contrast >= 4.5:1.
5. **Remote-first interaction.** Every control works via D-pad arrows + Enter/Back as well as mouse/touch.
6. **Silent keyboard capture.** Unless Remote mode (`C`) is active, playback shortcuts stay local to the player.
7. **English-first with live `en`/`tr` switching.** Both the Web Player menu (`Language`) and GTK3 HeaderBar (`EN`/`TR`) switch languages seamlessly at runtime.

## 2. Color palette (dark-first)

| Token | Value | Purpose |
|---|---|---|
| `--canvas` | `#000000` | Fullscreen player background & desktop GUI window |
| `--surface-1` | `rgba(28,28,30,0.72)` | Frosted control dock & pills |
| `--surface-2` | `rgba(44,44,46,0.92)` | Secondary popover menu |
| `--hairline` | `rgba(255,255,255,0.12)` | Subtle 1 px border divider |
| `--text-primary` | `#F5F5F7` | Primary labels & headings |
| `--text-secondary` | `rgba(245,245,247,0.62)` | Secondary captions & badges |
| `--text-tertiary` | `rgba(245,245,247,0.38)` | Section eyebrows & passive text |
| `--accent` | `#0A84FF` | Focus rings, active buttons, primary actions |
| `--accent-dim` | `rgba(10,132,255,0.18)` | Hover/focus surface fill |
| `--live` | `#FF3B30` | Live indicator dot |
| `--warn` | `#FF9F0A` | Buffering / elevated latency warning |
| `--ok` | `#30D158` | Healthy / low-latency badge |

**Frosted glass recipe:** `backdrop-filter: blur(40px) saturate(180%)`, `--surface-1` background, 1 px `--hairline` border, `inset 0 1px 0 rgba(255,255,255,0.06)` top highlight, `0 12px 40px rgba(0,0,0,0.55)` shadow.
**Color blindness:** Status badges always pair color with explicit icons and text labels.

## 3. Typography

| Role | Size (1080p ref) | Weight | Line height | Notes |
|---|---|---|---|---|
| Overlay title | 34 px | 600 | 1.2 | `SF Pro Display` stack |
| Body | 22 px | 400 | 1.45 | `clamp(16px, 1.45vw, 24px)` |
| Badge / telemetry | 19 px | 500 | 1.2 | `font-variant-numeric: tabular-nums` |
| Secondary | 17 px | 400 | 1.4 | Helper copy |

Font stack: `-apple-system, "SF Pro Display", "SF Pro Text", "Segoe UI Variable", Inter, system-ui, "Helvetica Neue", Arial, sans-serif`.

## 4. Spacing & radius tokens

| Token | Value | Use |
|---|---|---|
| `--s-1` | 4 px | Tight icon gap |
| `--s-2` | 8 px | Badge gap |
| `--s-3` | 12 px | Button horizontal padding |
| `--s-4` | 16 px | Panel padding |
| `--s-5` | 24 px | Card/edge margin |
| `--s-6` | 32 px | Section spacing |
| `--radius-s` | 10 px | Small controls |
| `--radius-m` | 16 px | Menus, cards, toasts |
| `--radius-l` | 22 px | Pill bars & action buttons |

## 5. Motion

| Interaction | Duration | Easing |
|---|---|---|
| Menu open/close | 200 ms | `cubic-bezier(0.32, 0.72, 0, 1)` |
| Control bar auto-hide | 250 ms | `--ease-panel` |
| Focus ring | 120 ms | `ease-out` |
| Toast / badge fade | 160 ms | `ease-in-out` |
| Remote cursor tracking | 0 ms (instant) | none |

`@media (prefers-reduced-motion: reduce)` collapses transitions and animations to zero.

## 6. Web player anatomy

```
┌──────────────────────────────────────────────────────────────┐
│ 1. Video stage (object-fit: contain, radius 0, pure black)   │
│                                                              │
│ 2. Remote cursor overlay (24 px white ring + 8 px halo)      │
│                                                              │
│ 3. Top bar (auto-hide): device pill · LIVE · quality · ms    │
│                                                              │
│ 4. Bottom dock (auto-hide, frosted glass)                    │
│    ▶/⏸   🔇/🔊   ⛶   🖱 Remote   ⋯ (TV Mode, Language, Quality)│
│                                                              │
│ 5. Toast notification (top-right, 3 s)                       │
└──────────────────────────────────────────────────────────────┘
```

### 6.1 Auto-hide behavior
- Pointer move, click, or key press reveals top/bottom bars; 2.5 s idle hides both (unless paused or menu open).
- Double-clicking video toggles fullscreen (when Remote mode is off).

### 6.2 Player states

| State | Visual presentation |
|---|---|
| Connecting | Solid black overlay + spinner + `"Connecting..."` |
| Live | Overlay hidden; top-right `● LIVE` pulse + device/quality/latency pills |
| Buffering | Translucent dim overlay + spinner + `"Slow network"` / `"Buffering stream..."` |
| Paused | Overlay + `"Paused"` + `"Resume"` action button |
| Permission denied | Solid overlay + `"Screen share permission denied"` + `"Retry"` |
| Error | Solid overlay + `"Stream disconnected"` + `"Reconnect"` |

### 6.3 Remote control overlay
- 24 px circular white cursor (`--live` border on right-click press).
- `C` toggles Remote mode; arrow keys nudge cursor 12 px, `Enter` left-clicks.

### 6.4 Telemetry badges
- Latency: `<= 400 ms` green (`--ok`), `401-700 ms` amber (`--warn`), `> 700 ms` red (`--live`).
- Quality: shows current tier plus `↑`/`↓` toast when adaptive controller switches resolution.

## 7. Terminal TUI (`screencast-ui`)

Default 80x24 layout (collapses cleanly below 80x24):

```
┌ screencast 0.1.0 | stream: 1080p · 6.2 Mb/s | Ready ────────┐
│ devices (4)                                                 │
│ ▸ LG OLED (10.9.46.60)      10.9.46.60:8009       ● ready   │
│   Samsung TV (10.9.46.61)   10.9.46.61:7250       ● MS-MICE │
│   Apple TV (10.9.46.62)     10.9.46.62:7000       ● unsupp. │
│   Browser mode (MSE)        http://10.9.46.50:7700          │
├─────────────────────────────────────────────────────────────┤
│ stats: 30 fps · 1 clients · latency 240 ms · audio on       │
├─────────────────────────────────────────────────────────────┤
│ ↑↓ select  ⏎ share  p pause  m mute  r scan  s snap  q quit │
└─────────────────────────────────────────────────────────────┘
```

## 8. Embedded asset modules (`<= 350` lines each)

```
assets/player/
├── index.html      # Minimal stage skeleton + ordered script tags
├── style.css       # Design tokens, core layout, frosted bars, reduced-motion
├── overlays.css    # Menu popover, state overlays, toast, TV-mode scaling
├── i18n.js         # English catalog + language switcher & DOM updater
├── i18n_tr.js      # Dedicated Turkish translation catalog
├── mse.js          # MediaSource/SourceBuffer queue & live-edge chaser
├── controls.js     # Mouse/keyboard/remote cursor & TV-mode handlers
├── app.js          # WebSocket lifecycle, /api/info, telemetry loop
└── hls.min.js      # Self-contained HLS fallback shim (`ScreencastHlsFallback`)

assets/gui/
├── screencast_gui.py          # Main GTK3 ScreencastWindow
├── screencast_gui_css.py      # CSS tokens matching §2
├── screencast_gui_i18n.py     # English catalog, language detector, parser tokens
├── screencast_gui_i18n_tr.py  # Dedicated Turkish GUI translation catalog
├── screencast_gui_helpers.py  # Binary lookup, device grouping, stdout parsers
├── screencast_gui_widgets.py  # Reusable GTK3 card/header/hero/settings builders
└── screencast_gui_runner.py   # Subprocess worker threads
```
