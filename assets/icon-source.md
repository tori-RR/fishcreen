# fishcreen icon

Generated using the built-in `image_gen.imagegen` tool (not the CLI/API fallback), 2026-10-30.

The user's supplied sketch establishes the composition: an unlit portrait secondary display on the left, and a lit blue landscape primary display on the right.

## Files

- `fishcreen-source.png`: original generated transparent raster (1536 × 1024).
- `fishcreen.png`: square transparent app image, mechanically fitted using Tauri's icon generator.
- `fishcreen.ico`: Windows icon with 16, 24, 32, 48, 64 and 256 px images, all 32-bit.
- `fishcreen-legacy.ico`: previous app icon retained for rollback.

The square PNG and ICO were produced with:

```powershell
node .\node_modules\@tauri-apps\cli\tauri.js icon .\assets\fishcreen-source.png --fit contain --output .\src-tauri\target\icon-build
```

`icon-build\icon.png` and `icon-build\icon.ico` are copied to the final names above. Transparency was verified through alpha samples, and the 16 px and 32 px results were visually inspected. No manual image editing was applied.

## Generation prompt

```text
Use case: sketch-to-render.
Asset type: final Windows desktop application icon for fishcreen, designed to remain identifiable at 16px and 32px.
Input image 1 is a rough composition reference, not a texture: two adjacent screens, the narrow portrait screen on the LEFT is off, the larger landscape screen on the RIGHT is on. Retain that precise left/right relationship and roughly aligned upper edges.
Produce ONE refined icon on a genuinely transparent square canvas. The two screens are simple rounded rectangles with confident geometric contours, front-facing, no perspective. Left portrait display: dark graphite gray, visibly switched off, with a subtle lighter outline so it remains visible on dark taskbars. Right landscape display: clean luminous cyan-blue with a very restrained smooth gradient suggesting the screen is lit. Elegant thin charcoal frame and subtle edge highlight; soft, controlled corners, premium and minimal, not cartoonish. A narrow transparent gap clearly separates the two screens. The silhouette fills about 85% of the canvas width and is vertically centered. The right screen is roughly three times the left screen's width and about 1.45 times its height as in the drawing.
No text, no letters, no numbers, no logos, no power symbols, no stars, no stands, no keyboards, no base plate, no enclosing rounded-square background, no cast shadow, no external glow. No gray background. Preserve actual alpha transparency around and between the screens.
```

Generation used `transparent_background: true`. The result contains restrained translucent edge shading; the surrounding canvas is transparent.
