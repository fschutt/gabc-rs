# Engine findings from the M0 spike

Date: 2026-11-18. Engines: printpdf 0.12.9, azul-layout / azul-core / azul-css 0.0.17, allsorts-azul 0.17.2 (all from crates.io). Rule: each of these is fixed in the engine with a regression test there, not worked around in gabc-rs.

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

**Fills: azul and printpdf.** A fill is "paint a rectangle (or gradient) under a mask clip", and `PushImageMaskClip { bounds, mask_image, mask_rect }` has no geometry. Proposal, for review before I write it:

- Add `geometry: Option<MaskGeometry>` to `PushImageMaskClip`, with `MaskGeometry { path: SvgMultiPolygon, view_box: Option<(f32, f32, f32, f32)>, even_odd: bool }`, set where the mask is rasterised from an `SvgNodeData::Path` and `None` for a real image mask. About ten sites in azul name the variant's fields.
- In printpdf, `PushImageMaskClip` with geometry becomes `q` plus a clip path (`W` or `W*`), and `PopImageMaskClip` becomes `Q`, exactly as `PushClip`/`PopClip` are handled today. Whatever is painted under it (colour, gradient) is then clipped by the real path.
- Open point: the `SvgNodeData::Path` arm of `push_image_mask_clip` only exists with the `cpurender` feature. printpdf's `html` feature does not enable it, so a build that only makes PDFs emits no clip at all for an SVG fill. The item needs to be emitted without a rasteriser too. `StrokedPath` solves the same problem with `mask: Option<ImageRef>`; for the clip either `mask_image` becomes optional, or the no-rasteriser arm passes a null image. That choice changes what GPU consumers see, so it is the engine owner's.
- A real image mask (`SvgNodeData::ImageClipMask`) needs a PDF soft mask. Separate and smaller; nothing in gabc-rs uses it.

## E2. PNG: text ink is clipped to its line box

Case: `spike/cases/text-ink-clip.html`.

With `line-height: 1` the descenders of `g j p q y` are cut off; with `line-height: 0.5` the text is sliced to a band; a neume glyph taller than its line box is cut the same way. The PDF is correct. `overflow` is `visible` everywhere in the case, so nothing may be clipped.

Cause: `DisplayListItem::Text.clip_rect` does two jobs.

- `paint_inline_content` (`layout/src/solver3/display_list.rs`) gives every text run the container's content box as `clip_rect`. On an overflow-visible axis that box is widened horizontally by one em for glyph ink, but deliberately not vertically.
- The reason is in `layout/src/solver3/page_breaks.rs`: `atomic_lines` and the widows/orphans grouping read the same `clip_rect` as the line box that must not be split (azul#478).
- cpurender (`text_run_clip` in `layout/src/cpurender/raster.rs`) clips the ink to it. printpdf ignores it, which is why the PDF is right.

Proposed fix, for review before I write it: give the text item one rectangle per job, the line box for pagination and a paint clip that, on a visible axis, does not cut ink (the vertical counterpart of the one-em horizontal allowance that is already there). The `Text` variant is built or matched without `..` in about 33 places in azul and in one in printpdf, so this is a mechanical but wide change, and it touches the code azul#478 fixed. Loosening the shared rectangle instead would make the avoid-ranges of neighbouring lines overlap again, which is what #478 was.

## E3. Every `<svg>` element produces a text run without a font

## E3. Every `<svg>` element produces a text run without a font

Each `<svg>` in a document prints, in debug builds, `dropping a TextLayout with FontHash::invalid()` once per render. A document without `<svg>` prints none. Harmless today, since the item is dropped, but something creates an empty text layout for an SVG element. Not yet traced.

## E4. PNG: no way to ask for grayscale text anti-aliasing

`render_pages` draws text with LCD subpixel anti-aliasing on desktop targets, which leaves colour fringes in an exported image. The only switch is the environment variable `AZ_TEXT_AA=grayscale` (`layout/src/cpurender/raster.rs`), which does not exist in WASM. `PageRenderOptions` needs a text anti-aliasing field; a page image has no subpixel layout.

## E5. Background of an absolutely positioned `<span>` is not painted

Case: first row and last row of `spike/cases/text-ink-clip.html`. The spans have a `background`, and neither renderer paints it, while the same on a `<div>` or `<p>` works. gabc-rs does not need this, so it is recorded here and not on our critical path.

## Added to allsorts-azul

`allsorts::font_builder` (`TrueTypeBuilder`, `Outline`) on branch `feat/font-builder`, based on `azul-fork-0.17`: builds a TrueType font from outlines, with cubic-to-quadratic conversion. `subset::FontBuilder` was crate-private, so there was no public way to assemble a font from scratch. `xtask font` uses it to build `assets/fonts/gabc-neumes.ttf`.
