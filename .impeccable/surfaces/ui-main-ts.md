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

OWN-WORLD: Ink-tinted neutral surfaces, muted blue active controls, 14px rounded section containers and precise horizontal setting rows. Segoe UI / Microsoft YaHei system typography belongs to this desktop utility. Dark is the user-confirmed default, with system and light options retained.

STORY: Identify the target screen and mode first, read its live state, adjust timing and transitions, then apply or save. Any preset parameter edit changes the selection to Custom; Daily and Movie stay fixed.

FIRST VIEWPORT: A small fishcreen header with the existing icon and a restore action, then screen and mode controls. A slim status line remains visible; the scroll region contains grouped settings. Bottom actions are fixed, showing dirty/saved/error states without interrupting the task.

FORM: Single-page vertical groups, the user's explicit selection from the offered layouts (candidate 1; surface seed b45f1687). Code-first explicitly selected. The signature interaction is the live readout beside each slider, with number inputs allowing precise keyboard control and a restrained custom-preset transition.

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance
