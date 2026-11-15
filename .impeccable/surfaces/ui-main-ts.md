---
version: 1
slug: "ui-main-ts"
primary_target: "ui/main.ts"
related_targets: ["ui/style.css"]
---

# fishcreen 设置面板

Mode: Operate. Windows desktop WebView2 settings. User confirmed all existing functionality stays, the settings panel is redesigned, a single vertically grouped page shows all parameters, and implementation starts directly in code.

## Direction contract

THESIS: A quiet place to tune one secondary screen. The full parameter flow remains on one page, with a clear separation between leaving, returning and going idle.

OWN-WORLD: Codex-reference black and gray neutral surfaces, active controls defined by tonal contrast, 14px rounded section containers and precise horizontal setting rows. Blue remains only in the application icon; green and red retain their status meanings. Segoe UI / Microsoft YaHei system typography belongs to this desktop utility. Dark is the user-confirmed default, with system and light options retained.

STORY: Identify the target screen and mode first, read its live state, adjust timing and transitions, then apply or save. Any preset parameter edit changes the selection to Custom; Daily and Movie stay fixed.

FIRST VIEWPORT: A small fishcreen header with the existing icon and a restore action. The live status sits directly below the application name, replacing the slogan, and the version shows only its numeric value. The main scroll region begins with screen and mode controls, followed by grouped settings, without a duplicate status line. Bottom actions are fixed, showing dirty/saved/error states without interrupting the task.

FORM: Single-page vertical groups, the user's explicit selection from the offered layouts (candidate 1; surface seed b45f1687). Code-first explicitly selected. The header/content/footer layout and single scrolling region stay unchanged. General appearance and startup rows retain their labels and controls but omit their two explanatory hints. The signature interaction is the live readout beside each slider, with number inputs allowing precise keyboard control and a restrained custom-preset transition.

WINDOW: Default 480 × 880 logical pixels, minimum 420 × 520. Compact rows keep labels and 180px control groups side by side; sliders stack only at 440px and below. Text, exact number inputs and all parameters remain available.

BLACKOUT COVERAGE: Show Active Window is a global checkbox, off by default for full coverage. Enabling it leaves the current foreground window's visible-frame rectangle uncovered where it intersects the selected display. It persists across presets without entering Custom and does not change hardware brightness. Switch labels provide 44 × 44px hit targets while retaining the existing 38 × 22px visual track.

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance
