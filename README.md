# gabc-rs

Gregorian chant notation (GABC) and metered hymns (ABC) rendered through one HTML+CSS string into PDF, PNG and the browser.

**Status: early.** There is no parser yet. What exists is the groundwork:

- `assets/fonts/gabc-neumes.ttf`: a neume font of 36 glyphs on private-use code points U+E000 to U+E023, built by `cargo run -p xtask -- font`.
- `spike/`: renders a hand-written chant line through [printpdf](https://github.com/fschutt/printpdf) (PDF) and [azul](https://github.com/fschutt/azul)'s CPU renderer (PNG) to check that both lay it out alike.
- `docs/engine-bugs.md`: what that check found in the engines, with reproduction pages in `spike/cases/`.

## How it is meant to work

```
GABC / ABC text -> syntax tree and diagnostics -> neume shapes -> lines -> HTML+CSS string
                                                                            |-> printpdf    -> PDF
                                                                            |-> azul        -> PNG
                                                                            '-> a browser
```

Neumes are glyphs of the neume font, placed with `em` offsets, so one `font-size` scales a score. One em is four staff spaces, and a glyph's origin is in the middle of its box.

## Licences

- The code is under the MIT licence, see `LICENSE`.
- The glyph outlines in `third_party/exsurge/` and the font built from them come from [exsurge](https://github.com/frmatthew/exsurge), Copyright (c) 2008-2016 Fr. Matthew Spencer, OSJ, MIT licence, see `third_party/exsurge/LICENSE`.
- gabc-rs contains no code from Gregorio, which is under the GPL.
