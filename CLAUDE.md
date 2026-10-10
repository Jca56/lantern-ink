# Lantern Ink

Alva's SVG editor: the vector sibling of Lantern Studio 3 (LS3), on Rust +
wgpu + Lantern UI 2, with our own vector code. It is built the way LS3
was: a headless core first, then an MCP server (so Claude can make and
edit SVGs), then the LUI2 window in LS3's look, then the live bridge.

## Read first
- `docs/ARCHITECTURE.md`: the design: crates, the document (the SVG's own
  tree), Commands and snapshot undo, the CPU renderer, the MCP tools, the
  live bridge, the GUI, testing, milestones, and the decisions (§12, the
  ones already made marked ✅).
- LS3 is the reference for the look, the MCP server and the bridge:
  `~/Projects/lantern-studio-3` (`docs/ARCHITECTURE.md`,
  `docs/m0/mcp-protocol.md`, `crates/studio-tools`, `crates/studio-app`).

## Status
- **M0 (architecture) is done**, 2026-10-05: Alva approved the doc and
  decided D1–D10 as recommended. D25 (Boxy SVG's marks are stripped) is
  hers too, the same day.
- **M1 (core) is built**, 2026-10-06. Four crates, 135 tests, clippy-clean:
  - `ink-geom`: paths that keep their segments' kinds (line, quadratic,
    cubic, arc), path data read and written, affines, flattening (arcs
    holding their exact area), strokes (joins, caps, dashes), exact
    bounds.
  - `ink-doc`: the lossless XML reader and writer, the node tree and its
    IDs, the edits (set an attribute, insert, delete, move, each keeping
    the file's own indentation), Commands applied all-or-nothing,
    `Document::adopt` (D25), and the typed views (style cascade, colours,
    lengths, transforms, shape geometry, viewport, gradients, drop-shadow
    filters, `url(#…)` lookups).
  - `ink-render`: the scene builder, the exact-coverage rasterizer
    (`coverage.rs`), gradients, group layers, clip paths, drop shadows,
    bands on all cores.
  - `ink-core`: open, new, apply with snapshot undo (each step with its
    label and Actor), derived dirty state, atomic save, render, PNG
    export.
  - M1's done-test passes on all 144 corpus files:
    `ink-core/tests/m1.rs`.
  - **Not drawn yet:** `<text>`, `<style>` rules and classes,
    `feGaussianBlur`, `<use>`, masks, patterns, images, markers. That is
    `lntrn-svg`'s set exactly; the first three are due in M3 (and are
    drawn now: rules and blurs since M3b, text since M3d).
  - **Found on the way** (other projects, so told to Alva, not touched):
    LS3's vector rasterizer counts a pixel twice where a stroke's pieces
    overlap in part of it (the inside of a curved stroke comes out
    heavier); `lntrn-svg` and `rsvg-convert` both lose a drop shadow that
    is thrown into the picture from past its edge.
- **M2 (headless MCP) is done**, 2026-10-06: built, and its done-test
  passed in a fresh session (a session's tools are fixed when it starts):
  1. ✅ `lntrn-mcp`, a new crate in LUI2 (D11, LUI2 U082; approved by
     Alva: that crate, and nothing else there without asking): JSON-RPC
     lines, a `Server` around a `Host` (both protocol eras), schemas,
     `Args`, `Reply`, `stdio::serve`, `Log`. LS3's protocol transcripts
     run against it.
  2. ✅ `ink-tools`: 17 tools (`doc_new`, `doc_open`, `doc_list`,
     `doc_info`, `doc_source`, `doc_preview`, `doc_save`, `doc_export`,
     `doc_close`, `node_add`, `node_add_svg`, `node_set`, `node_move`,
     `node_delete`, `history_undo`, `history_redo`, `batch` with
     "@names"). Transcript tests: `ink-tools/tests/transcripts.rs`.
     Attributes are as the file writes them (D16); `doc_info` gives each
     node's box in the document's coordinates (exact under any
     transform: `Path::bounds_through`).
  3. ✅ `ink-mcp` → `~/.lantern/bin/lantern-ink-mcp`: stdio, no GPU, logs
     to stderr and `~/.lantern/log/lantern-ink-mcp.log`, previews in
     `~/.lantern/cache/lantern-ink/previews/` (cleared after a day),
     unsaved drawings autosaved to
     `~/.lantern/config/lantern-ink/autosave/` when idle 5 s, once a
     minute while busy, and at EOF (`ink_core::Autosave`).
  4. ✅ Registered 2026-10-06 (user scope, `alwaysLoad`, as server `ink`:
     tools appear as `mcp__ink__…`; `mcp__ink` allowed in
     `~/.claude/settings.json`). Smoke-tested over real stdio, and
     `claude mcp get ink` connects.
  5. ✅ The done-test passed 2026-10-06: a fresh session drew a lantern
     icon with only the tool descriptions to go by, looked at it with
     both renderers, saved it and exported it (Alva's copy:
     `~/Pictures/lantern.svg`). Opened again, edited, undone and saved,
     it gave the same bytes; the refused calls each named their fix.
  - **The done-test's seven findings were dealt with 2026-10-06** (six
    fixed, one kept on purpose). The rules they left behind:
    - A schema says what the server takes: an attribute's value is a
      string or a number (null too in `node_set`), a batch step's `args`
      a plain object (`input::common::value`, `any_object`).
    - A file has one drawing at a time (`Core::doc_at`,
      `CoreError::AlreadyOpen`): `doc_open` answers with the drawing
      that's open already, and `doc_save` won't take another open
      drawing's file.
    - The lantern strip is its own size: the reply gives it, and says so
      when `max_edge` was asked for.
    - Each preview has a numbered file of its own, and a run keeps its
      newest 32 (`Env::next_preview`).
    - Markup put in is laid out like the file all the way down
      (`ink-doc/src/layout.rs`), but never among words (`<text>`,
      `<title>`, `<style>`) or in what isn't SVG's.
    - `doc_info` lists what a `<defs>`, a gradient or a filter holds in
      the file's order, each definition with its attributes.
    - Kept on purpose: a new drawing declares `xmlns:ink` before
      anything uses it (ARCHITECTURE §3.3: a drawing Ink made carries
      it).
  - `doc_preview` with `renderer: "lantern"` draws the drawing with
    `lntrn-svg` itself at 16, 24, 32, 48 and 64 px, each enlarged pixel
    for pixel (ARCHITECTURE §5.4).
  - **Deploy:** `cargo build --release --workspace`, then `install` the
    binary to `~/.lantern/bin/lantern-ink-mcp.new` and `mv` it over.
  - `~/.lantern/bin/lantern-ink` was the May 2026 iced prototype's
    binary until M4a's window took the name (Alva, 2026-10-07).
- **M3 (operations) is done** (all five slices built 2026-10-07; its
  done-test ran the same day and what it found was fixed: see the end
  of this section), in five slices, in the order Alva
  chose 2026-10-06: **a** structure and transforms, **b** paint (styles,
  gradients, clips, filters; the renderer learns `feGaussianBlur` and
  `<style>` rules), **c** paths (anchors, editing, boolean ops), **d**
  text, **e** tidy. D13 (bake a move into the geometry whenever that's
  exact; groups pass it down), D14 (a property is written where the node
  has it), D15 (three decimals) and D18 (anchors have stable ids) are
  decided; D19 waits for its slice.
- **M3a is built** (2026-10-06): `Command::Transform`, `Duplicate`,
  `Group`, `Ungroup`, and `Move` keeping a node where it shows; the tools
  `node_transform`, `node_align`, `node_duplicate`, `node_group`,
  `node_ungroup`, `node_info`, `doc_query` and `doc_set` (25 tools in
  all). A fresh session's tools are the new binary's; M3a has had no
  done-test in one yet.
  - Where a transform is written is `ink-doc/src/settle.rs`, and its
    rules are ARCHITECTURE §3.4. Its done-test draws the corpus:
    `cargo test --release -p ink-render --test settle` (and `report`, or
    `inspect` with `INK_FILE=name.svg`, each with `-- --ignored
    --nocapture`, to read how far apart the pictures are, file by file
    or node by node).
  - `ink-geom` writes an arc's radii as finely as it takes to keep the
    arc where it was (`data.rs`): one near a half turn is pulled flat
    by its ends being rounded, by twenty times what was rounded away.
  - `ink:decimals` on the root sets a document's precision (D15);
    `doc_set` writes it, and declares `xmlns:ink` where it's missing.
  - What's at a point is `ink-doc/src/hit.rs` (`hit::at`), on
    `ink-geom`'s `Path::contains` and `Path::distance`: front to back,
    through transforms and clip paths, as the renderer would draw it.
    M4's Pointer tool is meant to use the same.
  - `doc_set` with `content: "fit"` puts the root through the transform
    from the old viewBox to the new: how a 500-unit Boxy icon becomes a
    24-unit one in one step.
  - (`SetGeometry` came with M4c's c4, for a shape's own handles;
    `ToPath` with M3c.)

- **M3b (paint) is under way**, in four pieces, the renderer first
  (Alva's order, 2026-10-06): **b1** the renderer, **b2** styles
  (`SetStyle`, a `node_style` tool), **b3** gradients (`gradient_add`,
  `gradient_set`; a user-space gradient moves with its shape, but one
  that other shapes share is never touched: Alva's call), **b4** clips
  and filters as tools (`clip_set`, `filter_set`).
- **M3b's b1 is built** (2026-10-06, 217 tests): filters are chains of
  steps and `<style>` rules are in the cascade (ARCHITECTURE §5.2).
  - A filter's steps are read in `ink-doc/src/filter.rs` and run in
    `ink-render/src/filter.rs` (`filter/blur.rs`, `filter/blend.rs`).
    A plain drop shadow draws the bytes it did before: the kitchen-sink
    goldens didn't change.
  - What `<style>` rules say of a node is kept on the node
    (`Node::rules`, `ink-doc/src/sheet.rs`) and worked out again after
    every Command (`Document::restyle`); `style::prop` reads it. Nothing
    else needs to know rules exist.
  - A second golden, `filters-and-rules.svg`, and a report to read it
    against `rsvg-convert`: `cargo test -p ink-render --test golden
    against_rsvg -- --ignored --nocapture`.

- **M3b's b2 is built** (2026-10-07, 221 tests): `Command::SetStyle`
  (`ink-doc/src/styling.rs`) and the `node_style` tool (26 tools). A
  property's name is one SVG has (a near miss is refused with the name
  it was near), and its value is checked where Ink draws with it.
- **M3b's b3 is built** (2026-10-07, 225 tests): `Command::Define`
  (into `<defs>`, made if missing), the tools `gradient_add` and
  `gradient_set` (28 tools), and a user-space gradient that is one
  shape's alone now moves with it (`Settle::carry`).
  - New gradients go by the painted shape's box unless asked for
    `units: "user"`: a box gradient follows its shape for free and can
    be shared, but can't paint a line with no height.
  - Alva, 2026-10-07: for the rest of M3b each finished piece is
    committed once it's tested and deployed, without asking first.
    (That was for M3b. She said the same of M3c: see there.)
- **M3b is built** (2026-10-07, 232 tests, 30 tools): its last piece,
  b4, is `Command::SetClip` (`ink-doc/src/clip.rs`) and the tools
  `clip_set` and `filter_set`.
  - A clip path that is one node's alone now moves with it too
    (Alva, 2026-10-07; ARCHITECTURE §3.4), so a clipped group passes a
    move down. What a node is drawn with, and what that lets its
    numbers take, is `ink-doc/src/settle/drawn.rs`.
  - `clip_set` takes shapes only (SVG lets a clip path hold nothing
    else Ink draws); to cut several things as one, group them and clip
    the group. `release` puts the shapes back.
  - `filter_set` makes a drop shadow or a blur; any other filter is
    written with `node_add_svg`. Filters don't travel: a filtered node
    takes a move into its numbers and keeps a scale or a turn as a
    transform, so its shadow scales and turns with it.
  - **Ungroup still refuses** a group with a filter, a clip path, a
    mask or an opacity over several children unless told to drop them.
    M4's window will want a friendlier answer.

- **M3c (paths) is under way**, in three pieces (Alva's order,
  2026-10-07): **c1** anchors and editing, **c2** boolean ops, **c3**
  outline stroke and simplify. Her calls the same day: **D18, anchors
  have stable ids** (`A3`), kept in memory and never written; **boolean
  ops keep the real curves** (crossings found, segments cut and kept;
  no flattening to polygons), proven by drawing each result against the
  renderer clipping one shape by the other over thousands of pairs; and
  **each finished piece is committed once it's tested and deployed,
  without asking first** ("M3c path editing", "M3c boolean ops", "M3c
  outline and simplify"), telling her after.
- **M3c's c1 is built** (2026-10-07, 257 tests, 33 tools):
  `Command::ToPath`, `EditPath` and `SetPath`, and the tools `path_set`,
  `path_edit` and `path_op` (`to_path`, `reverse`; the boolean ops,
  outline and simplify join `path_op` in c2 and c3).
  - A path is edited as an `Outline` (`ink-doc/src/outline.rs`): runs
    of anchors, and what joins each to the next (a line, a curve, an
    arc: the segment kinds the file had are the ones it keeps). The
    edits are `PathEdit` (`pathedit.rs`); the Commands are `paths.rs`.
    One segment at a time is `ink-geom`'s `Piece` (split, part, nearest
    point), which c2's boolean ops are meant to stand on too.
  - **Anchor ids** live on the node (`Node::anchors`), are handed out
    by the document's counter (never reused, undo or not), and are
    kept whenever a path's `d` changes without its layout changing
    (the same runs of as many anchors): a transform, a `node_set` of
    `d`, an undo. A copy gets its own. `node_info` lists them.
  - **The anchors kept beside a path are the ones its written `d`
    reads back as** (`Outline::settle`, run by `set_outline`): a
    closed run given its first anchor again at its end (or one a
    rounding away from it) has it once, as the file would say. Without
    that the path came back with every id new. A test writes 4000
    awkward outlines and reads each back
    (`whatever_outline_is_written…` in `outline.rs`).
  - `path_edit`'s edits can name the anchor they make (`as`) for later
    edits of the same call: the tool learns the id by making the edits
    on a copy (`Document::anchors_made`), which a Command making the
    same edits of the same document is sure to match.
  - An edit rewrites the whole `d` in Ink's writing (absolute, spaced,
    to the document's decimals): the file's own relative commands
    don't survive a path edit. Everything else about the element does.
  - Anchors can't be named before they exist: in a `batch`, a path
    made by an earlier step has ids the call can't know yet (only
    `as` names inside one `path_edit`). `path_set` gives a whole
    outline in one go; a wish for "the third anchor" hasn't come up.
- **M3c's c2 is built** (2026-10-07, 282 tests): boolean operations.
  `Command::Boolean { nodes, how }` (`ink-doc/src/boolean.rs`), and
  `path_op`'s `union`, `subtract`, `intersect` and `exclude`. The
  first node named takes the result (made a `<path>`) and keeps its
  place, paint and id; the others are deleted. Each shape counts where
  it shows (through its groups' transforms) and by its own fill rule.
  - The work is `ink-geom`'s, three files: `meet.rs` (where two
    segments cross, touch, or share a stretch), `wind.rs` (how often an
    outline winds round a point, by the curves themselves; and
    `Path::area`, exact), `combine.rs` (cut every outline where it
    meets another, keep the cut edges that have the result on one side
    only, join them into loops). **Nothing is flattened:** a cut arc is
    an arc of the same ellipse, a cut curve the same curve, and cuts
    that didn't end up mattering are joined again.
  - **How it's proven** (all in `cargo test --workspace`, about 30 s):
    `ink-geom/tests/combine.rs` puts 3000 pairs of shapes, 6000 pairs
    where one is a result of the other, and 500 threes through all
    four operations: some 600 000 places are each in the result
    exactly when they should be, and the four results' exact areas
    add up. `ink-render/tests/combine.rs` draws 2000 pairs against
    the renderer clipping one shape by the other (the worst pixel is
    a quarter of a pixel out, which is what a clip is out by).
    `ink-doc/tests/corpus.rs` makes every shape of every corpus file
    one with the next: 3037 pairs, 10 112 results, none refused.
  - **What makes it hold** (each found by a test above, each a rule
    now): a part cut from an arc carries the arc's own ellipse
    (`Piece`'s private `arc`: worked out again from its ends, a half
    turn's centre lands a hundred-millionth away); two segments *meet*
    only where they truly cross or touch, or where an end of one lies
    on the other (two that only run close stay two lines, and a
    crossing too fine to home in on is found by which side each end
    of the close stretch is on); and what's filled either side of an
    edge is looked up from the place along it with the most room, a
    quarter of the way to its nearest neighbour.
  - One tolerance, `TOL` in `combine.rs`: a billionth of the shapes'
    size counts as the same place. When outlines can't be told apart
    even so, the Command is refused (`Tangled`) rather than guessed;
    no test has made it happen since the rules above.
  - A loop of the result thinner than the file can write (half a unit
    in the last decimal) is left out: two shapes drawn to abut whose
    numbers overlap by a rounding leave no seam. Shapes that only
    *nearly* share a side are still two sides: nothing is snapped.
  - A group left empty by the shapes taken out of it stays (tidy is
    M3e's). `batch` no longer reports a node that a later step of the
    same batch took out again.
- **M3c's c3 is built** (2026-10-07, 300 tests), and with it **M3c
  is built**: outline stroke and simplify. `Command::OutlineStroke`
  (`ink-doc/src/stroking.rs`) and `Command::Simplify` (`paths.rs`),
  and `path_op`'s `outline` and `simplify` (with `tolerance`).
  **M3d (text) came next.**
  - A stroke's outline is `ink-geom/src/offset.rs`: a ribbon along
    each piece, a join at each corner, a cap at each open end, made
    one by `combine`. Its edge is a line beside a line and an arc
    beside a circle's arc, exactly; fitted cubics beside any other
    curve, within `tolerance` of the true edge (a two-hundredth of
    the stroke's width unless told: finer makes more anchors). Where the stroke is wider than its line bends (it folds),
    that stretch is short straight slices between the curve's true
    normals instead: a folded ribbon's outline doesn't say what it
    covers.
  - A shape with no fill becomes the outline. One with a fill keeps
    it; the outline is a new path over it (under, for `paint-order:
    stroke`), and if the shape had an opacity, filter, clip path or
    mask, the two go in a group that has it now (Alva hasn't been
    asked about this: it's what Illustrator, Inkscape and Figma do).
    What's measured against the shape's box (a gradient across it, a
    filter's reach) is measured against a bigger box afterwards.
  - Simplify (`ink-geom/src/simplify.rs`) goes corner to corner (a
    turn over 30° stays a corner whatever the tolerance) and says each
    smooth stretch with one line, one circle's arc or one fitted
    cubic if that stays within the tolerance both ways. No anchor
    moves; the ones left keep their ids. The default tolerance is a
    five-hundredth of the path's size.
  - **How they're proven:** `ink-geom/tests/outline.rs` holds the
    outline to the stroke's own definition at 140 000 places (round
    all over: everywhere within half the width of the line; cut off
    square: wherever a square-on line from it reaches); 
    `ink-render/tests/outline.rs` draws 1500 strokes beside the
    renderer's (0.06 of a pixel apart where the line isn't cut off
    square) and outlines all 830 strokes in the corpus that aren't
    measured against a box (each file draws as it did, to 0.05 of a
    pixel); `ink-geom/tests/simplify.rs` measures 1200 simplified
    paths against what they were.
  - **What the proofs changed in the core** (`combine.rs`, `meet.rs`):
    the same line is now anything within the tolerance between the
    same two corners, and the look to either side of an edge steps
    past the edges it stands for; a cut in one piece is carried to
    every piece that shares that stretch; a cut corner sits on the
    straight line it cuts; and an edge that can't be told from its
    neighbour and is shorter than a file can say is one corner.
    `Tangled` says what couldn't be worked out. All the boolean
    proofs pass as before.
  - **The renderer changed too** (found by drawing strokes beside
    their outlines; goldens looked at and kept again): a stroked
    line is flattened for its stroke (`flatten_to_stroke`: true
    directions at the ends of curves, no chord turning further than
    the stroke's edge can take), dashes are cut from the curves
    (`Path::dashed`), and a pixel with alpha 0 has no colour (blur
    tails were different bytes from band to band, under an alpha of
    0). Still approximate, and written down in ARCHITECTURE §5.1: a
    wide line cut off square on a tight bend.
  - **Known limit:** outlines that come within a billionth of their
    size of each other without being the same line (files written
    with nine to eleven significant digits can do it) may be refused
    (`Tangled`) rather than guessed. Nothing in the corpus does.

- **M3d (text) is under way**, in three pieces, the renderer first:
  **d1** the renderer draws `<text>`, **d2** `SetText` and the tools
  `text_add`, `text_set` and `font_list` (and a text taking a move into
  its `x` and `y`, and saying which font it ended up in), **d3**
  `TextToPath` and `text_to_path`. Alva's calls, 2026-10-07: **plain
  text and `<tspan>`s** (text on a path and characters placed one by
  one stay in the file, not drawn, until something needs them); **the
  generic families are Lantern's fonts**, not fontconfig's; **one new
  method in LUI2's `lntrn-text`** (`line_glyphs`, U085); and **each
  finished piece is committed once it's tested and deployed, without
  asking first** ("M3d text rendering", "M3d text editing", "M3d text
  to path"), telling her after.
- **M3d's d1 is built** (2026-10-07, 324 tests): the renderer draws
  `<text>`. ARCHITECTURE §5.5 is the design.
  - The fonts are `ink-doc/src/fonts.rs`: the machine's, in one
    `lntrn-text` engine for the process. `sans-serif` is the desktop's
    font from `lantern.toml` (Inter where it names none; `ink-mcp`
    reads it at start), `monospace` JetBrains Mono, `serif` the first
    installed of a short list. Two weights (600 up is bold): that's
    `lntrn-text`'s limit.
  - A text is set by `text::lay` (`ink-doc/src/text.rs`,
    `text/font.rs`, `text/lay.rs`): each element's glyphs as a `Path`
    in the text's coordinates, and the box round the glyphs' cells.
    The renderer, clip paths, `hit::at` and `page_bounds` all read
    that; `geometry::outline_of` is a shape's outline or a text's.
  - `doc_info`'s box for a text is round its glyphs (its ink), like
    a shape's; what a gradient is measured across is the cells' box,
    as SVG says. Two boxes on purpose.
  - A text Ink can't set (`text::Unset`: on a path, characters placed
    one by one, top to bottom, stretched) isn't drawn, and `doc_info`
    says why. A `<tspan>` has no box: it's part of its text.
  - **Tests set text in "Ink Test"** (`tests/fonts/`, made by
    `ink-doc/tests/font.rs`: boxes and a ring, no kerning; its header
    says every glyph's numbers). Never test a box or a picture in a
    real font: it's another machine's failure. After changing the
    fonts: `INK_BLESS=1 cargo test -p ink-doc --test font`.
  - A heart or an emoji comes back from `lntrn-text` as a picture
    (its fallback order puts the colour font first): `fonts::shape`
    asks again in families that draw outlines. Ink draws no pictures.
  - **Not yet:** `clip_set` still takes shapes only, though a clip
    path with a text in it is drawn.
  - **Found on the way:** LUI2's `lntrn-text/src/engine.rs` is 545
    lines (it was 541 before `mod glyphs;` went in): past its 500, told
    to Alva.

- **M3d's d2 is built** (2026-10-07, 331 tests): `Command::SetText`
  (`ink-doc/src/lettering.rs`) and the tools `text_add`, `text_set`
  and `font_list` (36 tools in all).
  - Lines are `<tspan>`s back at the text's `x`, `dy` ems down; a
    stretch with its own paint or lettering is a `<tspan>`; an empty
    line is the space before the next. `SetText` replaces everything
    in the text.
  - **A text takes a move into its `x` and `y`**, and its lines'
    (`settle/lettered.rs`): all of them or none. A turn or a scale
    stays its `transform`; so does a move when a position is `50%` or
    `2em`, or its paint would stay behind.
  - A family that isn't installed is written as asked and reported:
    `text::lettered` says which font each part of a text ended up in,
    and the tools' replies and `node_info` pass it on.
  - `text_set` doesn't move a text: `node_transform` and `node_align`
    do, since they know its lines.
  - `ink` is registered with Claude Code on genforge too (2026-10-07,
    user scope, `alwaysLoad`, `mcp__ink` allowed), as on Alva's other
    machine.

- **M3d's d3 is built** (2026-10-07, 335 tests, 37 tools), and with it
  **M3d is built**: `Command::TextToPath` (`ink-doc/src/outlined.rs`)
  and the tool `text_to_path`. **M3e (tidy) is next**, then M3's
  done-test in a fresh session.
  - A text painted one way becomes one `<path>` in its place (its id
    and paint kept); one whose spans paint for themselves becomes a
    `<g>` of paths. What only letters read is taken off.
  - A text set in another font than it asks for is refused unless
    `as_drawn`: its paths would be the other font's for good. (Alva
    hasn't been asked about this guard: it follows "a missing font is
    reported, not silently swapped".)
  - **What it changes, and can't help:** a box with the text in it
    gets tighter (cells before, outlines after), so a gradient across
    the text or a group's glow region shifts a little. Proven by
    drawing: `cargo test -p ink-render --test text -- --nocapture`
    (13 of the 19 corpus files unchanged to 0.002 levels; the casino,
    arcade and cyberpunk folders 0.04 to 0.20 apart).
  - **For the DE's icons:** the 19 corpus icons with `<text>` show no
    words in Lantern's apps (`lntrn-svg` draws none). `text_to_path`
    is the fix, but those icons live in other projects: Alva's to say.

- **M3e (tidy) is under way**, in three pieces: **e1** `Tidy` and
  `doc_tidy`, **e2** the clean `.svg` from `doc_export`, **e3** D19's
  `ink:label` and `ink:locked`. Alva's calls, 2026-10-07: **tidy drops
  only what nothing uses**, with comments, unreferred ids and
  titles going only when asked by name; **an exported `.svg` is a
  clean copy to ship** (tidied, comments and Ink's marks out,
  formatting kept, the drawing untouched); **D19: label and lock now,
  guides with the window (M4)**; and **each finished piece is
  committed and pushed once it's tested and deployed**, telling her
  after.
- **M3e's e1 and e2 are built** (2026-10-07, 343 tests, 38 tools):
  `Command::Tidy { also }` (`ink-doc/src/tidy.rs`), the tool
  `doc_tidy`, and `doc_export` to an `.svg` (`tidy::shipped`,
  `Core::export_svg`).
  - Tidy goes round until nothing more is unused: a gradient only an
    unused gradient built on, a group left empty by what went. What a
    `<style>` sheet names (anything after a `#`) counts as used.
    `xmlns:ink` is never dropped by Tidy; the clean copy drops it.
  - `doc_tidy` is a direct tool, not a batch edit: it says what went
    from the drawing as it was (afterwards those nodes aren't there
    to name).
  - A comment alone on its line takes the line with it, but among
    words (`<text>`, `<style>`, `<title>`, unknown elements) only the
    comment goes: the space may be part of what's said.
  - The clean copy finds every `ink:` attribute before it takes any
    declaration out (a test caught it leaving nested ones in when the
    root's declaration went first).
  - **Proof:** `cargo test -p ink-render --test tidy -- --nocapture`
    (every corpus file, tidied and shipped, draws the same bytes).

- **M3e's e3 is built** (2026-10-07, 349 tests, 39 tools), and with it
  **M3e and all of M3**: D19's `ink:label` and `ink:locked`
  (`ink-doc/src/marks.rs`), `Command::SetLabel` and `SetLocked`, and
  the tool `node_mark`. M3's done-test came next (it ran the same
  day: see below). M4, the window, came after (see the end of this
  section).
  - A locked node, and everything in it, refuses every Command but
    locking and unlocking: `Document::guard` runs before each one (in
    a batch, as each step's turn comes). What would move, rewrite or
    remove a locked node from above (deleting, transforming or
    ungrouping the group it's in) is refused too; putting things beside
    it, painting its group and copying it are not.
  - **A new Command needs an arm in `guard`** (the match is
    exhaustive, so the compiler says so): say whether it changes its
    nodes themselves (`Own`) or what's in them too (`Deep`).
  - Tidying steps round what's locked; a clean copy to ship carries
    neither mark.
  - Over the wire a lock's refusal ends "ask her first": Alva locks
    what she doesn't want changed. A label comes off with `""`
    (`lntrn-mcp` reads a null argument as not given).
  - Guides wait for M4 (Alva, D19): nothing can show or snap to them
    yet.
  - **`ink-doc/src/command.rs` is 472 lines:** the next Command wants
    `run`'s arms moved to a file of their own first (500 is the flag).
  - The server's instructions (2048 characters, Claude Code's limit)
    are full: the text, tidy and mark tools are found by their own
    descriptions, not named there.

- **M3's done-test passed** 2026-10-07 in a fresh session: a stress
  sheet (Alva's choice), one cell per tool family, with only the tool
  descriptions to go by. All 39 tools were called. Everything §11 asks
  of M3 works over MCP: the numbers checked by hand were exact, the
  sheet draws the same in `rsvg-convert` (but for text with no family
  and italic Lexend, below), every refusal said why, the locks held
  against 17 tries, and a file opened again, edited, undone and saved
  gave the same bytes (the sheet, and a copy of the app icon, whose
  Boxy marks went as D25 says). Alva's copies:
  `~/Pictures/ink-m3-stress.svg`, its clean copy `-clean.svg` and a
  `.png`.
- **What it found was fixed the same day** (Alva: fix them now; 353
  tests, deployed, and the deployed binary driven over stdio through
  each finding). The rules they left behind:
  - **Simplify shares a heading at every join** inside a smooth
    stretch (`simplify.rs`, `through`): half way between the one it's
    reached with and the one it's left with. A wave of 26 short lines
    was 12 anchors of arcs, lines and pinched cubics at the default
    tolerance, and is 4 smooth cubics now. A curve whose handles pass
    each other is no fit.
  - **A shape with no inside has no fill to keep** when its stroke is
    outlined (`stroking.rs`, `flat`): a `<line>` becomes the outline
    itself, with its id, instead of staying behind unseen.
  - **What would paint nothing is refused** by `SetStyle`
    (`styling.rs`, `fits`): a `url(#…)` naming nothing (unless the
    paint has a colour to fall back on), and a gradient measured by a
    box on a shape with no height or width. `gradient_add` and
    `node_style` both go through it; `node_set` writes what it's
    given, as ever.
  - **A lock holds what its node is drawn with** (Alva's call):
    `Document::held` in `marks.rs` finds every gradient, clip path
    and filter a locked node names (in an attribute or by a `<style>`
    rule), and what those name; `reach` refuses a change to one, to
    what's in one, or to what holds one. What only unlocked nodes use
    is free.
  - **A copy of a locked node isn't locked** (Alva's call), nor is
    anything in a copy: `duplicate` takes the locks off it.
  - **A refusal's hint is the tools' to add** (`input.rs`, `hint`):
    the document says why in words the window can use ("say to drop
    it"), and the tool layer adds the argument or the tool (`drop:
    true`, `as_drawn: true`, `text_to_path` for a text that isn't a
    shape, the note on locks).
  - **`Applied::lost`** says what a Command was told to let go (an
    ungroup's opacity or clip path): the reply names it.
  - **`doc_info` says its listing once**, in the text (as data it was
    every node over again: about half its size), and with `node_id`
    lists one node and what's in it. `node_info` names what's
    directly in a node. `node_add_svg` says so when what it added has
    things inside.
  - **One path's anchors come with the reply** that made it a path
    (`to_path`), turned it round or simplified it: they're what's
    wanted next. `path_edit` on a shape has no anchor to name, and
    says so now.
  - **`batch`** takes "@name" for `node_align`'s `to`, and says the
    `id` each new gradient, clip path or filter ended up with.
  - **`node_delete` says who's left** naming an id nothing has (it
    lists every such id, not only the ones this delete made).
  - **Text:** `text_add` with no font, and none handed down, writes
    `font-family="sans-serif"`; a family with no italic is reported
    upright (`fonts::slants`: an "a" asked for both ways comes back
    the same); `SetText` with no line height keeps the lines as far
    apart as they were (`Document::leading`: the least any line is
    below the last, so a text whose every line follows an empty one
    reads as twice as open).
- **Left as they are, for Alva to say** (small, none in the way of
  M4): a locked node can't be relabelled; `node_add_svg` refuses a
  comment beside elements, and one inside a group stays on its
  opening tag's line; gradient stops out of order are written as
  given; `smooth` and `corner` between two arcs do nothing, silently;
  `font_list` says its families twice (text and data); "1 decimals";
  anchors in a reply's data aren't rounded; `gradient_set` (over
  `SetAttr`) can still put a level line's own gradient back to box
  units; a `<style>` rule that paints a locked node can itself be
  edited; a group that only holds a `<defs>` a locked node uses can't
  be moved (it's refused like one that holds the node).
- **Should Ink slant an italic the family doesn't have?** Browsers
  and `rsvg-convert` do; Ink sets it upright and says so. Not asked
  yet.
- **Seen, no change proposed:** a radial gradient with its focus
  outside its radius is drawn as SVG 1.1 says (focus pulled onto the
  circle; with `repeat` that aliases), where `rsvg-convert` draws
  SVG 2's cone.
- **The app icon is made:** `~/.lantern/icons/lantern-ink.svg`
  (Alva's; the window's copy is `crates/ink-app/assets/lantern-ink.svg`).

- **M4 (the window) is under way**, in six slices, the shell first.
  Alva's calls, 2026-10-07: **the bar is Boxy's replacement** (D20: all
  twelve tools, the object tree, fill and stroke, the icon aids, a menu
  row for every operation the MCP has); **the order** is **a** the
  shell and the viewer, **b** the object tree, the Pointer and undo,
  **c** paint and the shape tools, **d** the Node tool and the Pen,
  **e** text, gradients and the eyedropper, **f** the icon aids and not
  losing work; **the look is copied** into `ink-app` (D12); and **the
  first deploy takes the `lantern-ink` name**. The checklist she ticks
  or strikes is **`docs/M4.md`**: read it before any M4 work.
- **M4a (the shell and the viewer) is built** (2026-10-07, 394 tests,
  deployed; **not yet looked at by Alva**: its boxes in `docs/M4.md`
  are hers to tick). A fifth crate of code, `ink-app` →
  `~/.lantern/bin/lantern-ink`.
  - The window is LS3's shell, copied and cut to what Ink has so far:
    `theme.rs`, `layout.rs`, `chrome/` (logo, toolbar, tabs, status
    bar, panel frame), `camera.rs`, `canvas.rs`, `docs.rs`, `files.rs`,
    `picker.rs`, `lifecycle.rs`, `menus.rs`, `actions.rs`,
    `settings.rs`, `log.rs`. `ink.rs` is the state, `host.rs` the two
    seams with LUI2, `workspace.rs` a frame.
  - **The canvas is tiles** (`tiles.rs`, ARCHITECTURE §8 "as built"):
    levels that change places when the view's tiles have all landed,
    drawn on the pool from a `Plan` (`ink-render/src/plan.rs`, §5.1).
    `page.rs` draws the ground, the checks, the page's edge and the
    tiles.
  - **The core has no GPU, so it's there from the start** (`Ink::new`
    makes it; LS3's waits for `init_gpu`). The window names wgpu
    nowhere: tiles and icons are `lntrn_image::Image`s handed to
    LUI2's `Images` in `after_rebuild`.
  - **A save is written off the window's thread:** `Core::begin_save`
    → `SaveJob::write` on the pool → `Core::saved` (`Core::save` is
    the three in a row, for the MCP server). A file is read on the
    pool (`ink_core::read_text`) and opened with `Core::open_read`.
  - **The window is tested whole without a GPU** (`ink_tests.rs`):
    real shell frames from LUI2's `Harness`, the tiles' pictures kept
    by a stand-in for the GPU's images (`tiles::Store`) that counts
    them. New window behaviour gets a test there.
  - The camera keeps a slow drag's fractions (`origin`) and shows the
    page's corner on a whole pixel (`corner()`): everything drawn over
    the canvas goes through `page_at` / `window_at`, never `origin`.
  - A drawing made here that nothing was done to is *untouched*
    (`Ink::untouched`): no `•`, closes without asking, and gives its
    tab to the first file opened.
  - The four tool icons LS3 has none of (Node, Polygon, Hand, Zoom)
    were drawn with Ink's own MCP tools; all of them are drawn in the
    window by `ink-render`, not `lntrn-svg`.
  - Menu rows that wait for a later slice are `menus::later("…")`:
    greyed, in their final place. Lighting one is giving it an id and
    an arm in `actions.rs`.
  - `ink_core::desktop::use_font` is the one reader of `lantern.toml`'s
    font, for the MCP binary and the window both.
  - **Not in M4a, on purpose:** nothing edits yet (no Pointer, no
    tree, no panels' insides); File > New asks no size (slice f);
    the CLAUDE pill says the bridge is M5.
  - **For M4b, from the tile speed report** (`cargo test --release -p
    ink-render --test speed tiles -- --ignored --nocapture`; five
    minutes, one core): a 4K screen of an icon with no shadows is 25
    to 125 ms of one core; one with drop shadows is 1.5 to 3.5 s (a
    tenth to four tenths of a second on the pool). So a drag can't
    redraw shadowed tiles every frame: gestures (ARCHITECTURE §4.3)
    need what's dragged drawn once and moved, and `Applied` has no
    dirty box yet for "only the tiles an edit touches". The filters
    are where the time goes (every pixel of a tile's margin, clear or
    not, through linear light): the renderer's to speed up, measured
    first.
  - **Deploy:** `cargo build --release --workspace`, then `install`
    each binary to `~/.lantern/bin/<name>.new` and `mv` it over
    (`lantern-ink`, `lantern-ink-mcp`).
  - **The launcher entry** is `deploy/lantern-ink.desktop` (Alva,
    2026-10-08). On a machine without one it goes to
    `~/.local/share/applications/`, and the app icon to
    `~/.lantern/icons/lantern-ink.svg`: the launcher finds `Icon=` there
    by name, and Alt+Tab finds the entry by the window's app id, so the
    file's name stays `lantern-ink.desktop`. Both are installs, Alva's
    to say yes to on each machine (genforge has them, 2026-10-08).
  - The canvas's ground is LS3's light tan (`theme::GROUND`, `#AAA295`:
    its present shader's `SURROUND`; Alva, 2026-10-08). LS3 also
    throws a soft shadow round its canvas; Ink marks the page's edge
    with a line instead, since what's drawn past the edge shows.

- **M4b (the object tree, the Pointer, undo) is under way**, in four
  pieces: **b1** the groundwork in the core (gestures, and tiles kept
  across an edit), **b2** the object tree, **b3** the Pointer, **b4**
  the menus and the Box. Alva's calls, 2026-10-08: **a drag shows the
  real drawing, live and exact** (not a lifted copy, not an outline
  alone); **scaling by the handles keeps a stroke's width**, with a
  "Scale strokes" tick in the Box to turn scaling on; and **each piece
  is committed and pushed once it's tested and deployed, without
  asking first** ("M4b gestures and dirty tiles", "M4b object tree",
  "M4b pointer", "M4b menus and box"), **stopping with a checklist
  after each one that has something to see** (b1 has nothing: the
  first stop is after b2).
- **M4b's b1 is built** (2026-10-08, 403 tests, deployed). ARCHITECTURE
  §4.3 and §8 have the design "as built".
  - **Gestures** are `ink-core/src/gesture.rs`: `Core::begin`,
    `update`, `commit`, `cancel`. A gesture comes to one Command, said
    again whenever the drag moves on (the whole drag so far, never a
    step of it); `update` applies it to a copy of the document, and
    `Core::shown` is what a window draws, with a `Look` that says when
    that's another picture. The document, its history and what's saved
    are untouched until `commit`.
  - **What to draw again is the renderer's to say, not a Command's**
    (`Applied` has no dirty box, and needs none): `Plan::changed_from`
    holds the drawing laid out now against the one that shows. A new
    level takes every tile those boxes don't touch (`tiles.rs`; a
    tile's picture is a `Pic`, shared and freed by its last holder).
  - **A new filter stage has two things to say** in
    `ink-render/src/filter.rs`: how far it looks (`Stage::reach`) and
    whether it can paint where nothing is drawn (`floods`). The tiles
    kept across an edit stand on both.
  - **The proof** is `ink-render/tests/changed.rs` (every eighth corpus
    file with `cargo test`; all of them with `--release -- --ignored`,
    two and a half minutes). **The speed** is
    `cargo test --release -p ink-render --test speed drags -- --ignored
    --nocapture` (four minutes): a step of a drag on a 4K canvas is
    79 ms at the median and 643 ms at worst, all of it tiles under wide
    shadows. **Alva hasn't been shown these numbers' consequence yet:**
    the artwork will trail the pointer on shadowed icons until one of
    §8's three ways is built (her choice, when she has felt it in b3).
  - The window's tests are `ink_tests.rs` (the harness and the shell)
    and `ink_tests/` beside it (`canvas.rs`: edits and gestures on the
    tiles). New window behaviour gets a test there.
- **M4b's b2 is built** (2026-10-08, 416 tests, deployed; **not yet
  looked at by Alva**: its boxes in `docs/M4.md` are hers to tick).
  - **What a tab keeps beside its document** is `select.rs`
    (`Selection`, on `Tab`): the nodes selected and the one in hand,
    which rows are open (groups start open; `<defs>`, texts, gradients
    and the rest start shut), the row being renamed. `tops()` is what
    an action on "the selection" acts on, back to front.
  - **The tree** is `tree/` (`mod.rs` the panel, `rows.rs` a row,
    `drag.rs` where dragged rows land). It draws the drawing as it
    looks (`Core::shown`) and never edits: what it wants comes back as
    `tree::Intent`s, which `edits.rs` carries out
    (`Ink::tree_asked`). A row is named by its `ink:label`, else its
    `id`, else its kind's word (`select::name_of`).
  - **Every edit from the window goes through `Ink::edit`**
    (`edits.rs`): one Command, one step of Alva's, with the label the
    Edit menu shows ("Hide", "Show", "Lock", "Unlock", "Rename",
    "Restack"). A refusal is said in the status bar with nodes called
    what their rows are (`in_row_names`: the document says `N7`).
  - **The eye is `display="none"`** (through `SetStyle`, so written
    where the node has it): hidden in every app, and in the file. The
    padlock is `ink:locked`. A definition has no eye.
  - Rows drag as LS3's layers do, several at once when several are
    selected (`Command::Move`, tried on a copy for every gap the
    pointer passes: where the drawing would refuse, there's no line).
  - **The selection shows on the canvas** as a gold box round each
    selected node (`overlay.rs`; boxes from `geometry::page_bounds`,
    kept per `Look` on the tab). b3 gives it its handles.
  - The tree's pictures are `icons::Glyph`: LS3's two eyes and its
    folder (copied into `assets/icons/`), and the tools' own icons for
    the kinds. The padlock is drawn with lines.
  - **Not in b2:** a row's right-click menu (b4), picking on the
    canvas (b3), a thumbnail or a swatch on a row, scrolling the tree
    to what's picked.
- **M4b's b3 is built** (2026-10-08, 434 tests, deployed; **not yet
  looked at by Alva**). ARCHITECTURE §8 has the Pointer "as built".
  - **`Command::Resize`** is `Transform` with lines left as they are
    (`settle.rs`: `Settle::keep`): what a scale handle does. A stroke
    keeps its width and dashes and a rect its corner rounding (my
    reading of "the stroke keeps its width": Alva chose that, and was
    told of the rounding and of circle → ellipse at b3's handoff).
    `ink-doc/tests/resize.rs` has what the file then says, and holds a
    sixth of the corpus against `Transform`. **Not over MCP yet:**
    `node_transform` always scales strokes.
  - **`command.rs` was split**: the Commands are there, applying one
    is `apply.rs`. A new Command still needs its arm in `guard`.
  - **The Pointer** is `pointer.rs` (`Ink::pointer_tool`, once a frame
    before the canvas is drawn; `pick`; the marquee's `caught`),
    `handles.rs` (the box's geometry and what each drag comes to, by
    itself and tested by itself) and `overlay.rs` (what's drawn over
    the canvas, from a `Scene` the Pointer hands back). Its state is
    `Ink::pointing` (`Ink::pointer` was taken: the status bar's).
  - **A frame of the Pointer reads the drawing, then asks the core**
    (`Ask`): the drawing is borrowed from the core while it's read, so
    what a drag wants (`begin`, `update`, `commit`) is done after.
  - **The Pointer works at one level** (`Selection::within`,
    `context()`): a pick is a child of that level's group. A row
    picked alone in the tree puts the Pointer in that row's group.
  - **The selection's box comes from the committed drawing's boxes**
    (`Tab::boxes`, by `History::stamp`), never a preview's: during a
    drag the box is the box as it was through the drag's affine.
  - **Keys:** `menus::canvas_key` (Escape, the arrows) after the bound
    keys and the tools' letters; mid-drag `Host::key` lets only Escape
    through (`Pointer::busy`).
  - **Cursors** are `cursors.rs`: the desktop's own resize and turn
    arrows from `~/.lantern/icons/cursors/`, drawn by `ink-render`.
    Without them, the system's shapes (and the arrow for a turn).
  - "Scale strokes" is `Settings::scale_strokes`, off; its tick comes
    with the Box (b4).
  - **Not in b3:** a box turned with the one thing in it (the box is
    always upright, so a turned rect scaled unevenly skews); snapping
    (slice f); a right-click menu (b4); dragging with Alt to copy.
- **M4b's b4 is built** (2026-10-08, 450 tests, deployed; **not yet
  looked at by Alva**), and with it **all of M4b**. Next: M4c (paint
  and the shape tools), once she has looked at b4.
  - **`ops.rs`** is the Edit and Object menus' work: `Ink::op(Op, cx)`
    for every row, the keys and the right-click menu
    (`selection_menu`). `Picked` is what the menus need of the
    selection to light their rows. Order (`restack`) moves things past
    the next one in their own group's stack that isn't selected; "to
    the back" is over the definitions, never under them.
  - **Two new Commands' worth in `ink-doc`:** `Command::Paste` and
    `Document::clipping` (`paste.rs`), and `arrange.rs` (lining up and
    spreading out: `ink-tools`' `node_align` uses it too now, so the
    window and Claude can't disagree).
  - **Copy and paste are two frames each** (`Ink::clip_out`,
    `Ink::pasting`): an action has no `Ui`, so what's copied goes to
    `ui.state.set_clipboard` with the next frame, and a paste asks for
    the system's text (`clipboard_wanted`) in one frame and reads it in
    the next.
  - **The Box** is `toolbox.rs` (LS3's frame: the square, the panel)
    and `boxes.rs` (what it holds with the Pointer in hand). A number
    dragged along is `Ink::boxing`: a gesture, like a drag on the
    canvas, with the selection's box on the canvas carried along
    (`Pointer::carried`); while a drag on the canvas goes, the numbers
    read where it has the box (`Pointer::live`).
  - **`controls/`** is LS3's number field and toggle, copied (D12). The
    slider, dropdown and button come when a slice needs them: copy
    LS3's file, don't write another.
  - **Nothing floats over the canvas without `Ui::child` on a layer
    above it:** that's what keeps a press on the Box from being the
    canvas's. (An `interact` across the whole panel took every press
    from the rows in it: drawn first wins.)
  - `Settings` has `scale_strokes` and `align_to_page` now.
  - **Left as they are, for Alva to say:** paste lands where it was
    copied from (no offset, not at the pointer); a copy of something
    inside a group leaves behind what it inherited from the group (its
    place is made up for, its group's paint isn't); pasted `<style>`
    rules are dropped; the clipboard is plain text (no `image/svg+xml`
    without a LUI2 change); `node_transform` has no way to keep strokes
    yet (`Command::Resize` is the window's only).
  - (`pointer.rs` was 480 lines: its picking, `pick` and `caught`, is
    `picking.rs` since c4.)

- **M4c (paint and the shape tools) is under way**, in four pieces:
  **c1** fill, stroke and the picker, **c2** stroke settings, opacity
  and a gradient as a kind of paint, **c3** the shape tools, **c4** a
  drawn shape's own handles. Alva's calls, 2026-10-08: **Fill and
  Stroke are two rows** at the top of the right panel (a swatch, its
  hex, and the kind of paint), with the stroke's settings, the
  opacity, the palette grid and the palettes under them; **Ink starts
  with her LS3 palettes** (read once, then its own file); and **each
  piece is committed and pushed once it's tested and deployed, without
  asking first** ("M4c fill, stroke and the picker", "M4c stroke
  settings", "M4c shape tools", "M4c shape handles"), **stopping with a
  checklist after each**.
- **M4c's c1 is built** (2026-10-08, 461 tests, deployed; **not yet
  looked at by Alva**).
  - **The paint model** is `paint.rs`: `Paint` (none, a colour whose
    alpha is its opacity, a gradient by name), `Paints` (a fill and a
    stroke), `read` (a node as it's drawn: what it says over what it
    inherits), `painted` (the shapes and texts a selection's paint goes
    on, through its groups) and `set` (the properties that say a
    paint).
  - **`painting.rs`** is the window's side: `Ink::paints` is what the
    section shows with nothing selected and what the next shape will
    get; `set_paint` puts a paint on the selection (one `SetStyle`,
    labelled "Fill" or "Stroke") and, while the button is down on what
    chose it, as a gesture (`Ink::painting`).
  - **`colour/`**: `picker.rs`, `palettes.rs` and `drawer.rs` are
    LS3's files, copied and retitled (keep them near LS3's: a fix
    there wants making here); `section.rs` is Ink's own face for them.
    Palettes are `~/.lantern/config/lantern-ink/palettes.json`,
    started from Studio's (`settings::studio_palettes`).
  - The palette grid's swatches are capped at 36 px so a full palette
    of 64 leaves the tree its room.
  - **Not in c1:** the gradient kind and the picker for its stops (c2
    makes one; e edits it), the stroke's width and the rest (c2), the
    node's opacity (c2), a swatch in a tree row.
- **M4c's c2 is built** (2026-10-08, 467 tests, deployed; **not yet
  looked at by Alva**).
  - **Everything the section sets is a `paint::Set`** (a paint, a
    gradient, the line's width, cap, join and dashes, the opacity):
    `Set::properties` is what says it, `Set::label` what its step is
    called, `Paints::take` makes it the next shape's too. `Paints` now
    carries the `Line` and the opacity. `Ink::set_paint(Set, held)` is
    the one way in: a paint and a line go on `paint::painted` (the
    shapes and texts), an opacity on `paint::faded` (the selected
    things themselves).
  - **Held, it's a gesture; let go, it's an edit** (`Ink::painting`):
    a colour, a width and an opacity are dragged to; a kind, a cap, a
    join and dashes are done at once.
  - **`colour/line.rs`** is the rows under Stroke and the opacity
    (their glyphs are drawn with rects and circles: no icons). The
    slider is LS3's, copied into `controls/slider.rs` as a row control
    (`Slider::in_row`): Ink's panels are laid out by hand, and its name
    is the caller's to draw.
  - **A gradient made here** is `gradient-N`, box units, top to bottom
    (`Ink::gradient_for`); `Section::last_gradient` remembers which one
    each paint last was. Its swatch draws its real stops.
  - **The section scrolls and folds** (`painting.rs`: `TREE_SHARE`,
    `TREE_LEAST`; `Settings::paint_folded`): at a 1.4 scale the whole
    of it is taller than the panel.
  - **Not in c2:** editing a gradient's stops or its line across the
    shape (slice e); miter limit, dash offset, `paint-order`,
    `fill-rule` (none asked for); "mixed" where the selection's shapes
    are painted differently (the one in hand is what shows).
- **M4c's c3 is built** (2026-10-09, 475 tests, deployed; Alva drew
  her first picture with it the same day). The first time something
  new can be made in the window by hand.
  - **`shapes.rs`** is what a drag makes (no window in it): `dragged`
    (the two ends, once Shift and Alt have shaped them), `markup` (the
    element, painted as `Paints` says), `ring` (a polygon's or a star's
    corners), and the tools' own `Settings`. **`shaping.rs`** is the
    gesture (`Ink::shape_tool`, `Ink::shaping`): `Command::Insert` at
    `Place::LastIn` of the Pointer's level, previewed each frame.
  - **As LS3's shape tools** (Alva's choices there, kept): a drag
    always draws, the tool stays in hand, the new shape is selected,
    Shift squares and Alt draws from the middle.
  - **New shapes land on whole units** (`shapes::grid_for`, `on_grid`;
    Ctrl frees them): my reading of LS3's "taken to the whole pixel".
    **Alva was told at c3's handoff, and hasn't said**; slice f's
    snapping (half units, a switch) is to replace it.
  - A polygon is a plain `<polygon>`: nothing in the file says how many
    sides it was drawn with. c4 reads that back off its points.
  - The shape tools' settings and `Ink::paints` aren't kept between
    runs yet.
- **The right panel's text is 24 px** (`theme::FONT_PANEL`; its small
  buttons `FONT_PANEL_SM`, 20), up from 20: Alva's call, 2026-10-09,
  after it read small beside LUI2's 25 px menus. Rows are 50 px (the
  tree's 52). **Palette swatches fill the panel's width**, eight
  across, up to 56 px each (they stopped at 36). The section is taller
  for it: in a 1080-px window the tree is at its least share and the
  section scrolls.
- **M4c's c4 is built** (2026-10-09, 488 tests, deployed; **not yet
  looked at by Alva**), and with it **all of M4c**. Next: M4d (the
  Node tool and the Pen), once she has looked at c4. Alva's calls,
  2026-10-09: **one corner dot** on a rectangle (not one a corner),
  and **a polygon given other sides keeps its circle** (its middle and
  its reach; not its box).
  - **`Command::SetGeometry { node, geometry }`** (`ink-doc/src/shape.rs`,
    `set_geometry`): a shape's own numbers, through `Geometry::write`,
    so only what changed is written and the rest stays as the file says
    it. A shape stays the kind it is; no size or radius is less than
    nothing. **Not over MCP:** `node_set` says the same there.
  - **The corner dot** is `rounding.rs` (the geometry, by itself:
    `Rounded`, `dot`, `dragged`) and a `Drag::Rounding` in `pointer.rs`:
    a gesture, landing as "Corners". It stands 28 px inside the
    rectangle's first corner, on from the middle of the corner's arc,
    and follows the pointer along its own line. A rectangle picked
    alone, not locked, and at least 56 px on its shorter side has one;
    the box's own corners and sides win where they meet it.
  - **The dot rounds to whole units** (`shapes::grid_for`), as new
    shapes land; Ctrl frees it. **Alva hasn't been asked:** it goes
    with c3's whole units, and slice f's snapping replaces both.
  - Corners rounder one way than the other (`rx` ≠ `ry`, from another
    program) keep that shape under the dot and the Box.
  - **A polygon is read back off its corners** (`polygons.rs`,
    `Regular::read`): the best-fitting circle (its middle, and where
    its one unit across and down have gone, so stretched, turned and
    leant ones read too), a regular polygon's corners first, then a
    star's. **Any triangle reads as 3 sides and any slanted box as 4.**
    What isn't one (a corner moved, an arrow) has no rows.
  - **Other sides and back is the same polygon to the file's last
    decimal, not always to the byte** (the circle is read off rounded
    numbers; a pentagon 10 across read 10.0005). Undo is to the byte,
    and a polygon set to what it is already isn't touched. **I told
    Alva "exactly" when I asked her; she was told the truth at c4's
    handoff.** Exact would take remembering each circle beside what was
    written from it: not built.
  - **The selected shape's rows are `shapebox.rs`** (`read`, `command`,
    `rows`; `Ink::own_shown`, `tune`, `tune_settled`, `Ink::tuning`):
    the same rows the shape tools show for the next shape, on a
    `shapes::Settings` read off the one in hand; what changed between
    before and after the rows is the `Tune`. As in LS3, it's set on
    every selected shape of that kind (each rectangle as round as it
    can go). A polygon made a star is as deep as the tool's setting.
  - `Host::key` counts `Ink::tuning` among the drags that let only
    Escape through.
  - **Not in c4:** a handle on the canvas for a polygon or a star
    (the Box is where their sides are, as `docs/M4.md` says); rows for
    an ellipse or a line (their numbers are the box's); the dot under a
    shape tool (handles are the Pointer's).

- **M4d (the Node tool and the Pen) is under way**, in four pieces:
  **d1** the Node tool sees and drags, **d2** its edits (a new anchor
  on a segment, smooth, corner, break, join; the Box), **d3** the Pen,
  **d4** the Path menu. Alva's calls, 2026-10-09: **the Pen is LS3's**
  (a click places a corner, a press dragged on pulls its handles out:
  **D17 changed**, which said click only); **Pen points and dragged
  anchors land on whole units, Ctrl frees them** (handles and bends
  are always free); and **each piece is committed and pushed once it's
  tested and deployed, without asking first** ("M4d node tool", "M4d
  node edits", "M4d pen", "M4d path menu"), **stopping with a
  checklist after each**.
- **M4d's d1 is built** (2026-10-09, 500 tests, deployed; **not yet
  looked at by Alva**).
  - **Two things new in the core** (`ink-doc/src/pathedit.rs`):
    `PathEdit::Pull { after, share, to }` (the point `share` along a
    segment taken to `to`: what dragging a segment does; a line or a
    cubic by its two control points, a quadratic by its one, an arc as
    the circle's arc through it) and `Outline::handles(id)` (where an
    anchor's two handles are, if it has them). Neither is over MCP.
    (A line counts its points as the cubic with no handles does, which
    isn't evenly: `Pull` makes up for it.)
  - **The Node tool is three files:** `nodes.rs` (what a press takes:
    a handle of a picked anchor, then an anchor, then a segment; what
    a handle dragged comes to; LS3's sizes), `anchors.rs` (which
    anchors of which shapes, and the Commands that change them),
    `noding.rs` (the tool's frame, `Ink::node_tool`; its state is
    `Ink::noding`). `overlay.rs` draws a path's line, its anchors and
    the picked ones' handles in LS3's look, in place of the selection's
    box.
  - **A click with the Node tool picks the shape itself**, whatever
    group it's in (`picking::top_at`), and puts the Pointer at that
    shape's level. Anchors show for every selected shape that shows and
    isn't locked; a group selected shows none.
  - **A shape that isn't a path yet becomes one with the first change
    to its anchors, not when they're looked at or picked** (my reading
    of the checklist's "first touched": a click shouldn't cost a
    rectangle its corners' dot). So its anchors need names before they
    have any: `anchors::WouldBe` keeps each such shape as the path it
    would be made, alone, and `firsts` says what the anchors are
    called when one change makes several shapes paths (the document
    hands ids out from one counter). An anchor picked on a shape that's
    still no path is called anew whenever the drawing changes
    (`WouldBe::refresh`, by its place in the outline).
  - **Smooth is read, not kept:** SVG has no word for it, so an
    anchor's handles are smooth while they lie in line through it
    (within two degrees). One dragged takes the other round then, each
    keeping its own length; Alt sends it alone, and Shift holds it to
    45°.
  - **The Node tool's keys:** Delete takes the picked anchors out (a
    shape left with none goes altogether), the arrows move them, and
    Escape lets go of them before it lets go of the shape
    (`Ink::anchors_in_hand`, in `actions.rs` and `pointer.rs`).
  - Anchors are drawn from the drawing as a drag has it (`Core::shown`),
    so they're a frame behind the pointer; nothing is pruned from the
    pick mid-drag for that reason.
  - **Not in d1:** a new anchor on a segment, smooth and corner, break
    and join, and the Box's rows for an anchor (all d2); snapping to
    other anchors (slice f). Every frame reads and flattens each shown
    path afresh: fine for icons, to be kept per `Look` if a big path
    drags slowly.
  - (`noding.rs` was 459 lines: d2's edits went into `nodeops.rs`.)
- **M4d's d2 is built** (2026-10-09, 503 tests, deployed; **not yet
  looked at by Alva**).
  - **What's done to the anchors picked is `nodeops.rs`**:
    `Ink::node_op(NodeOp)` (Smooth, Corner, Break, Join, Delete, Add),
    one Command and one step each, from the Box's buttons, the
    right-click menu (`node_menu`, `menus::NODE_OP`), the keys, and
    double clicks on the canvas. `Can` says which there's anything to
    do with, to grey the rest.
  - **The Commands are `anchors.rs`'s**, all through `each`: the edits
    a closure gives for each shape's picked anchors, only the shapes it
    changes made paths first, and the names their anchors go by after.
    `smoothed`, `broken`, `joined`, `added` (and d1's `moved` and
    `deleted`).
  - **Double clicks, as LS3's pen has them:** on a segment, an anchor
    there (picked, to be dragged); on an anchor, a corner made smooth
    and back. So a press on an anchor just after a click on it toggles
    it instead of dragging it: LS3 does the same.
  - **Join takes two loose ends of one path** (the core's `Join` is
    within one outline): ends of two paths need the paths made one
    first, which nothing in the window does yet. **Alva hasn't been
    asked** whether Join should do that itself.
  - **A corner takes off its own handles only:** the curve into it
    keeps the handle at its other end, so it can still look curved.
  - **The Node tool's Box** (`Ink::node_box`): X and Y of the one
    anchor picked (typed, or dragged along as a gesture: `box_set`,
    shared with the shape rows' `tune`), then Smooth and Corner, Break
    and Join, Delete. Always the same buttons, greyed
    (`controls::button_if`) while there's nothing for them to do.
  - **`controls/button.rs`** is LS3's button, copied (D12), with Ink's
    greyed form beside it. A context menu's rows can't be greyed
    (LUI2's `Item` has no such state), so the menu leaves out what
    can't be done.
  - **Not in d2:** keys for smooth, corner, break and join (none
    asked for); making a segment straight again (`Straighten` is in the
    core); X and Y for several anchors at once.
  - (`noding.rs` was 472 lines; what shows and what's under the
    pointer moved to `anchors.rs` and `nodes.rs` with d3: 446.)
- **M4d's d3 is built** (2026-10-09, 507 tests, deployed; **not yet
  looked at by Alva**).
  - **One thing new in the core:** `PathEdit::Extend { from, to, out,
    into }` (`ink-doc/src/pathedit.rs`): on from a loose end to a new
    anchor, with a line or, where either has a handle on it, a cubic.
    From a run's first anchor it goes on backwards. Not over MCP.
  - **The Pen is `penning.rs`** (`Ink::pen_tool`, `Ink::penning`), and
    **the Node tool runs under it** (`workspace.rs`): the Pen takes a
    press on bare canvas (a point placed) and a press on the path's
    other end (closing it); a press on an anchor, a handle or a segment
    of what's selected is the Node tool's, as are double clicks, the
    right-click menu and the Box. So with the Pen in hand everything
    of the Node tool's works, but its marquee and its picking of
    shapes.
  - **Where the Pen goes on from is the one anchor picked**, when
    that's a loose end of an open path (`Tip`): so a click on either
    end of any selected open path, old or new, takes it up again. With
    no such anchor picked, a press begins a new path.
  - **A path begins with its second point.** The first is the Pen's
    own till then (`Penning::start`): a path of one point draws
    nothing, and a press and a change of mind would leave it in the
    file. So the first press is no step. The handle pulled out of the
    path's end is the Pen's too (`Penning::out`), till the next
    segment has a place for it.
  - **Handles are pulled by how far the pointer goes from the press**,
    not from where the point landed: a point snaps to whole units, and
    a click a little off one mustn't pull handles out of it.
  - **A press on the path's other end closes it, at once** (LS3's
    way), with the handle pulled out of this end and the other end's
    own, turned round. So that end can't be dragged while this one is
    picked: pick another anchor, or press Enter, first. **Told to Alva
    at d3's handoff.**
  - **Keys:** Enter lets go of the path (`menus::PEN_END`); Escape
    gives up a point being placed, then lets go as Enter does; Delete
    takes the end back off and picks the one before it (`pen_back`),
    or forgets a first point.
  - A new path is painted as the last shape was (`shapes::path`), on
    top of the level the Pointer is in, and is what's selected.
  - **Not in d3:** a line from the path's end to the pointer before a
    press (LS3 has none either); Alt to break a handle's mirror while
    placing; a setting to join onto another path's end.
  - **`ink-doc/src/pathedit.rs` is 459 lines:** the next edit wants
    `Outline::edit`'s arms in two files first.
- **M4d's d4 is built** (2026-10-09, 511 tests, deployed; **not yet
  looked at by Alva**), and with it **all of M4d**. Next: M4e (text,
  gradients and the eyedropper), once she has looked at d4.
  - **The Path menu is `pathops.rs`**: `PathOp` (its rows, in
    `PathOp::ROWS`), `can` (which are lit), `command` (the one Command
    each is), and `Ink::path_op`. A row's action is `menus::PATH_OP`
    with the row's label (`menus::path_action`). No Command is new:
    they're M3c's.
  - **Which shapes each is for** (my calls, told to Alva at d4's
    handoff): Union, Subtract, Intersect and Exclude take the shapes
    selected themselves, back to front, so **the one furthest back
    takes the result and keeps its paint**, and Subtract is it less
    the ones in front (as Inkscape, Illustrator and Figma have it).
    Object to Path, Outline Stroke, Simplify and Reverse take every
    shape the selection is or holds, as paint does: each the ones it
    has something to do to. Nothing locked.
  - Union is lit for one shape too (it's made simple where it crosses
    itself); the other three need two.
  - **The same rows are in the selection's right-click menu**, under
    "Path", the ones with nothing to do left out.
  - Afterwards what's selected is what was, that's still there, with
    what the step made (a stroke's outline beside its shape).
  - **Simplify's default tolerance is `ink_doc::paths::simplify_tolerance`**
    (a five-hundredth of the path's size): `ink-tools` uses the same,
    so the window and Claude simplify alike.
  - **Not in d4:** keys for these (Inkscape's Ctrl + and Ctrl − are
    Ink's zoom; none bound until Alva says which); a tolerance to set
    for Simplify or Outline Stroke; shapes in a selected group made one
    (select them themselves); a text made a path (Text to Path is
    slice e's).

- **M4e (text, gradients, the eyedropper) is under way**, in five
  pieces, the quick ones first (Alva's order, 2026-10-09): **e1** the
  Eyedropper, Object > Clip and Release Clip, Text > Text to Path;
  **e2** Drop Shadow and Blur, with their settings in the Box; **e3**
  the Gradient tool and the stops bar; **e4** the Text tool; **e5**
  the text's Box. Her calls the same day: **the Eyedropper takes the
  colour you see there** (not the shape's paint as set); and **straight
  through**: each piece is committed and pushed once it's tested and
  deployed ("M4e eyedropper and clip", "M4e shadow and blur", "M4e
  gradient tool", "M4e text tool", "M4e text box"), **stopping only at
  the end of M4e, with one checklist for all of it**.
- **M4e's e1 is built** (2026-10-09, 516 tests, deployed).
  - **The Eyedropper is `eyedrop.rs`** (`colour_at`,
    `Ink::eyedrop_tool`): one pixel of the drawing itself, drawn by
    `ink-render` eight times finer than the screen shows it, with the
    point in its middle. So it's the colour through gradients, opacity
    and shadows, and a blend only within a sixteenth of a screen pixel
    of an edge. Not finer: a blur costs by how finely it's drawn. It
    goes through `set_paint` (held: a gesture the paint section lands),
    so it's the selection's fill and the next shape's; Shift, the
    stroke. See-through is taken see-through; nothing, not at all.
  - **Object > Clip, Release Clip and Text > Text to Path are
    `effects.rs`** (`Effect`, `can`, `command`, `Ink::effect`): the
    thing on top of the selection, a shape, cuts the rest; one thing
    under it is cut itself, several are grouped and the group is cut
    (the group's id learnt on a copy). A text set in another font than
    it asks for is asked about ("Make paths of it anyway?"), as Ungroup
    asks.
  - `Ink::selected_after` is what's selected after a step that makes
    things: what's left of the selection, then what was made. The Path
    menu uses it too.
  - **Not in e1:** a cursor for the Eyedropper (the desktop's theme has
    none; LS3's is a PNG of its own); a swatch of what's under it
    before a press.
- **M4e's e2 is built** (2026-10-09, 519 tests, deployed).
  - **Shadows and blurs are `shadows.rs`**: `Soft` (a shadow: where,
    how soft, its colour, whose alpha is how dark; or a blur), `read`
    (what a thing's filter is, where it's one such step and nothing
    else), `set`, `removed`, and their rows in the Pointer's Box
    (`rows`; `Ink::soft_shown`, `soften`, `unsoften`, through
    `box_set` like the shape rows).
  - **A filter is changed where it is while it's the selection's
    alone** (the step's attributes and the filter's room, by
    `SetAttr`); one that other things use is left, and the selection
    gets its own. Taken off, a filter that was the selection's alone is
    deleted with it. A chain written by hand has no rows.
  - **The room a filter needs is `ink_doc::filter::region`**, which
    Claude's `filter_set` uses too (it was the tool's own): the window
    and Claude give the same filter for the same shadow.
  - **Object > Drop Shadow… and Blur…** (`Ink::soft_menu`) give the
    selection a first one (a tenth of the Box's dragging step × 100:
    one unit on an icon's page) unless the one in hand has its kind
    already, then open the Box with the Pointer in hand, where the
    settings are. A thing with no box to measure by (a level line) is
    said, not given one.
  - The shadow's colour uses the paint section's picker
    (`Ink::picker`), opened from a swatch in the Box.
  - **Not in e2:** a shadow and a blur on one thing at once (each
    replaces the other: one step to a filter here); spread, inner
    shadows, more than one shadow.
- **M4e's e3 is built** (2026-10-09, 525 tests, deployed).
  - **A shape's gradient in hand is `grads.rs`**: `Held` (which paint,
    the gradient, its line in its own numbers and what takes those to
    the drawing's coordinates, its stops), `Change` (its line, its
    stops, its kind), `Held::set` and `fresh`. `ink_doc::gradient`
    learnt `Gradient::line` and `to_user` for it (the renderer's
    `Paint::fit` says the same in its own words: change both).
  - **A gradient is changed where it is while it's that shape's
    alone** and says its own stops: only the attributes that differ
    (a stop's colour through `SetStyle`, so it's written where the stop
    has it). One that other shapes use, or that takes its stops from
    another, is left, and the shape gets a copy of its own with the
    change made (Alva's rule since M3b). Linear to radial is another
    element: a new one, and the old one goes if it was the shape's own.
  - **The tool and its Box are `grading.rs`** (`Ink::grade_tool`,
    `grade_box`, `Ink::grading`): a drag across a shape is its
    gradient's line (a plain colour gets a new gradient, from that
    colour to `section::gradient_of`'s darker one, measured by the
    shape's box); a press on another shape takes that one up; the
    line's ends are handles (round where it starts, square where it
    ends), with the stops' colours along it. Shift holds to 45°.
  - **"Radial" is the gradient in hand's kind**, and without one the
    next one's: a radial one is about where the drag begins, out to
    where it ends. **"Stroke"** has the tool work on the stroke's
    gradient.
  - **The stops bar:** a press near a stop takes it, anywhere else
    puts one there (the gradient's own colour at that place); held, it
    goes along with the pointer, no further than its neighbours. The
    one picked has its colour (the paint section's picker), its place
    ("At %") and "Remove Stop" (two are the fewest). The bar's drags go
    through `box_set`, as the other rows' do.
  - The Gradient tool works on one shape: of the selection, the one in
    hand (or the first shape it holds).
  - **Not in e3:** a text's gradient (its box is its glyphs' cells:
    `Held::of` takes shapes only); a radial gradient's focus (set to
    its middle when its line is moved); spread (pad, reflect, repeat);
    reversing; dragging a stop on the canvas (they show there, and are
    dragged on the bar).
  - **`grading.rs` is 451 lines:** its Box wants a file of its own
    before anything is added.
- **M4e's e4 is built** (2026-10-09, 532 tests).
  - **Two things new in `ink-doc`'s text:** `Laid::chars` (each
    character's own cell, in the order `said` says them: a caret stands
    at one's left side, or its right) and `text::written` (a text read
    back as the lines `SetText` would be given to write it again: a new
    line wherever a row starts one, the empty lines counted from how
    far down it starts, each stretch a span with what its `<tspan>`s
    set). What `SetText` writes, `written` reads back, and written
    again changes nothing. A span's `id` or `class` isn't kept.
    (`text.rs` passed 500 lines: its tests are `text/tests.rs` now.)
  - **Typing is `typing.rs`** (no window): `Words` (lines of spans),
    `Caret` (a line and a place in it), and what each key does.
    Typing goes into the stretch the caret is in, or on the end of the
    one before it, so stretches a file has are kept; nothing in the
    window makes a new kind of stretch yet.
  - **The tool is `texting.rs`** (`Ink::text_tool`, `Ink::texting`):
    a click on a text takes it up with the caret where the click is
    (`caret_at`, by `Laid::chars`); a click anywhere else begins a new
    one there, on whole units (Ctrl frees it), on top of the level the
    Pointer is in.
  - **Typing is a gesture, landed as "Type" once it pauses half a
    second** (LS3's rule), or when anything else is done:
    `Ink::type_settled` is the first thing `Ink::act` does, so an undo
    takes back what was just typed. After an undo the words are read
    from the drawing again (`Editing::stamp`); the caret stays put.
  - **A text begins with its first character** (till then the click is
    the tool's own, as the Pen's first point is), and **one whose last
    character is taken out goes** (`Command::Delete`): typed into
    again there, it's a new one.
  - **The keyboard is the text's while one is typed into:**
    `Ink::text_keys_in` takes its keys at the frame's start (letters,
    Enter, Backspace, Delete, the arrows, Home, End), before the
    window's own see them. A field being typed into (LUI2's
    `ime_rect`, read the frame before), a menu and a dialog keep the
    keyboard. Escape lets go of the text.
  - A new text is lettered as `Texting::letters` says and filled as the
    last shape was (a gradient isn't carried: Lantern's gold). Its size
    unless set is forty of the Box's dragging steps: 4 on an icon's
    page.
  - **Not in e4:** selecting within a text (so no copy, cut or paste
    of words, and no styling a stretch); Ctrl+A in a text (it's the
    drawing's); a text's own transform when a new one is typed where
    one was deleted; text on a path and the rest `text::Unset` names.
- **M4e's e5 is built** (2026-10-09, 535 tests, deployed; **not yet
  looked at by Alva**), and with it **all of M4e**. Next: M4f (the
  icon aids, and not losing work), once she has looked at slice e.
  - **The Text tool's Box is `textbox.rs`** (`read`, `set`,
    `Ink::text_box`): the font, the size, bold, italic, how lines hang
    (`text-anchor`), and how far apart they are. It letters the text
    typed into, or with none, the texts selected; and always the next
    one typed. Only what changed is set (`SetStyle`, so written where
    the text has it; off is the property taken off); line height is
    written into the lines (`SetText` with what `written` reads).
  - **The font list is every family installed, each row lettered in
    its own** (`controls::dropdown` with `faces`: LS3's dropdown,
    copied, D12), after the three generic names, which are Lantern's
    fonts. Asked for once a run (`Texting::families`).
  - **A font that isn't installed says so**, in a line under the list:
    which, and what this machine drew instead (`text::lettered`). The
    text keeps asking for the one it names.
  - What's been typed lands before a row of the Box is set, so each is
    a step of its own.
  - **Not in e5:** letter spacing, underline, a weight between regular
    and bold (`lntrn-text` has two); the Box's rows under the Pointer
    (take the Text tool).

- **M4f (the icon aids, and not losing work) is under way**, in five
  pieces: **f1** the pixel grid and snapping, **f2** rulers and guides,
  **f3** the preview strip, **f4** File > New…, Page…, Export… and
  Edit > Tidy, **f5** autosave and what it found, Preferences, and the
  last greyed rows. Alva's calls, 2026-10-09: **rulers are built** (a
  guide is dragged out of one; View > Rulers hides them); **the
  preview strip is at the bottom of the right panel**, folding away;
  **Preferences holds the essentials and the look** (what snapping
  lands on, a new drawing's decimals, the grid's and guides' colours,
  the checks behind the page); and **straight through**, as slice e:
  each piece committed and pushed once it's tested and deployed ("M4f
  pixel grid and snapping", …), **stopping only at the end of M4f,
  with one checklist for all of it**.
- **M4f's f1 is built** (2026-10-09, 547 tests, deployed).
  - **What lands where is `snap.rs`** (no window; LS3's `snap.rs` with
    a grid under it): `Targets` (lines down and across, the grid's
    step, how near a line draws), and the ways something lands on
    them: `point`, `edge`, `diagonal`, `moved`, and `handle` (each
    handle of the selection's box). `Landed` is the lines landed on.
  - **Nearest wins, a line before the grid where they're as near.** A
    line reaches eleven px; the grid reaches everywhere. So a line off
    the grid takes only what's nearer it than the grid is.
  - **Half units come in with zoom** (`snap::step_for`): once they're
    six px apart on the screen. A 512-unit page fitted lands things on
    whole units; an icon's page, on halves too. **My call, not Alva's:
    she was told at f1's handoff.**
  - **The window's side is `snapping.rs`**: `Ink::snap_to(ui, view,
    doc, skip)` before a tool reads the drawing (None while View >
    Snapping is off or Ctrl is down), `of_shapes` (which boxes are
    lines: shapes and texts that show, and what stands at the
    Pointer's level; not what's dragged), `add_anchors` (the Node
    tool's and the Pen's). What a tool landed on goes in `Ink::landed`,
    which the canvas clears each frame and the overlay draws.
  - **A moved box lands by an edge on the grid, never its middle**; on
    a line by its left, middle or right. So something off the grid is
    pulled onto it both ways by any drag, as in other editors.
  - **`shapes::grid_for` is a whole step of the grid** still; nothing
    rounds to it by itself any more (`on_grid` is gone).
  - **The pixel grid is `grid.rs`**: whole units across the page, over
    the drawing, once they're eight px apart.
  - `Settings` has `snapping`, `pixel_grid`, `snap_grid` and
    `snap_shapes` now (the last two wait for Preferences, f5).
  - The canvas's keys (the arrows, Escape) are `keys.rs` now, out of
    `pointer.rs` (which was 479 lines).
  - **Not in f1:** the gradient's ends, a curve's handles and a bent
    segment land nowhere (free, as Alva chose for handles); a box
    turned or scaled about its middle lands by the corner dragged
    only; numbers typed or dragged in the Box aren't snapped; equal
    gaps between shapes ("smart" spacing).
- **M4f's f2 is built** (2026-10-09, 556 tests, deployed).
  - **Guides are in the file:** `ink-doc/src/guides.rs` (`Guide::X`, a
    line down the page at an x; `Guide::Y`, one across at a y;
    `guides::of`), on the root as `ink:guides="x12 y4.5"`, and
    `Command::SetGuides { guides }`, all of them at once. No lock holds
    them. **Not over MCP** (no tool reads or sets them yet).
  - **The rulers are `rulers.rs`** (`step`, `parts`, `ticks`, `label`,
    by themselves and tested by themselves; `draw`) and three new
    rects of `Layout` (`ruler_top`, `ruler_left`, `ruler_corner`: the
    canvas is what's left). `theme::RULER` is their thickness.
  - **The left ruler's numbers are stacked digits** (my call: LUI2
    can't turn text, and whole numbers at 18 px would want a ruler
    fifty px wide). **Told to Alva at f2's handoff.**
  - **Guides in the window are `guiding.rs`** (`Ink::guide_tool`, run
    before every tool: while a guide has the pointer the tools get
    nothing; `Ink::guiding`). A drag of one is the window's own, and
    lands as one `Ink::edit`: "Add Guide", "Move Guide", "Remove
    Guide".
  - **The Pointer takes a guide only on bare canvas** (my call, told
    to Alva): where one crosses a shape or the selection's box, the
    press is theirs. The tan ground round the page is always bare.
  - `Ink::snap_lines(…, guides)` is `snap_to` with the guides or
    without (a guide dragged doesn't land on guides).
  - `Settings` has `rulers`, `guides` and `snap_guides`; View has
    Rulers, Guides and Clear Guides.
  - Test helpers: `Running::run(id)` does a menu's action.
  - **Not in f2:** a guide's place typed as a number; locking guides;
    slanted guides; a guide's own colour; the corner square does
    nothing (other editors move the origin from it); the rulers don't
    mark the selection's extent.
  - **For f4:** when File > Page… fits the drawing to a new viewBox
    (the root transformed), the guides want to go through the same
    transform.
- **M4f's f3 is built** (2026-10-09, 560 tests, deployed).
  - **The preview strip is `strip.rs`** (`Strip`, `Ink::strip`,
    `Ink::preview_strip`): `lntrn_svg::render` of the drawing's text
    at each of `strip::SIZES` times the screen's scale, on the job
    pool, shown one px for one. `ink-app` depends on `lntrn-svg` now
    (for this only: everything else in the window is `ink-render`'s).
  - **It takes the panel's foot, and the paint section pays for it**
    (`paint_section(ui, whole, foot, window)`): the tree's share is of
    the whole panel. In a 1080-px window with the strip open the
    palette grid is scrolled to; folded (`Settings::strip_folded`), it
    isn't.
  - **`ink_doc::lantern::misses`** is what `lntrn-svg` doesn't draw of
    a drawing (it was `ink-tools`' own): the strip says it in a line
    under the icons, the MCP's Lantern preview in its reply.
  - The window's tests keep the strip's pictures in a stand-in of
    their own (`Running::strips`), counted like the tiles'.
  - **Not in f3:** a light ground to see an icon on; a click on a size
    to zoom the canvas to it; the strip of anything but the tab that
    shows.

## Working here
- **Ink builds against LUI2's working tree** (`../lantern-ui-2`): a
  machine whose LUI2 is behind won't even load the workspace (it was
  `lntrn-mcp` missing on genforge, 2026-10-07). Pulling LUI2 is
  Alva's to say yes to.
- `cargo test --workspace`, `cargo clippy --workspace --all-targets`.
- **The corpus** is `tests/corpus/` (144 SVGs from the Lantern projects;
  `SOURCES.txt` says where each came from). Every layer is tested on it.
- **Goldens** are `crates/ink-render/tests/goldens/`. After a change
  meant to change a picture, look at the new one, then keep it:
  `INK_BLESS=1 cargo test -p ink-render --test golden`.
- **Reports to read, not tests:** how Ink differs from `lntrn-svg` and
  from `rsvg-convert` on every file, and one file's worst pixels:
  `cargo test -p ink-render --test corpus -- --ignored --nocapture`
  (`report`, `third_opinion`, or `inspect` with `INK_FILE=name.svg`).
  Speed: `cargo test --release -p ink-render --test speed -- --ignored
  --nocapture`.

## Ground rules for this repo
- **LS3 and LUI2 are other projects:** read them freely, ask Alva before
  changing anything in them. A LUI2 change Ink needs comes up one at a
  time, with its reason.
- **The May 2026 plan is retired** (iced, kurbo, usvg, vello): don't bring
  it back.
- **A plain `.svg` is the working file.** Opening and saving one
  untouched must give the same bytes.
- **Don't launch the GUI without asking**, and never capture the screen.
  The window's behaviour is proven in `ink_tests.rs`; how it looks is
  Alva's to see.
- **No GPU code below `ink-app`**, and `ink-doc`, `ink-render` and
  `ink-core` never depend on `lntrn-ui` or `lntrn-app` (ARCHITECTURE §2).
- **The 500 / 600 line rule is for code.** Docs aren't counted (this
  file, `docs/*.md`): they aren't code, and a design doc reads best
  whole (Alva, 2026-10-06).
