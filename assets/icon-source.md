# fishcreen icon

`fishcreen.svg` is the editable vector source, redrawn from the existing approved icon on 2026-11-15. It retains the unlit portrait secondary display on the left and the lit blue landscape primary display on the right.

The transparent 512 × 512 canvas contains rounded rectangles, paths, clipping paths and linear gradients only. It has no embedded bitmap, font, script or external resource. The original composition, blue screen, graphite display and restrained edge highlights are preserved.

## Files

- `fishcreen.svg`: source used directly by the settings panel and browser favicon.
- `fishcreen.png`: 512 × 512 RGBA export from the SVG.
- `fishcreen.ico`: Windows export with 16, 24, 32, 48, 64 and 256 px images, all 32-bit; shared by the EXE, window and tray.
- `fishcreen-source.png`: original generated raster reference, retained unchanged.
- `fishcreen-legacy.ico`: earlier icon retained unchanged.

## Regenerate

After installing project dependencies, run:

```powershell
.\scripts\build-icons.ps1
```

This uses the project's Tauri CLI to render the SVG into `src-tauri\target\svg-icon-build`, then copies its PNG and ICO exports into `assets`. Windows build resources are automatically regenerated when the ICO changes.

## Original raster reference

The original raster was generated using the built-in `image_gen.imagegen` tool on 2026-10-30, based on the user's sketch. It is retained as a reference; it is no longer the production source. Its original generation prompt follows.

```text
Use case: sketch-to-render.
Asset type: final Windows desktop application icon for fishcreen, designed to remain identifiable at 16px and 32px.
Input image 1 is a rough composition reference, not a texture: two adjacent screens, the narrow portrait screen on the LEFT is off, the larger landscape screen on the RIGHT is on. Retain that precise left/right relationship and roughly aligned upper edges.
Produce ONE refined icon on a genuinely transparent square canvas. The two screens are simple rounded rectangles with confident geometric contours, front-facing, no perspective. Left portrait display: dark graphite gray, visibly switched off, with a subtle lighter outline so it remains visible on dark taskbars. Right landscape display: clean luminous cyan-blue with a very restrained smooth gradient suggesting the screen is lit. Elegant thin charcoal frame and subtle edge highlight; soft, controlled corners, premium and minimal, not cartoonish. A narrow transparent gap clearly separates the two screens. The silhouette fills about 85% of the canvas width and is vertically centered. The right screen is roughly three times the left screen's width and about 1.45 times its height as in the drawing.
No text, no letters, no numbers, no logos, no power symbols, no stars, no stands, no keyboards, no base plate, no enclosing rounded-square background, no cast shadow, no external glow. No gray background. Preserve actual alpha transparency around and between the screens.
```

Generation used `transparent_background: true`. The result contains restrained translucent edge shading; the surrounding canvas is transparent.
