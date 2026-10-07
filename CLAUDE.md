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
- **M3 (operations) is under way**, in five slices, in the order Alva
  chose 2026-10-06: **a** structure and transforms, **b** paint (styles,
  gradients, clips, filters; the renderer learns `feGaussianBlur` and
  `<style>` rules), **c** paths (anchors, editing, boolean ops), **d**
  text, **e** tidy. D13 (bake a move into the geometry whenever that's
  exact; groups pass it down), D14 (a property is written where the node
  has it) and D15 (three decimals) are decided; D18 and D19 wait for
  their slices.
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
  - **Not yet:** `ToPath` and `SetGeometry` have no Command: nothing
    needs them before M3c's path tools and M4's handles.

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
    (That was for M3b: from M3c on, commits are asked for again.)
- **M3b is built** (2026-10-07, 232 tests, 30 tools): its last piece,
  b4, is `Command::SetClip` (`ink-doc/src/clip.rs`) and the tools
  `clip_set` and `filter_set`. **M3c (paths) is next**; D18 (anchor
  ids) is Alva's to decide at its start.
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
