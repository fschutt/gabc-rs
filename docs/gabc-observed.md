# What Gregorio does with GABC, as observed

gabc-rs is written without Gregorio's source. Where the public description of the format (https://gregorio-project.github.io/gabc/) leaves a question open, the answer comes from giving Gregorio a small piece of GABC and reading what it prints: its exit status, its messages, and its structural dump (`gregorio -F dump`). The `Probe` workflow does that; `gh workflow run probe.yml -f probes='(c4) A(f) B(g)'` asks one question per line.

This file lists what was observed in November 2026 with Gregorio 6.1.0, as Ubuntu packages it for GitHub's `ubuntu-latest` runner (the version is what `gregorio --version` prints in the workflow's log). Each row is a probe that was run. "Error" means Gregorio exits with a failure; the parser's code for the same thing is in the last column.

The second source is the reference run (`Corpus` workflow): all 18,740 GregoBase scores through both parsers. After the rules below, the two verdicts agree on 99.89% of them.

## Header

| Input | Gregorio | gabc-rs |
|---|---|---|
| `mode: 5;` | value `5` | |
| `mode: 5;;` | value `5` | |
| `multi: a` newline `b;;` | value is both lines | |
| `book: abc` newline `mode: 5;` | one header `book` with the value `abc⏎mode: 5`: a value runs to the next `;`, over line ends | W104 |
| a last entry with no `;` before `%%` | error | E003 |
| `a: b; odd name.x: 12;` on one line | accepted: entries are not tied to lines, and a name may be anything | |
| `staff-lines: 5;` | pitches `n` and `p`, clef line 5 and the bars `;7` `;8` become valid | |

## Text

| Input | Gregorio | gabc-rs |
|---|---|---|
| `A$(x$)(f)` | text `A(x)`: `$` takes the next character as it stands | |
| `a$<i$>b(f)` | text `a<i>b` | |
| `x<y(g)`, `<(,)` | a `<` that starts no tag is text | |
| `<foo>x</foo>(g)`, `<u>d</u>` | printed as text: the tags are `b i sc ul tt c e eu nlba sp v alt pr clear` | W106 |
| `<i>A(f) B(g) C</i>(h)` | italic on all three: a tag stays open over syllables | |
| `<i>A(f) B(g) C(h)` | accepted, italic to the end | W107 |
| `x</i>(g)` | error: style not started | E021 |
| `<b>A<i>B</b>C</i>(f)` | accepted: tags need no order | |
| `<v>\star</v>(,)` | verbatim TeX | W101 |
| `<v>(</v>`, `<sp>(</sp>`, `<alt>All (standing):</alt>` | the parenthesis is text | |
| `<sp>[</sp>`, `<alt>[x]</alt>` | the bracket is text | |
| `<c>x(y)</c>O(f)` | error: in other tags a parenthesis starts notes | E011 |
| `<v>open A(f)`, `<sp>open`, `<alt>open` | error | E023 |
| `<v>a$b</v>` | `ab`: the escape works in verbatim text too | |
| `<v>$\star$</v>(,)` | error: `$<` makes the `<` of `</v>` text, so the tag stays open | E023, E024 |
| `<pr>B(g)`, `<pr:1>C(h)`, `<clear>A(f)` | tags that stand alone | |
| `B[tr](g)` | translation `tr` | |
| `B[tr]C[tr2](g)`, `A[x]<i>B</i>(f)`, `<i>A[x]</i>(f)` | error: nothing follows the translation | E027 |
| `[unclosed(g)` | error | E022 |
| `A{b}c(f)` | `b` is the forced centre | |
| `A{b(f)`, `Ab}c(f)`, `A{b}{c}(f)` | accepted | |
| `ct{<e>o</e>}(d)` | accepted | |
| `n<e>{u}m</e>(j)` | error: forced centre within an elision | E026 |
| `A(f) B` | error: text after the last notes | E024 |
| `<eu>A(f) B(g)</eu>` | error, for the same reason: the closing tag is text with no notes | E024 |
| `A(f) B(g))C(h)` | the stray `)` is text | |

## Which letters stand under the first note

Gregorio's dump marks the letters of a syllable that it centres under the first note. Every row was probed; `layout::vowel_segment` gives the same.

| Syllable | Centred | |
|---|---|---|
| `Dó` `mi` `nus` `um` `AL` `Ísra` | `ó` `i` `u` `u` `A` `Í` | the first vowel |
| `quo` `qui` `guis` `cui` `cuí` `uí` `huic` | `o` `i` `i` `i` `í` `í` `i` | a `u` before another vowel is a consonant |
| `ius` `iam` `ia` `Ie` | `u` `a` `a` `e` | so is an `i` before another vowel |
| `cae` `quae` `guae` `aé` | `ae` `ae` `ae` `aé` | a diphthong is centred whole |
| `lau` `áu` `coe` `oé` `eu` `seu` `hei` | `au` `áu` `oe` `oé` `eu` `eu` `ei` | |
| `cœ` `præ` `ǽ` `Æ` `y` | `œ` `æ` `ǽ` `Æ` `y` | |
| `qu` | `u` | with no vowel after it, the `u` is one |
| `str` `*` | `str` `*` | no vowel: the whole text |
| `Ps.` `1.` `et.` | `Ps` `1` `e` | punctuation at the end stays outside |

exsurge, whose line layout gabc-rs otherwise follows, takes `ui` as a diphthong (`qui` centred on `ui`). Gregorio's reading is used.

## The first syllable

| Input | Gregorio | gabc-rs |
|---|---|---|
| `A(f) B(g)` | accepted with no clef | W103 |
| `(c4) (c3) A(f)`, `(c3)(c3) A(f)`, `() (c4) A(f)` | error: clef change on the first syllable | E018 |
| `(c4) A(c3) B(f)`, `(c4) *(c3) A(f)` | error, with text too | E018 |
| `(c4) (c3f) A(g)`, `(c4) A(c3f)` | accepted: the clef is not alone | |
| `(c4) A(f) (c3) B(g)` | accepted | |
| `(c4) A(z-) B(g)` | error: line break on the first syllable | E018 |
| text, then `(c3)` as the first group | accepted: it is the initial clef (seen in GregoBase) | |

## Clefs and bars

| Input | Gregorio | gabc-rs |
|---|---|---|
| `(c1)` to `(c4)`, `(f1)` to `(f4)`, `(cb3)`, `(fb4)` | clefs | |
| `(c5)` | error on four lines, valid on five | E013 |
| `(c4@f2)` | a clef with a secondary clef | |
| `(f c3 g)` | a clef among the notes | |
| `` (`) `` `` (`0) `` | virgula, virgula above the staff | |
| `(^)` `(^0)` | divisio minimis, and above the staff | |
| `(,)` `(,0)` | divisio minima, and above the staff | |
| `(;)` | divisio minor | |
| `(;1)` to `(;6)`, `(,1)` to `(,6)` | Dominican bars 1 to 6; the comma form is the same bar | |
| `(;7)` `(;8)` | error on four lines, valid on five | E016 |
| `(;9)`, `(;0)` | error | E011 |
| `(:)` `(:?)` `(::)` | divisio maior, dotted, finalis | |
| `(,')` `(;_)` `(::')` | a vertical or horizontal episema on the bar | |
| `(g';h)` | a bar among the notes | |

## Notes

| Input | Gregorio | gabc-rs |
|---|---|---|
| `fv fV fo fO fw fW fq fs F -f` | virga, virga reversa, oriscus, oriscus scapus, quilisma, quilisma, punctum, stropha, punctum inclinatum, initio debilis | |
| `fvv fvvv fss fsss` | bivirga, trivirga, distropha, tristropha | |
| `fr` | hollow punctum | |
| `fR` | punctum between lines (linea punctum); `fr0` the same, hollow | |
| `f=` | linea | |
| `F0 F1 F2` | inclinatum descending, ascending, standing | |
| `fo0 fo1`, `fO0 fO1` | oriscus descending, ascending | |
| `f~ f< f>` | deminutus, auctus ascending, auctus descending | |
| `f. f..` | mora, two morae; `f.0 f.1` below and above | |
| `f' f'0 f'1` | vertical episema, forced below and above | |
| `f_ f_0 f_1 f_2 f_3 f_4 f_5` | horizontal episema: automatic, below, above, not bridged, small left, centre, right | |
| `fr1` to `fr8` | signs above the note: accentus, reversed, circulus, semicirculus, reversed, musica ficta flat, natural, sharp | |
| `fx fy f#`, `fx? fy? f#?`, `f##` | flat, natural, sharp; in parentheses; soft sharp | |
| `ixxi`, `f#x` | one accidental; the last letter wins | |
| `gxwf` | a quilisma on `g`, no flat: a later shape wins over `x` | |
| `ix_[oh:h]iv`, `ix~f` | the signs stay with the accidental | |
| `fr9 f_6 F3 fo2 f'2 f.2 g0h hv0` | error: unrecognized digit | E011 |
| `(n)`, `(p)` | error on four lines | E016 |
| `(o)` | error: `o` is a shape, not a pitch | E011 |
| `(f+)` | a custos at `f` | |
| `(z)` `(Z)` `(z0)` | line break, ragged line break, automatic custos | |
| `(--f)`, `(f-)` | error | E011 |

## Signs away from their note

| Input | Gregorio | gabc-rs |
|---|---|---|
| `(g.[oh:h].)` | two morae: a sign after an attribute belongs to the note | |
| `(fhf/.gv)`, `(f/')`, `(dh!'iv)` | accepted, the sign is dropped | W108 |
| `(.)`, `(')`, `(_)`, `(~)` | error: nothing to put the sign on | E011 |
| `(f/_)`, `(f/~)`, `(f!~)`, `(f/v)`, `(f/x)` | error | E011 |
| `(gh__)` | the episema lies over both notes | |
| `(g_h_)`, `(g_[oh:h]h__)`, `(gxh__)` | accepted; an accidental counts as a note here | |
| `(g/h__)`, `(g!h__)`, `(gh___)` | error: more episemas than notes they can lie over | E017 |
| `(g_0_1h_)` | accepted: `_0` and `_1` are below and above the same note (seen in GregoBase) | |
| `(gh~])`, `(f]g)` | a stray `]` is ignored | W108 |

## Spaces, joins, fusion, second voice

| Input | Gregorio | gabc-rs |
|---|---|---|
| `(f g)`, `(f/g)`, `(f//g)` | glyph space, neumatic cut, larger cut | |
| `(f/[2]g)`, `(f//[-0.5]g)` | a space of that size | |
| `(f[-0.5]g)` | error: the size needs a space before it | E012 |
| `(fgh/!ijk)` | a space inside one neume | |
| `(f!/g)` | a cut at which no line is broken; `!//`, `!/0`, `!/[n]` occur in GregoBase and are accepted | |
| `(!f)`, `(g!!h)` | accepted | |
| `(g!)`, `(!)` | error | E011 |
| `(f@g)`, `(@fg)`, `(@[fd])`, `(f@[fdfdc~])` | fusion | |
| `(g@@h)`, `(f@)` | error | E011 |
| `(f{ix}g)` | the part in braces takes no room (second voice) | |
| `(f{g)`, `(f}g)` | accepted | W109 |
| `(f<nlba>gh)`, `(f</nlba>g)` | start and end of a stretch without line breaks | |
| `(g(h)` | error | E010 |

## Attributes

| Input | Gregorio | gabc-rs |
|---|---|---|
| `alt cn cs ev gv hl ll nocustos nv ob ocb ocba oh oll oslur shape ub uh ull uslur` | accepted | |
| `[zz:1]`, `[nm:1]`, `[ubb:1;2mm]`, `[e:x]` | error | E012 |
| `(f[ob:1{]g[ob:1}])`, `(f[oh:h{]gh[oh}])`, `([oll:1{1]lvKJ[oll:}])` | a stretch from `{` to `}` | |
| `A(f[ob:1{]) B(g[ob:1}])` | the stretch may end in a later syllable | |
| `(f[ob:1{]g)` | error: unclosed | E019 |
| `(f[oh}])` | error: end with no start | E019 |

## NABC

| Input | Gregorio | gabc-rs |
|---|---|---|
| `(g\|vi)` with no `nabc-lines` | fatal error | E030 |
| `nabc-lines: 1;` and `(h\|ta\|pe)` | `ta` is NABC, `pe` is GABC again | W102 |
| `(g\|)` | an empty NABC snippet is accepted | |

## Known differences

Of 18,740 GregoBase scores, 20 still get different verdicts:

- Three single scores that Gregorio accepts and gabc-rs rejects: a group that runs over a line end with a sign on the new line (`(...⏎~)`), a `Y` after a note, and `([[hg]])`.
- Two scores where a ledger-line stretch crosses a `!` or `/`: Gregorio reports "termination without start", gabc-rs accepts.
- One score with a forced episema on a note that already has an automatic one: Gregorio rejects, gabc-rs accepts.
- Fourteen scores of one transcriber that Gregorio rejects with "syntax error" and gabc-rs accepts; the cause is not found yet.
