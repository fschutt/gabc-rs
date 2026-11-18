# gabc-rs

Gregorian chant notation (GABC) and metered hymns (ABC) rendered through one HTML+CSS string into PDF, PNG and the browser.

**Status: early.** The parsers and the checker work. Chant and hymns are drawn in a first version: neumes on four lines, round notes on five, with clef, bars, lyrics and line breaks. Still to come: real text metrics, ties, slurs and beams, the drop cap, PDF and PNG straight from the library.

- `gabc check <files or directories>`: checks `.gabc` and `.abc` files and the notation blocks of Markdown files, and prints every problem as `path:line:col: error[E011]: message`.
- `gabc html <file.gabc or file.abc>`: the score as an HTML document, which printpdf turns into a PDF and azul into a PNG.
- `gabc_rs::render`: one notation block to the HTML of its score.
- `wasm/` and `demo/`: the same for the browser, as a WebAssembly module with three plain exports and a test page with a text box and a preview: <https://fschutt.github.io/gabc-rs/>.
- `gabc_rs::abc_layout`: a hymn on a five-line staff with its verses in rows.
- `gabc_rs::neume`, `gabc_rs::layout`, `gabc_rs::html`: notes into neumes with their glyphs (a port of exsurge), syllables into lines, lines into HTML and CSS.
- `gabc_rs::gabc::parse`: a strict GABC parser. It reports every problem with its position and still returns what it could read.
- `gabc_rs::abc::parse`: the same for hymns, in a subset of ABC 2.1 (one tune, one voice, lyrics). It also checks that bars add up to the metre and that every note has its syllable.
- `assets/fonts/gabc-neumes.ttf`: a neume font of 36 glyphs on private-use code points U+E000 to U+E023, built by `cargo run -p xtask -- font`.
- `spike/`: renders a hand-written chant line through [printpdf](https://github.com/fschutt/printpdf) (PDF) and [azul](https://github.com/fschutt/azul)'s CPU renderer (PNG) to check that both lay it out alike.
- `docs/engine-bugs.md`: what that check found in the engines, with reproduction pages in `spike/cases/`. The fixes are released.

## The reference run

`cargo run -p xtask -- corpus gregobase` downloads the [GregoBase](https://gregobase.selapa.net) database (about 18,700 scores, transcriptions under CC0) into `target/corpus`, checks every score and writes a report. With `--oracle gregorio` each score is also given to an installed Gregorio, and the report lists where its verdict and ours differ. The `Corpus` workflow does this on every change to the parser.

Gregorio is only ever run, as a program. Questions about the format are settled from its public description and from what the program outputs, never from its source. `docs/gabc-observed.md` lists what was observed, and the `Probe` workflow asks it about a piece of GABC. On the GregoBase library the two parsers reach the same verdict for 99.89% of the scores. Of the scores both accept, all but two hold the same clefs, notes, bars, custodes and line breaks in the same order.

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
- `assets/fonts/gabc-music.ttf` holds 33 glyphs of [Bravura](https://github.com/steinbergmedia/bravura), Copyright (c) 2015 Steinberg Media Technologies GmbH, under the SIL Open Font License 1.1 (`assets/fonts/gabc-music-OFL.txt`). The licence reserves the name "Bravura", so the font is called "Gabc Music". `cargo run -p xtask -- music-font` builds it.
- gabc-rs contains no code from Gregorio, which is under the GPL.
