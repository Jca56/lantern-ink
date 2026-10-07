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
    `lntrn-svg`'s set exactly; the first three are due in M3.
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
  - `~/.lantern/bin/lantern-ink` is the May 2026 iced prototype's
    binary, not ours: left alone until M4's window takes the name.
- **M3 (operations) is under way** (a, b and c built; d text and e
  tidy to go), in five slices, in the order Alva
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
  - **Not yet:** `SetGeometry` has no Command: nothing needs it before
    M4's handles. (`ToPath` came with M3c.)

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
  **M3d (text) is next.**
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

## Working here
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
- **No GPU code below `ink-app`**, and `ink-doc`, `ink-render` and
  `ink-core` never depend on `lntrn-ui` or `lntrn-app` (ARCHITECTURE §2).
- **The 500 / 600 line rule is for code.** Docs aren't counted (this
  file, `docs/*.md`): they aren't code, and a design doc reads best
  whole (Alva, 2026-10-06).
