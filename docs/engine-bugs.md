# Engine findings from the M0 spike

Found on 2026-11-18 with printpdf 0.12.9, azul-layout / azul-core / azul-css 0.0.17 and allsorts-azul 0.17.2 from crates.io. Rule: each of these is fixed in the engine with a regression test there, not worked around in gabc-rs.

## Status

| # | Finding | Fix | State |
|---|---|---|---|
| E1 | SVG paths in a PDF: fills print as boxes, strokes are missing | strokes: [printpdf#298](https://github.com/fschutt/printpdf/pull/298); fills: [azul#484](https://github.com/fschutt/azul/pull/484) and [printpdf#299](https://github.com/fschutt/printpdf/pull/299) | released: azul 0.0.18, printpdf 0.12.10 |
| E2 | PNG: text ink cut at its line box | [azul#484](https://github.com/fschutt/azul/pull/484) | released: azul 0.0.18 |
| E3 | A line without text reports an unresolved font | [azul#484](https://github.com/fschutt/azul/pull/484) | released: azul 0.0.18 |
| E4 | Page images get LCD colour fringes, no way to choose | [azul#484](https://github.com/fschutt/azul/pull/484) | released: azul 0.0.18 |
| E5 | Background of an absolutely positioned `<span>` not painted | [azul#484](https://github.com/fschutt/azul/pull/484) | released: azul 0.0.18 |
| | No public way to build a font from outlines | [allsorts-azul#3](https://github.com/fschutt/allsorts-azul/pull/3) | open for review |

gabc-rs builds against the released crates, with no patches. With them, `spike/cases/svg-path-fill.html` and `spike/cases/text-ink-clip.html` render the same in the PDF and in the PNG. The sections below describe each finding and its cause.

How to reproduce any case:

```sh
cargo build -p spike
SPIKE_HTML=spike/cases/<case>.html ./target/debug/spike <Helvetica.ttf> Helvetica assets/fonts/gabc-neumes.ttf target/spike/out
pdftoppm -r 192 -png target/spike/out.pdf target/spike/out-pdf   # then compare with target/spike/out.png
```

## What works (no action)

- Glyph spans with `position: absolute` and `em` offsets land on the same pixels in the PDF and in the PNG.
- The neume font (private-use code points, no hinting) is found, shaped and drawn by both renderers, and embedded in the PDF as `GabcNeumes-Regular`.
- Thin boxes work as staff lines, stems and bar lines.
- A `break-inside: avoid` block is moved to the next page whole, in both renderers, with the same page count.
- The whole stack type-checks for `wasm32-unknown-unknown`.

## E1. PDF: SVG paths are not drawn as paths

Case: `spike/cases/svg-path-fill.html`.

| Shape | PDF | PNG (cpurender) |
|---|---|---|
| filled triangle | solid box of the fill colour | correct |
| stroked curve, `fill="none"` | nothing | correct |
| filled path with a hole (`evenodd`) | solid box | correct |

Cause, in two places:

- azul-layout turns the fill of an `SvgNodeData::Path` into a raster mask (`push_image_mask_clip` in `layout/src/solver3/display_list.rs`, only with the `cpurender` feature) and paints the fill colour as a rectangle under that mask. A stroke becomes a mask-based `DisplayListItem::StrokedPath`. The display list never carries the geometry.
- printpdf's bridge (`src/html/bridge.rs`, the `_ =>` arm) handles neither `PushImageMaskClip` nor `StrokedPath`, so the rectangle is painted unclipped and the stroke is skipped.

The two halves need different fixes.

**Strokes: printpdf only.** `StrokedPath` already carries the geometry (`path`, `view_box`, `bounds`), colour, width and `StrokeOutline` (caps, joins, miter limit, dashes). The bridge only has to write it. Done on printpdf branch `fix/svg-paths-in-pdf`: a `StrokedPath` arm, `SvgToPage` (the same view-box mapping as azul's `svg_user_space_mapping`), `svg_path_to_lines`, and three unit tests.

**Fills: azul and printpdf.** A fill is "paint a rectangle (or gradient) under a mask clip", and `PushImageMaskClip { bounds, mask_image, mask_rect }` has no geometry. What was done:

- Add `geometry: Option<MaskGeometry>` to `PushImageMaskClip`, with `MaskGeometry { path: SvgMultiPolygon, view_box: Option<(f32, f32, f32, f32)>, even_odd: bool }`, set where the mask is rasterised from an `SvgNodeData::Path` and `None` for a real image mask. About ten sites in azul name the variant's fields.
- In printpdf, `PushImageMaskClip` with geometry becomes `q` plus a clip path (`W` or `W*`), and `PopImageMaskClip` becomes `Q`, exactly as `PushClip`/`PopClip` are handled today. Whatever is painted under it (colour, gradient) is then clipped by the real path.
- The `SvgNodeData::Path` arm of `push_image_mask_clip` only existed with the `cpurender` feature, which printpdf's `html` feature does not enable, so a build that only makes PDFs emitted no clip at all for an SVG fill. `mask_image` is now optional and the item is emitted without a rasteriser too.
- A real image mask (`SvgNodeData::ImageClipMask`) needs a PDF soft mask. Separate and smaller; nothing in gabc-rs uses it.

## E2. PNG: text ink is clipped to its line box

Case: `spike/cases/text-ink-clip.html`.

With `line-height: 1` the descenders of `g j p q y` are cut off; with `line-height: 0.5` the text is sliced to a band; a neume glyph taller than its line box is cut the same way. The PDF is correct. `overflow` is `visible` everywhere in the case, so nothing may be clipped.

Cause: `DisplayListItem::Text.clip_rect` does two jobs.

- `paint_inline_content` (`layout/src/solver3/display_list.rs`) gives every text run the container's content box as `clip_rect`. On an overflow-visible axis that box is widened horizontally by one em for glyph ink, but deliberately not vertically.
- The reason is in `layout/src/solver3/page_breaks.rs`: `atomic_lines` and the widows/orphans grouping read the same `clip_rect` as the line box that must not be split (azul#478).
- cpurender (`text_run_clip` in `layout/src/cpurender/raster.rs`) clips the ink to it. printpdf ignores it, which is why the PDF is right.

Fix: the text item has one rectangle per job: `line_box` for pagination, and `clip_rect` as a paint clip that, on a visible axis, does not cut ink (the vertical counterpart of the one-em horizontal allowance that was already there). Loosening the shared rectangle instead would have made the avoid-ranges of neighbouring lines overlap again, which is what azul#478 was.


## E3. A line without text reports an unresolved font

Each inline `<svg>` in a document printed, in debug builds, `dropping a TextLayout with FontHash::invalid()` once per render. It is not about SVG: any block whose line holds only an inline-level box and no text (an inline-block, an inline `<svg>`) did it, and a block-level `<svg>` did not. `paint_inline_content` built a `TextLayout` for a line with no glyph runs, with no font to give it, and `push_item` dropped it again with that report. SVG `<text>` itself draws correctly with the registered font in both renderers.

Fix: the item is not built when the line has no glyph runs.

## E4. PNG: no way to ask for grayscale text anti-aliasing

`render_pages` drew text with LCD subpixel anti-aliasing on desktop targets, which leaves colour fringes in an exported image. The only switch was the environment variable `AZ_TEXT_AA=grayscale`, read once into a process-wide cache in the rasteriser, which does not exist in WASM.

Fix: the text anti-aliasing variables are read in one place (`TextAaSettings::from_env`), the value is carried by the glyph cache, and `PageRenderOptions` has a `text_aa` field. The spike passes `TextAaSettings::grayscale()`.

## E5. Background of an absolutely positioned `<span>` is not painted

Case: first row and last row of `spike/cases/text-ink-clip.html`. The spans have a `background`, and neither renderer painted it, while the same on a `<div>` or `<p>` worked.

Cause: a node's own background is skipped when its specified `display` is inline and its parent runs inline layout, on the assumption that the parent's line paints it. An absolutely positioned span is out of flow and on no line.

Fix: `parent_paints_as_inline_shape` answers no for `absolute` and `fixed` boxes. It is in the display list builder, so it applies to both renderers.

## Added to allsorts-azul

`allsorts::font_builder` (`TrueTypeBuilder`, `Outline`) on branch `feat/font-builder`, based on `azul-fork-0.17`: builds a TrueType font from outlines, with cubic-to-quadratic conversion. `subset::FontBuilder` was crate-private, so there was no public way to assemble a font from scratch. `xtask font` uses it to build `assets/fonts/gabc-neumes.ttf`.
