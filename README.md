# gabc-rs

Gregorian chant notation (GABC) and metered hymns (ABC) rendered through one HTML+CSS string into PDF, PNG and the browser.

**Status: early.** The parsers and the checker work; nothing is drawn from a score yet.

- `gabc check <files or directories>`: checks `.gabc` and `.abc` files and the notation blocks of Markdown files, and prints every problem as `path:line:col: error[E011]: message`.
- `gabc_rs::gabc::parse`: a strict GABC parser. It reports every problem with its position and still returns what it could read.
- `gabc_rs::abc::parse`: the same for hymns, in a subset of ABC 2.1 (one tune, one voice, lyrics). It also checks that bars add up to the metre and that every note has its syllable.
- `assets/fonts/gabc-neumes.ttf`: a neume font of 36 glyphs on private-use code points U+E000 to U+E023, built by `cargo run -p xtask -- font`.
- `spike/`: renders a hand-written chant line through [printpdf](https://github.com/fschutt/printpdf) (PDF) and [azul](https://github.com/fschutt/azul)'s CPU renderer (PNG) to check that both lay it out alike.
- `docs/engine-bugs.md`: what that check found in the engines, with reproduction pages in `spike/cases/`. The fixes are released.

## The reference run

`cargo run -p xtask -- corpus gregobase` downloads the [GregoBase](https://gregobase.selapa.net) database (about 18,700 scores, transcriptions under CC0) into `target/corpus`, checks every score and writes a report. With `--oracle gregorio` each score is also given to an installed Gregorio, and the report lists where its verdict and ours differ. The `Corpus` workflow does this on every change to the parser.

Gregorio is only ever run, as a program. Questions about the format are settled from its public description and from what the program outputs, never from its source.

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
