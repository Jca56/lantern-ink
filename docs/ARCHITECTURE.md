# Lantern Ink — Architecture

M0 draft for Alva's review, 2026-10-05. Built from an audit of Lantern
Studio 3 (`~/Projects/lantern-studio-3`, "LS3"), Lantern UI 2
(`~/Projects/lantern-ui-2`, "LUI2") and the 193 SVG files in the Lantern
projects ("the corpus", §10). References like "LS3 §7" point into LS3's
`docs/ARCHITECTURE.md`; "U016" and friends into LUI2's `docs/DECISIONS.md`.

§12 lists the decisions. D1–D10, the ones that shape the foundation, were
made by Alva on 2026-10-05; the rest wait for their milestones.

---

## 0. In one screen

- **Rust + wgpu + Lantern UI 2, and everything vector is ours.** No kurbo,
  usvg, vello or iced (the May plan is retired, D1).
- **The SVG is the document.** Ink's document is the file's own tree:
  elements and attributes as they were written, with typed views on top.
  There is no second model to keep in step with it, and no export step.
- **Untouched means byte-identical.** Opening a file and saving it changes
  nothing. Changing one fill changes one attribute. Whatever Ink doesn't
  understand is kept and written back as it was. The one exception is
  another editor's private marks (Boxy SVG's): Ink takes those out and
  puts its own in their place (D25).
- **Every edit is a Command**, and **undo is snapshots**: LS3's rules,
  unchanged. The GUI, the MCP server and the live bridge all produce
  Commands.
- **One renderer, on the CPU.** The same code draws the window's canvas,
  Claude's previews and PNG exports, so they can't disagree. Nothing below
  the window needs a GPU.
- **Headless first.** Core, then the MCP server, then the GUI as a second
  front end on the same core, then the live bridge: LS3's order.
- **It looks like LS3.** Same tokens, same chrome, same rules (18 px text,
  big targets, the sheen, 2 px borders).

---

## 1. Goals and non-goals

**Goals:**
- Draw and edit Lantern's icons and cursors without leaving Lantern, then
  logos and branding (D3). Today 107 of the corpus's 193 files carry Boxy
  SVG's `bx:` marks, so that is the tool Ink has to replace (D20).
- The working file is a plain `.svg` that browsers and Lantern apps open
  as it is (D2), and that diffs cleanly in git.
- Claude can make and edit SVGs, headless and live in Alva's window.
- What Ink shows is what Lantern apps draw (§5.4).
- Tests that are exact, from the first line of the renderer.

**Non-goals for now** (not picked in D3, or simply later):
- Illustration scale (thousands of paths at 60 fps) and game-art workflows
  (artboards, sprite sheets).
- Animation, scripting, full CSS, SVG 2 text flow, PDF or EPS.
- A project format of Ink's own. It can come later without touching the
  foundation, since Commands and the tree don't care how they're saved.

---

## 2. Crates

One workspace, `~/Projects/lantern-ink`. Arrows show dependencies.

```
LUI2 (GPU-free): lntrn-core · lntrn-data · lntrn-math · lntrn-image · lntrn-text
LUI2 (GPU):      lntrn-render · lntrn-ui · lntrn-app

ink-geom    Vector maths: paths (lines, quadratics, cubics, arcs), affines,
            bounds, flattening, strokes and dashes as outlines, hit tests,
            boolean ops. Knows nothing of SVG or of documents.
   ↓
ink-doc     The document: lossless XML in and out, the node tree and its
            IDs, typed views (shapes, the style cascade, paints, transforms,
            units, text set in the machine's fonts), Commands and their
            validation.
   ↓
ink-render  Document → pixels on the CPU: the scene, the exact-area
            rasterizer on all cores, gradients, clips, groups, filters, text.
   ↓
ink-core    Core: apply(Command), history, gestures, queries (hit test,
            bounds, snapping), open and save, export. Headless.
   ↓
ink-tools   MCP tool schemas + the dispatcher + a JSON-RPC line server
            + the live bridge's pipe.
   ↓                               ↓
ink-mcp → bin lantern-ink-mcp      ink-app → bin lantern-ink
          (stdio; no GPU)                    (LUI2 window; hosts a core;
                                              listens on the bridge socket)
```

**Rules:**
- **No GPU code below `ink-app`.** Five of the seven crates never see
  wgpu. `ink-app` hands finished tiles to LUI2 as images (§8), so Ink may
  end up naming wgpu nowhere at all.
- **`ink-geom` and `ink-render` take no Ink-only types in their public
  API** beyond the document they draw, so they can move into LUI2 later
  (D8).
- **`ink-doc`, `ink-render` and `ink-core` never depend on `lntrn-ui` or
  `lntrn-app`.** Headless builds link no window code.
- **The 500 / 600 line file rule applies to every source file.** Docs
  aren't code and aren't counted.

**What exists already, and what we do with it** (from the audit):

| Need | Where it is today | Plan |
|---|---|---|
| Flattening, strokes (joins, caps), exact-area rasterizer on all cores | LS3 `studio-core/src/vector` (856 lines, f64; 4096² with 118k edges in 50 ms) | Flattening and strokes ported into `ink-geom`. The rasterizer is Ink's own (§5.1): LS3's counts a pixel twice where a stroke's pieces overlap in it |
| Path `d` parser, colour and transform parsers, gradients, clip paths, drop shadows, group layers, dashes | LUI2 `lntrn-svg` (3.3k lines, f32, all private behind one `render(svg, size)`) | Port the knowledge, in f64 (LUI2 D004). `lntrn-svg` itself stays as it is and becomes our reference (§5.4) |
| XML | `lntrn-svg/src/xml.rs` drops comments, text and formatting | A new lossless one in `ink-doc` (§3.2) |
| Text shaping, glyph outlines | `lntrn-text` (`place_outlines`, U057) | Used through `line_glyphs` (LUI2 U085, added for M3d: each glyph with its character and its place, which `place_outlines` didn't say) |
| PNG, JPEG, WebP encoders | `lntrn-image` | Use as is |
| JSON; JSON-RPC lines, both MCP eras, schemas, arguments, replies, the stdio loop | `lntrn-data`; LS3 `studio-tools` (about 1.5k generic lines among 7.2k) | D11: lifted into a new LUI2 crate, `lntrn-mcp` (LUI2 U082). LS3's staged calls and socket pipe stay with it until the live bridge (M5) needs them shared |
| The Studio look: theme, layout, chrome, controls | LS3 `studio-app` (about 1.8k lines) | D12: copied into `ink-app` as each slice of M4 needs it (the theme, layout and chrome with M4a; the controls with the paint panel) |

That makes three vector stacks in the ecosystem once Ink has its own
(`lntrn-svg`, LS3's, Ink's). Ink's is the superset, written to be the one
the other two can move onto (D8).

---

## 3. The document (ink-doc)

### 3.1 The tree is the file's

```rust
struct Document {
    id: DocId,
    root: NodeId,                         // the <svg>
    nodes: HashMap<NodeId, Arc<Node>>,    // Arc: cheap snapshots (§4.3)
    before: String, after: String,        // what the file has around the root
    next: Counters,                       // monotonic, never reused
}
struct Node {
    id: NodeId,
    rev: u64,                // stamped on every change: (id, rev) keys caches
    parent: Option<NodeId>,
    name: String,            // as written: "path", "linearGradient", "bx:grid"
    kind: Kind,              // from the name, once: Svg, G, Path, Rect, …, Other
    attrs: Vec<Attr>,        // in the file's order
    children: Vec<Child>,    // elements, and what's written between them
    written: Written,        // how its tags were written (§3.2)
}
struct Attr  { name: String, value: String, /* + its spacing, quotes, raw text */ }
enum  Child { Node(NodeId), Text(String) /* text, comments, CDATA, as written */ }
```

- **Attributes are the truth.** A rect's corner radius is its `rx`
  attribute, nothing else. Typed views read it, Commands write it.
- **Typed views** answer "what does this node mean", and are cached by
  `(id, rev)`, which stays sound across undo branches (the U016 idiom):
  - `Geometry::of(node)`: rect, circle, ellipse, line, polyline, polygon or
    path, with lengths in user units.
  - `Style::resolve(doc, node)`: the cascade (presentation attribute, then
    `<style>` rules, then inline `style=""`) and inheritance from ancestors.
  - `Paint`, `Gradient` (following `href` chains), `Affine`, `ViewBox`.
- **A basic shape stays what it is.** A `<rect>` is saved as a `<rect>`,
  so its radius stays adjustable forever ("draw once, adjust forever").
  It becomes a `<path>` only when an edit needs it to, or Alva asks.
- **Paths keep their segments' kinds:** line, quadratic, cubic, arc. An
  arc is not turned into cubics behind anyone's back; a segment changes
  kind only when an edit can't be said in the old one.
- **Nothing in a file can stop it opening.** A malformed attribute means
  that node draws nothing (or the default), is flagged in `doc_info`, and
  is written back as it was. Only "this isn't XML" or "this isn't an SVG"
  refuse, saying why. Hostile files meet limits (nodes, depth, sizes).
- **Unknown elements and attributes** (`<metadata>`, another vocabulary's
  elements) are ordinary nodes of `Kind::Other`: shown in the tree,
  movable, deletable, never drawn, written back untouched.
- **Another editor's private marks are not kept** (D25). Ink knows such
  editors by their namespace: Boxy SVG's `https://boxy-svg.com` today,
  which is all the corpus has (107 files declare it; 33 hold its
  `<bx:export>` list, one a `bx:shape` hint). When a file is opened, every
  element and attribute in that namespace is taken out, and its `xmlns:bx`
  becomes `xmlns:ink` in the same place. The file on disk changes only
  when Alva or Claude saves. The reader and writer themselves stay
  lossless (§3.2): this is an edit made on the way in, not a lossy parse.

### 3.2 Lossless reading and writing

- Each node remembers **how it was written**: attribute order, quotes,
  the whitespace inside its tags, whether it closed itself. Each attribute
  keeps its raw text for as long as its value hasn't changed.
- The writer replays that. **An unchanged document saves to the same
  bytes.** A changed attribute is rewritten and nothing else moves, so a
  recolour is a one-line diff.
- Inside `style="…"`, one declaration is rewritten and the rest are kept.
- New nodes take their indentation from their siblings, and markup put
  in whole is laid out the same way all the way down (never among a
  `<text>`'s words, where white space means something). A node moved
  to another depth takes what's inside it in or out as far as it went.
- UTF-8 only (a BOM and CRLF line ends are kept as found); other encodings
  are refused with a clear message.
- **Numbers Ink writes** are rounded to a per-document precision (D15),
  and the rounded number is what's in memory too: what's saved is exactly
  what was shown.

### 3.3 Addresses and frames

- **IDs** are short, stable and never reused, as in LS3: documents `d1`
  (a headless server's) or `w1` (the window's), nodes `N7`. Node IDs live
  for as long as the document is open and are not written to the file
  (D10). An element's own `id="…"` is just an attribute, shown beside it.
- **Numbers mean what they mean in the file** (D16): an attribute a tool
  or a Command sets is in the node's own coordinates, inside whatever
  transforms its groups have, exactly as SVG has it. Nothing is converted
  behind anyone's back. What's reported back (`doc_info`'s boxes, hit
  tests) says where a node shows in the document's coordinates, the
  root's user units, y down; and the tools that move things about on the
  page (M3's `node_transform`) work in those.
- **View state lives outside the document:** camera, the selection, the
  open groups, guides being dragged. The selection is not undoable and
  never Claude's (every Command names its nodes).
- **Ink's own extras** are attributes in an `ink:` namespace that other
  programs ignore: `ink:locked`, `ink:label`, guides on the root (D19),
  and `ink:decimals` on the root, which is D15's "settable per document"
  (since M3a; `doc_set` sets it).
  The root declares `xmlns:ink` in a document Ink made, in one it took
  over from another editor (D25), and from the first time an `ink:`
  attribute is needed; a file from elsewhere is otherwise left unstamped.
- **A label and a lock** (as built in M3e, `ink-doc/src/marks.rs`).
  `ink:label` is a name for a person to know a node by: `doc_info`
  shows it, and the layers panel will; it changes nothing of how the
  drawing draws. `ink:locked="true"` makes a node, and everything in
  it, refuse every edit until it's unlocked (LS3's lock): it can't be
  changed, moved or deleted, and neither can anything in it. Every
  Command is checked as its turn comes (`Document::guard`), so in a
  batch what an earlier step unlocked a later one may change. Three
  ways a Command reaches a node: it changes the node itself (refused
  when the node or anything above it is locked); it would move,
  rewrite or remove what's in the node too, as a delete, a transform,
  an ungroup or a boolean does (refused as well when anything in it is
  locked); or it only locks or unlocks (refused only by a lock further
  up). So things still go beside a locked node, a group it's in can be
  painted, and a copy of it is a node of its own: **not locked** (Alva,
  2026-10-07: nobody locked the copy, and one that couldn't be moved
  off its original was no use). **A lock holds what its node is drawn
  with** (Alva, the same day): the gradient, clip path or filter it
  names, in an attribute or by a `<style>` rule, and what those name
  in turn. Changing one, or what's in one, or deleting the `<defs>`
  it's in, is refused like a change to the node itself, or a locked
  node could be recoloured or blanked from the side
  (`Document::held`). What only unlocked nodes use stays free.
  Tidying steps round what's locked. A clean copy to ship carries
  neither mark. The marks
  are read under whichever prefix stands for Ink's namespace, and
  written as `ink:`, declared on the root the first time one is needed.
- **Dirty state is derived:** `modified = version != saved_version`.

### 3.4 Commands

Typed, in document space, naming their nodes, deterministic, validated
all-or-nothing before anything changes. Families:

| | Commands |
|---|---|
| Structure | `Insert`, `Delete`, `Move` (reorder or re-parent, keeping the look), `Duplicate`, `Group`, `Ungroup`, `Paste` (a drawing off the clipboard put into this one, M4b) |
| Geometry | `SetGeometry` (a shape's own numbers: what a shape's own handle does, M4c), `SetPath`, `Transform` (any affine on any nodes, D13), `Resize` (the same, lines left as they are: what a handle does, M4b), `ToPath` |
| Style | `SetStyle` (fill, stroke, width, caps, joins, dashes, opacity, rule), `SetGradient`, `SetClip`, `SetFilter` |
| Path work | `Boolean` (union, subtract, intersect, exclude), `OutlineStroke`, `Simplify`, `Reverse`, `Join`, `Break` |
| Text | `SetText`, `TextToPath` |
| Document | `SetViewBox`, `SetSize`, `Tidy` (drop what nothing uses) |
| Raw | `SetAttr { node, name, value }`: any attribute, for what has no Command yet and for an XML inspector |
| | `Batch(Vec<Command>)`: one history entry, all or nothing |

One writer module turns typed values into attribute text, and decides
where a property goes (D14), so no Command formats numbers itself.

**As built in M1:** `SetAttr`, `Insert`, `Delete`, `Move` and `Batch`
(the structure and the raw rows). The typed ones come with the tools
that need them, in M2 and M3.

**As built in M3a:** `Transform`, `Duplicate`, `Group`, `Ungroup`, and
`Move` keeping a node where it shows. The writer is `ink-doc`'s
`value.rs` (numbers, points, path data and transforms as text, at the
document's `Precision`) and `props.rs` (a property set where the node
has it, D14); `shape.rs` is the typed view of a shape's own numbers.
`SetViewBox` and `SetSize` are `doc_set`, over `SetAttr` on the root.
What's at a point (§4.1's `hit`) is `hit.rs`.

**As built in M3b:** `SetStyle` (`styling.rs`): properties by SVG's own
names, each value checked where Ink draws with it, set on any number
of nodes, each written where its node has it (D14). What would paint
nothing is a slip too, and refused: a `url(#…)` that names nothing in
the drawing (unless the paint says what to use then), and a gradient
measured by the box of what it paints on a shape with no height or no
width (a level line: SVG paints nothing there). `Define` puts
elements (gradients, and in time clip paths and filters) into the
drawing's `<defs>`, made as the root's first child if there isn't one;
`SetGradient` is `gradient_set`, over `SetAttr`, `Delete` and `Insert`
on the gradient and its stops.

**As built in M3e (marks):** `SetLabel` and `SetLocked` (`marks.rs`,
§3.3), and the guard every other Command passes first.

**As built in M3e:** `Tidy { also }` (`tidy.rs`) drops what nothing
uses, and nothing that shows changes (Alva's scope, 2026-10-07).
- **Always:** definitions nothing refers to (gradients, clip paths,
  filters, masks, patterns, markers, symbols, and whatever else is
  kept in a `<defs>` for others to use), round after round until
  nothing more goes (a gradient only an unused gradient built on goes
  too); groups and `<defs>` with nothing in them; namespace
  declarations nothing under them uses. What a `<style>` sheet names
  counts as used. Ink's own namespace stays: a drawing Ink made
  carries it.
- **Only when asked by name**, since someone wrote these on purpose:
  `comments` (one alone on its line takes the line with it, except
  among words, where white space is part of what's said); `ids`
  nothing in the drawing refers to; `words` (titles, descriptions,
  metadata).
- **A clean copy to ship** (`tidy::shipped`, `Core::export_svg`,
  `doc_export` to an `.svg`): the drawing tidied, with its comments
  and Ink's own marks out too (every attribute in Ink's namespace, and
  the declarations), formatting kept. A copy: the open drawing and its
  file aren't touched, and an open drawing's file is refused as the
  place for one.
- **Proven by drawing** (`ink-render/tests/tidy.rs`): every corpus file
  tidied (with nothing asked for, and with everything) and every clean
  copy draws the same bytes as before, reads back the same, and has
  nothing left to tidy. In the corpus that is 125 definitions, 17
  empty groups and `<defs>` and 2 idle declarations in 38 files; asked,
  461 comments and 26 ids. Clean copies of all 144 come to 600 KB
  where the drawings are 641 KB.

**As built in M3d:** `SetText` (`lettering.rs`) and `TextToPath`
(`outlined.rs`), both in §5.5; `Transform` puts a text's move into its
`x` and `y`.

**As built in M3c:** `ToPath`, and the path work as two Commands on
one model: `EditPath { node, edits }` and `SetPath { node, runs }`.
A path is read as an `Outline` (`outline.rs`): runs of anchors, each
joined to the next by a line, a quadratic, a cubic or an arc, whichever
the file had. `PathEdit` (`pathedit.rs`) is the edits: move, set
handles, add (the path keeping its shape), delete, bend, pull (a
segment taken by any point of it: what a drag does, M4d), extend (on
from a loose end to a new anchor: what a pen does, M4d), straighten,
smooth, corner, close, break, join, reverse; so §3.4's `Reverse`,
`Join` and `Break` are edits, not Commands of their own. A shape that
isn't a path is made one by its first edit. Anchors have ids (D18):
on the node, from the document's counter, kept whenever a path's data
changes without its layout changing, and never written to the file.
An outline is settled before it's written (`Outline::settle`) into
what its path data reads back as, so the ids beside a path always
name the anchors its `d` has: a closed run's last anchor, written
where its first is with only the closing line between them, is the
first.

**Boolean operations** (M3c) are one Command, `Boolean { nodes, how }`
(union, subtract, intersect, exclude): the first node takes the result
as its outline and keeps its place and paint, the others are removed.
Each shape is what its own fill rule fills, where it shows; the work
is done in the first node's coordinates, where the result is written.
`ink-geom` does it on the curves themselves (`meet.rs`, `wind.rs`,
`combine.rs`): every outline is cut where it meets another, each cut
edge is kept if the result is on one side of it and not the other
(by how the outlines wind round a point just to either side), and the
kept edges are joined into loops with the result on their left all
the way round, so the result fills the same by either rule. A cut
piece of an arc is an arc of the same ellipse and a cut piece of a
curve the same curve: nothing is flattened, and a circle that comes
through untouched comes back the circle it was. §10 says how it's
tested.

**Outline stroke and simplify** (M3c) are `OutlineStroke { nodes }` and
`Simplify { nodes, tolerance }`.
- A stroke's outline (`ink-geom`'s `offset.rs`) is a ribbon along each
  piece of the line, a join at each corner and a cap at each open end,
  made one by the union above. Beside a straight line the stroke's edge
  is a straight line and beside a circle's arc an arc, exactly; beside
  any other curve it is fitted with cubics, to within a tolerance (a
  two-hundredth of the stroke's width unless told; never finer than
  the file can write). Where a stroke is wider than its line bends (it folds
  over itself) that stretch is built from short straight slices
  instead: no one ribbon's outline says what a folded stroke covers. A
  shape with no fill becomes its stroke's outline, and so does one
  with no inside for a fill to cover (a `<line>`, which says
  `fill="none"` only when someone thought to); one with a fill
  keeps it, and the outline is a new path over it (in a group that
  takes over the shape's opacity, filter, clip path and mask, if it had
  any: they applied to fill and stroke as one). What is measured
  against a shape's box (a gradient across it, the reach of a filter on
  it or above it) is measured against the outline's box afterwards,
  which is half the stroke's width bigger all round.
- A path is simplified (`simplify.rs`) a smooth stretch at a time, from
  corner to corner (a turn of more than 30° is a corner, and stays):
  each stretch becomes one line, one arc of a circle or one fitted
  cubic if that stays within the tolerance of it both ways, and is
  halved and tried again if not. No anchor moves and none is made, so
  the anchors left keep their ids. **What's fitted either side of a
  join shares its heading there:** half way between the one the join
  is reached with and the one it's left with, which for short lines
  drawn round a curve is what the curve had (each half fitted to its
  own end lines met the other at a kink, and missed the tolerance
  where the shared heading meets it: M3's done-test). A curve whose
  handles pass each other is no fit: it pinches into the corner it was
  meant to smooth.

**A gradient in a shape's own coordinates goes with the shape**
(`settle.rs`, since M3b) when it is that shape's alone: into the
gradient's own numbers when they're plain and all it has been through
is a move, an even scale, a turn or a mirror, and into its
`gradientTransform` otherwise. One that anything else uses (another
shape, another gradient's `href`, a group handing it to its children)
is never touched (Alva, 2026-10-06): its shape keeps the move in
`transform`.

**So does a clip path** (Alva, 2026-10-07, changing what M3a did): one
that is a node's alone goes where the node's numbers go, its shapes
taking what the node took (or the clip path keeping it as its own
`transform`, as a group would). So a clipped group passes a move down
like any other. A clip path that others use, that goes by its node's
box, that is itself cut by another, or that isn't there to look at,
is never touched, and holds its node's transform. A mask always does.

**`SetClip`** (`clip.rs`) cuts nodes to shapes: the shapes go into a
new `<clipPath>` in `<defs>`, written in the coordinates of what they
cut and staying where they showed. With no shapes it takes the clip
off, and a clip path that then cuts nothing is taken apart, its shapes
put back in the drawing over what they cut. `SetFilter` is
`filter_set`, over `Define` and `SetStyle`: a drop shadow or a blur in
a `<filter>` whose region has room for how far the effect reaches.

**Where a transform is written** (D13, `settle.rs`). A node put through
a transform looks exactly as SVG says it would with that transform on
it, so:
- **A shape takes it into its own numbers** when they can say the same
  picture: a path, a line and a polygon anything; a circle while it
  stays round; a rect and an ellipse a move, a scale along their sides
  and a quarter turn. Any other turn of those two stays as one `rotate`
  about their middle; a skew stays whole, as a `matrix`.
- **How it's drawn goes with it.** A stroke grows with its shape, so a
  stroked shape takes only an even scale into its numbers. A gradient
  across its box wouldn't turn with them, nor dashes along a rect's
  fixed outline, so a turn or a mirror then stays in `transform`. A clip
  path, a mask, markers and a gradient or filter in the node's own
  coordinates would be left behind, so everything stays there; a shadow
  wouldn't turn or grow, so a filtered node takes only a move.
- **A group passes it down** when everything in it can take it without
  being left a transform it didn't have, and keeps it otherwise.
- **A transform another editor left is taken in** the first time its
  node is moved, when that's exact.
- **"The same" is to the document's precision:** half a unit in the
  last place written, at the drawing's far edge.

Its done-test is `ink-render/tests/settle.rs`: every corpus file put
through seven transforms draws what the transform itself would (the
worst of 1008 pictures 0.08 levels apart), and every group that can be
taken away leaves its picture as it was.

---

## 4. Commands, history, gestures (ink-core)

### 4.1 Core API
The same shape as LS3's (LS3 §4.1): `apply(doc, Command, Actor)`,
`begin / update / commit / cancel` for gestures, `render`, plus queries
(`hit`, `bounds`, `snap_points`). `Applied` (the nodes changed, made,
removed and moved) tells the GUI what to select and the MCP server what
to say. **What to draw again is not a Command's to say** (as built in
M4b; this section first gave `Applied` a `dirty` box): the renderer
holds the drawing laid out now against the drawing laid out before
(`Plan::changed_from`, §8), which is right for every Command there is or
will be, for undo, and for an edit of Claude's, with nothing for a new
Command to forget.

### 4.2 History
- **A snapshot is the `nodes` map** (pointer copies) plus `root`. Editing
  a node is `Arc::make_mut`: one node copied.
- There are no pixels in history, so it is small: an icon's whole history
  is kilobytes. Capped at 1,000 states for now; a cap by bytes comes if a
  document ever needs it.
- **Generic by construction:** `apply` always runs validate → snapshot →
  execute → record → bump version. A new Command can't skip undo.
- Entries carry a label and an `Actor { Alva, Claude }`.

### 4.3 Gestures
- **Previews never write into the document.** A drag is a session holding
  overrides (this node, drawn with that transform or geometry) that the
  scene builder substitutes. It commits as exactly one Command.
- An MCP read in the middle of Alva's gesture sees only committed state.
- Heavy work runs at most once a frame, never per input event.

**As built in M4b** (`ink-core/src/gesture.rs`; Alva's call, 2026-10-08:
a drag shows the real drawing, live and exact):
- **A gesture comes to one Command, said again whenever the drag moves
  on**: "these nodes, moved this far from where they were", the whole of
  the drag so far and never a step of it. `Core::update` applies it to a
  copy of the document (which shares every node it didn't touch), and
  that copy is what the window shows: `Core::shown` gives it, with a
  `Look` that is equal to another moment's only if the drawing looked
  the same then. So the override is any Command at all, and the scene
  builder needs to know nothing of gestures.
- Nothing drifts: a hundred small moves aren't a hundred roundings to
  three decimals, and a drag that comes back to where it began is no
  change and no step.
- `Core::doc`, the history, what's saved and the autosave are the
  document as committed all the while. An edit that lands in the middle
  of a drag (Claude's over the bridge, an undo) has the gesture worked
  out again on the document as it is now.
- `Core::commit` applies the Command for real, as one labelled step
  (what it makes has the ids its preview showed: the copy counts from
  where the document does). A Command the document refuses shows
  nothing, and `update` and `commit` both say why.
- A node's `rev` can come round again in a preview with other content:
  nothing may key a cache by it across previews.

---

## 5. Rendering (ink-geom + ink-render)

### 5.1 The pipeline
`Document` → **scene** (per drawn node: its outline flattened for the
current scale, paint, opacity, clip, filter, bounds; cached by
`(id, rev, scale)`) → **raster** (rows across all cores) → straight RGBA8.

- **The rasterizer gives each pixel the exact share of it that's
  inside** (`ink-render/src/coverage.rs`): a row is cut where edges
  start, end or cross, so what's inside is trapezoids that never overlap,
  and their sides' swept areas add up to the coverage. Pieces that share
  an edge leave no seam, pieces that overlap aren't counted twice, and
  the bytes are the same however the rows are split into bands.
- **Curves** flatten within a tolerance of the output pixel (0.05 px),
  arcs holding the curve's exact area.
- **Strokes** as polygons, the SVG way: miter (with its limit), round and
  bevel joins; butt, round and square caps; dashes with an offset.
  Since M3c a stroked line is flattened for its stroke
  (`Path::flatten_to_stroke`): a point right by each end of every
  curve, so caps and joins sit square to the curve itself and not to
  its first chord, and no chord turning further than the stroke's
  outer edge can take; and dashes are cut from the curves themselves
  (`Path::dashed`), before flattening. What is still approximate:
  where a wide line is cut off square on a tight bend (a butt cap, a
  dash's end), the boxes along the flattened line stand a little past
  the cut on the inside of the bend. A fifth of a pixel for an icon's
  strokes; `ink-render/tests/outline.rs` measures it. Slices between
  the curve's true normals, as `offset.rs` makes them, would be exact.
- **A pixel too faint to show is clear** (alpha 0 has no colour): the
  far edge of a blur is the same bytes whichever band drew it.
- **Paints:** colours, linear and radial gradients (units, transform,
  spread, `href`), `currentColor`.
- **Groups** that fade, clip or filter as one draw into a layer of their
  own, laid on when done.
- **Clip paths**, nested, in either units.
- **Filters:** chains of steps (§5.2), in the colour space the file
  asks for.
- **Text** through `lntrn-text`'s glyph outlines, filled like any path
  (§5.5). It is set in `ink-doc`, which is where its box, what's at a
  point and (in time) text made into paths need it too; the renderer
  draws the outlines it's handed.
- **Colour:** 8-bit sRGB, composited in sRGB as SVG does.
- **A picture too big to draw whole is drawn in parts** (`Plan`,
  `ink-render/src/plan.rs`, for the window's tiles, §8): the drawing is
  laid out once for a zoom (the scene, before it's fitted to a frame)
  and any rectangle of its picture drawn from that, on whatever thread
  asks, with as much of the picture around it as its shadows look (the
  bands' margin, up to `MAX_REACH`, 1024 px). Parts laid edge to edge
  are the picture drawn whole, to within a rounding of the last bit: a
  part is the same shapes moved, where a band is the same shapes cut.
  `Plan::bounds` is the box around everything it paints, so a part
  outside it is known clear without being drawn.

### 5.2 What v1 draws, by what the corpus uses

| Drawn in v1 | Files using it (of 193) |
|---|---|
| Paths, basic shapes, groups, transforms | throughout |
| Linear / radial gradients | 157 / 31 |
| `feDropShadow` / `feGaussianBlur` | 135 / 11 |
| Inline `style=""` | 114 |
| Clip paths | 86 |
| `<text>` | 26 |
| Dashes | 15 |
| `<style>` rules / `class=""` | 1 / 3 |

**Kept in the file but not drawn yet** (no corpus file uses any): `<use>`,
`<symbol>`, `<mask>`, `<pattern>`, `<image>`, `<marker>`, other filter
primitives, blend modes. The tree marks such nodes "not drawn"; they come
in the order something needs them (D21).

**As built in M1:** everything in the table but `<text>`, `<style>`
rules and `feGaussianBlur`, which is exactly what `lntrn-svg` draws.
Those three come with M3's operations: the last two in M3b (below),
`<text>` in M3d (§5.5).

**As built in M3b:** `<style>` rules, and filters as chains of steps.
- **A filter is a chain** (`ink-doc/src/filter.rs`, drawn by
  `ink-render/src/filter.rs`): each step works on the element as drawn,
  on its shape alone, or on what an earlier step made (`in`, `in2`,
  `result`), in linear light unless it says `sRGB`. The steps are the
  seven the corpus uses: `feGaussianBlur`, `feOffset`, `feFlood`,
  `feComposite` (all its operators), `feMerge`, `feComponentTransfer`
  and `feDropShadow`. A filter with any other step, or with a step
  given a region of its own, isn't taken up: its element draws
  unfiltered rather than wrong. A filter works on what's inside its
  region and shows nothing outside it; the region turns with its
  element (`rsvg-convert` keeps it upright: the one place the two part).
- **`<style>` rules** (`ink-doc/src/sheet.rs`): selectors made of
  element names, `.class`, `#id` and `*`, alone or together, nested by
  a space or `>`; anything else matches nothing, and `@` rules are
  stepped over. What the rules say of each node is worked out when the
  document is read and after every Command, and kept on the node (never
  written), so `style::prop` answers from the node alone and everything
  that reads a property sees the rules: the renderer, hit tests,
  `Transform`. The order is CSS's: an attribute, then rules (by how much
  they single out, then the later), then the node's own `style`, then a
  rule's `!important`. So D14 has one more clause: a property a rule
  gives a node is set in the node's `style`, the only place that
  outvotes the rule.
- `lntrn-svg` draws neither, so the nine corpus files with them are
  left out of the agreement test and read against `rsvg-convert`
  instead: the eight with blurs are 0.27 to 0.61 levels from it on
  average at 256 px, where unfiltered they were 0.63 to 9.3.

### 5.3 Why the CPU (D7)
- One renderer for the window, previews and exports: no "it looked
  different in the export".
- The MCP server needs no GPU: it starts at once and can't fail for want
  of one (LS3's server exits without a GPU).
- Goldens are exact, and not tied to a GPU or its driver.
- It is fast enough for D3's work by a wide margin. Measured in M1 (22
  cores, release): the kitchen-sink test drawing takes 0.8 ms at 32 px,
  5 ms at 256 px and 60 ms at 1024 px. LS3's stress scene (4096², 202
  shapes, 118k edges) takes 220 ms, against 50 ms in LS3's rasterizer,
  which doesn't resolve overlaps. A stroker that emits outlines with no
  overlaps, and band buffers kept between frames, are where that time
  comes back when the window wants it.
- **The seam for later:** the scene is plain data. A GPU backend for
  illustration-scale work would consume the same scene, and the CPU one
  would stay as the reference it's tested against.

### 5.4 What Lantern apps will draw
Apps on LUI2 draw icons with `lntrn_svg::render` (LS3 and two Lantern-DE
crates today; six older Lantern-DE crates still use `resvg`). Ink depends
on `lntrn-svg`, unchanged, for two things:
- **The Lantern preview:** the icon at 16, 24, 32, 48 and 64 px, drawn by
  `lntrn-svg` itself, in the window and over MCP. It also says what
  `lntrn-svg` will not draw (today: text, masks, `<use>`, most filters).
- **Agreement tests:** Ink's renderer and `lntrn-svg` must agree on every
  corpus file both draw all of (D22; 26 files have something only Ink
  draws: blurs, `<style>` rules, text). They can't to the pixel: `lntrn-svg` samples 16
  heights per pixel row where Ink takes exact areas. Measured, the worst
  file is 0.95 levels apart on average at 64 px and 0.30 at 256 px. Two
  real differences remain, a few pixels each: dashes round a curve (Ink
  measures along the true curve), and a shadow thrown in from past the
  picture's edge (`lntrn-svg` and `rsvg-convert` have nothing there to
  throw; Ink draws a margin so it does).

`rsvg-convert` is on this machine (`/usr/bin/rsvg-convert`). Tests use it
as a second opinion when it's there and skip when it isn't; nothing links
to it.

### 5.5 Text
**As built in M3d (its first piece, the renderer; 2026-10-07).** A
`<text>` is drawn as outlines: its glyphs, filled and stroked like any
shape's.

- **The fonts are the machine's** (`ink-doc/src/fonts.rs`), found once
  and shared by every drawing in the process: `lntrn-text`'s engine
  behind a lock. It shapes a line (fallback from font to font,
  ligatures, kerning, right-to-left runs) and hands back each glyph's
  outline, the character it stands for and its place on the line
  (`TextEngine::line_glyphs`, LUI2 U085). Ink shapes at 1000 px to the
  em and keeps everything for a font size of 1, so a glyph's numbers
  are the font's own and one shaping serves any size.
- **The generic names are Lantern's** (Alva, 2026-10-07): `sans-serif`
  is the desktop's font (`lantern.toml`'s `[appearance] font_family`,
  read by `ink-mcp`; Inter where that names none), `monospace` is
  JetBrains Mono, `serif` the first installed of a short list (Lora
  first). A family that isn't installed gives way to the next in its
  `font-family`, and a text with none left is set in the sans. So text
  looks as it does in Lantern's apps, not as a browser on the same
  machine (which asks fontconfig) would set it. `lntrn-text` has two
  weights: from 600 up is bold. **A text `text_add` makes says
  `font-family="sans-serif"`** when it's given no font and no group
  above hands one down: with none said, a browser sets it in a serif.
  **Italic in a family with no italic is upright** (Ink slants nothing
  itself; a browser does), and the replies say so
  (`fonts::slants`, `Lettered::no_italic`).
- **A character that comes back as a picture** (the engine falls back
  to the colour emoji font for a heart from some families) can't be
  filled or stroked, so it's asked for again in families that draw
  symbols and emoji as outlines, and the line closes up around it.
  One that nothing draws as an outline isn't drawn.
- **Setting** (`ink-doc/src/text.rs`, `text/lay.rs`): the characters of
  the text and of the `<tspan>`s (and links) in it are gathered, white
  space dealt with as a browser does (line breaks and tabs are spaces,
  spaces in a row are one, none at either end; kept as written under
  `white-space: pre` or `xml:space="preserve"`), then shaped a run at a
  time: as far as one font goes with no jump in it, so elements that
  only paint differently still kern as one word. The pen starts at the
  text's `x` and `y`; an element's `x` or `y` puts it somewhere else
  and starts a new chunk, its `dx` and `dy` nudge it (the innermost
  element at a character has the say); each chunk is hung from where
  it started by its `text-anchor`. `letter-spacing` goes after each
  character and `word-spacing` after each space; `dominant-baseline`
  says which line of the font sits at `y`. Lengths may be in `em`, and
  a percentage is of the page.
- **What comes back** (`text::Laid`): each element's glyphs as one
  `Path` in the text's own coordinates, in painting order, so a
  `<tspan>` is painted as it says (`text::style_of`); and the box
  around the glyphs' cells (each as wide as its advance, from the
  font's top to its bottom), which is what a gradient or a filter
  measured against "the element's box" is measured against. Glyphs
  fill non-zero whatever `fill-rule` says.
- **Everything that reads an outline reads a text's:** the renderer, a
  clip path with a text in it, what's at a point (`hit.rs`: a text is
  there where its glyphs are), and where a node shows (`doc_info`'s
  box is around the glyphs themselves, as a shape's is around its
  outline). `geometry::outline_of` is the one way in.
- **Not set, so not drawn at all** rather than drawn wrong
  (`text::Unset` says which, and `doc_info` passes it on): text along a
  path, characters placed or turned one by one (`rotate`, or a list in
  `x`, `y`, `dx`, `dy`), text set top to bottom, text stretched to a
  length. Not read yet: the `font` shorthand, `text-decoration`,
  `baseline-shift`, `direction`.
- **A text takes a move into its numbers** (`settle/lettered.rs`,
  M3d's second piece): its `x` and `y`, and those of every `<tspan>`
  in it that says where it starts, all of them or none. A turn or a
  scale stays its `transform` (text is set upright, at its own size),
  and so does a move where a position can't have a number added to it
  (`50%`, `2em`) or the text's paint would stay behind (a gradient laid
  out in the page's coordinates, a clip path others use).
- **`SetText` gives a text its words** (`ink-doc/src/lettering.rs`):
  lines, each a row of stretches with whatever each sets for itself.
  SVG has no line breaks, so each line after the first is a `<tspan>`
  back at the text's `x`, `dy` ems further down (so it keeps up with
  the font's size); a stretch with paint or lettering of its own is a
  `<tspan>` saying so; an empty line is the space before the next.
  White space that would collapse (at a line's end, two in a row) puts
  `xml:space="preserve"` on the text. Everything that was in the text
  is replaced.
- **The tools** (`ink-tools/src/tools/text.rs`): `text_add` makes a
  `<text>` at a baseline's start, `text_set` changes what one says and
  how all of it is lettered, `font_list` names the families installed
  and what the generic names stand for. A family that isn't installed
  is written as asked (another machine may have it) and reported: the
  reply, and `node_info`, say which font each part ended up in and
  which of the families asked for aren't here (`text::lettered`).
- **`TextToPath` writes a text out as the outlines it's drawn as**
  (`ink-doc/src/outlined.rs`, the tool `text_to_path`), so it looks the
  same on a machine without its fonts, and in Lantern's apps, which
  draw no text. A text painted one way becomes one `<path>` in its
  place, with its id and its paint; one whose spans paint for
  themselves becomes a `<g>` of paths, each with what its span gave it
  (the nearest element to say a property wins). What only letters read
  (positions, fonts, spacing, `white-space`) goes with the letters;
  `fill-rule` is set to `nonzero` where the text was told otherwise. A
  text set in another font than it asks for is refused unless told to
  go ahead (`as_drawn`): its paths would be the other font's for good.
  **What changes:** anything measured across a box with the text in it
  (a gradient across the text, the glow region of a group it's in) is
  measured across a tighter box afterwards, since a text's box is its
  glyphs' cells and a path's is its outline. `ink-render/tests/text.rs`
  draws every text of the text golden and of the corpus before and
  after: thirteen of the nineteen corpus files are within 0.002 levels
  of what they were, and the six with such a box 0.04 to 0.20 apart.
- **Tests set text in fonts of their own** (`tests/fonts/`, family
  "Ink Test", made by `ink-doc/tests/font.rs`, which holds the files to
  what it makes): boxes for letters and a ring for an `o`, 1000 units
  to the em, no kerning, so every edge of a set text is a number a test
  can say and every machine draws the text golden to the same bytes.
  The nineteen corpus files with text are in real fonts, so they're
  read beside `rsvg-convert`'s pictures, not measured: twelve looked at
  on 2026-10-07 sit where its do, in Lantern's fonts.

---

## 6. Headless MCP server (ink-tools + ink-mcp)

LS3 §6 and its `docs/m0/mcp-protocol.md` hold the research. It all
carries over; only the differences and the tool list are new here.

- **Binary `lantern-ink-mcp`, server name `ink`**, so tools appear as
  `mcp__ink__node_add`. Registering it writes `~/.claude.json`, so it
  waits for Alva's OK at M2 (D23): user scope, `alwaysLoad`.
- **Same server as LS3's:** stdio, one JSON-RPC message a line, stdout
  carries only protocol, both protocol eras answered in any order, one
  thread applying calls in order, `structuredContent` beginning with a
  `message` (LS3 found Claude Code shows the model nothing else), errors
  as `isError` results that say how to fix the call.
- **No GPU**, so no "can't serve without one".
- **One tool = one Command = one undo step.** `batch` is one atomic step
  whose steps name what they make (`as: "leaf"` → `"@leaf"`). No implicit
  selection or active anything: every call names its IDs, every creating
  call returns them. There is no arbitrary-code tool.
- **Conventions:** attributes as the file writes them (D16): the
  element's own coordinates, y down; any SVG paint string; numbers
  written with at most three decimals (D15); file paths absolute, `~/…`,
  or relative to the project.
- **Previews are a strength here:** a vector draws sharp at any size.
  `doc_preview` takes a size, a region, a background (checker, white,
  black, none) and optionally only some nodes; `renderer: "lantern"`
  gives §5.4's strip. It returns the image plus a full-size PNG on disk.
- **Claude already writes SVG**, so two tools speak it directly:
  `doc_source` (the markup of a document or a node) and `node_add_svg`
  (a fragment parsed, validated and inserted as one undo step).
- **Unsaved documents** are autosaved while idle and at EOF, to
  `~/.lantern/config/lantern-ink/autosave/` (LS3's lesson from `kill -9`).

**Draft tool list** (about 30; \* = the M2 slice):

| Group | Tools |
|---|---|
| Documents | `doc_new`\*, `doc_open`\*, `doc_list`\*, `doc_info`\* (the tree, front to back, as a layers panel shows it), `doc_preview`\*, `doc_source`\*, `doc_save`\*, `doc_export`\* (PNG / JPEG / WebP at any size; a clean, tidied `.svg` to ship since M3e), `doc_close`\*, `doc_set`† (viewBox, size, decimals; fitting the content to a new viewBox), `doc_tidy` (drop what nothing uses; built in M3e) |
| Nodes | `node_add`\* (any element, with its attributes as the file writes them), `node_add_svg`\*, `node_set`\* (any attribute; null takes one off), `node_info`†, `node_move`\*, `node_duplicate`†, `node_delete`\*, `node_group`†, `node_ungroup`†, `node_transform`†, `node_align`† († = built in M3a), `node_mark` (a label, a lock; built in M3e) |
| Paths | `path_set`§, `path_edit`§ (anchors and handles), `path_op`§ (to path, reverse, the boolean ops union, subtract, intersect and exclude, outline stroke, simplify) (§ = built in M3c) |
| Paint | `node_style`‡ (properties set where they'll show: not in the first list, added because `node_set` writes attributes as given and can't follow D14), `gradient_add`‡, `gradient_set`‡, `clip_set`‡, `filter_set`‡ (‡ = built in M3b) |
| Text | `text_add`¶, `text_set`¶, `text_to_path`¶, `font_list`¶ (¶ = built in M3d) |
| Queries | `doc_query`† (what's at a point; a node's bounds are `node_info`'s) |
| History | `history_undo`\*, `history_redo`\*, `batch`\* |

---

## 7. Live bridge

As LS3 §7, to the letter where it can be:
- The window listens on `$XDG_RUNTIME_DIR/lantern-ink.sock` (mode 0600)
  **only while Alva lets Claude in**, and that switch is off at every
  launch.
- The MCP process forwards `tools/call` lines; the window runs the same
  dispatcher on its own core. `w…` ids are the window's, `d…` a server's;
  `doc_to_window` hands one over.
- Claude's Commands queue while Alva has a gesture going. History entries
  are tagged, and her Ctrl+Z undoes them.
- Her unsaved work is never Claude's to throw away, and a locked node
  refuses Claude as it refuses her.
- Simpler than LS3's in one way: with no GPU readbacks, only encoding and
  file work is staged off the window's thread.
- Whether Claude's paths draw themselves on screen, as LS3's strokes do,
  is decided at M5 (D24).

---

## 8. GUI (ink-app)

- **Layout:** one LUI2 area, and Ink draws everything inside it the way
  LS3 does: the toolbar down the left, the rainbow strips, document tabs
  (with their own "save first?"), the canvas, the right panel, the
  floating Box for tool and selection settings, and the full-width status
  bar with zoom and the CLAUDE pill. LUI2 gives the title bar and menus,
  popups, dialogs and the keymap. The logo reads `L A N T E R N   I N K`.
- **The look** is LS3's tokens verbatim (`studio-app/src/theme.rs`):
  the warm dark panels, gold accent, the sheen, 2 px rules (D12).
- **The right panel:** the object tree (what `doc_info` lists: eye, lock,
  drag to reorder, nested groups), fill and stroke, colour, and the
  Lantern preview strip.
  **As built in M4b** (`ink-app/src/tree/`, `select.rs`): every
  element is a row, front to back, groups open and everything else
  that holds elements shut (`<defs>`, a text over its spans, a gradient
  over its stops). The tree draws the drawing as it looks and asks the
  window for what it wants done (`tree::Intent`); the window applies
  each as one Command (`edits.rs`). What's selected, what's open and
  what's being renamed are the tab's, not the document's.
  **The paint section, as built in M4c** (`ink-app/src/colour/`,
  `paint.rs`, `painting.rs`), over the tree:
  - A row each for Fill and Stroke (Alva's choice of layout): a swatch
    that opens the picker, the colour in hex beside it, and what kind
    of paint it is. It shows the paint of what's selected, read as it's
    drawn (what a node says over what it inherits); with nothing
    selected, what the next shape drawn will get.
  - **A paint set goes on every shape and text of the selection**
    (through its groups: set on a group it would be outvoted by
    whatever in the group says its own), as `Command::SetStyle`: the
    colour as `#rrggbb`, its opacity beside it as `fill-opacity` or
    `stroke-opacity`, taken off when it's whole.
  - **The picker, the palettes and their drawer are LS3's**, copied
    (D12): one picker for the window, floating by the swatch it was
    opened for. A colour dragged in it (or a palette swatch held) is a
    gesture in the core, landing as one step when the button comes up.
  - A palette swatch pressed is the fill; with Shift, the stroke.
    Ink's palettes are its own file; the first time, Lantern Studio's
    are read in its place (Alva's choice).
  - **The line's rows** (`colour/line.rs`) are the stroke's width
    (dragged along or typed), its ends and corners (three ways each),
    and its dashes (typed as lengths, on then off); they wait, dim,
    while there's no stroke. Each is said outright (`butt`, `none`), so
    a group above saying otherwise doesn't decide it.
  - **The opacity is the selected thing's own** (`opacity`, on a group
    as on a shape: a group fades as one), where a paint and a line go
    on every shape inside.
  - **A gradient is a kind of paint:** making a paint one defines a
    linear gradient across the shape's box, top to bottom, from the
    colour it was to that colour darker, and paints with it, as one
    step; making it one again uses the gradient it last was, while the
    drawing still has it. Its stops are the Gradient tool's to edit
    (slice e).
  - **The tree keeps its room:** at least 38 % of the panel (220 px).
    Where the panel is too short for all of the section, the section
    scrolls in the rest; its heading folds it away.
- **The canvas:** the CPU renderer draws 256 px tiles on the job pool at
  the current zoom, and the window shows them as LUI2 images. While
  zooming, the old tiles stretch until sharp ones land; a frame never
  waits for a tile. Only tiles an edit touches are redrawn. If handing
  tiles through LUI2 proves too slow at 4K, a present pass of Ink's own
  goes in one module of `ink-app`, measured first.
  **As built in M4a** (`ink-app/src/tiles.rs`):
  - A tab's tiles are a *level*: the drawing in one state (its
    history's stamp), at one zoom. When either changes, a new level is
    drawn behind the one that shows, and they change places once every
    tile in view has landed. So the canvas never goes blank, and never
    shows half of one state beside half of another.
  - The pool lays the drawing out once for a level (a `Plan`, §5.1),
    from a copy of the document taken on the window's thread (a map of
    shared nodes: cheap), then draws tiles from it, the nearest the
    middle of the view first. Only as many are on the pool at once as
    it has threads (less one, kept for the next layout) and as a budget
    of pixels allows; what's still waiting when the view moves on is
    never begun.
  - Tiles are 256 px where shadows reach no further than 64 px, 512 up
    to 256, 1024 beyond: a tile is drawn with its shadows' reach
    around it, and small tiles would be mostly margin.
  - A tile nothing is painted in (`Plan::bounds`) is never sent, and
    one that comes back clear is never uploaded.
  - The page's corner shows on a whole window pixel (`camera.rs`), so
    tiles made for the zoom land pixel for pixel; stretched ones have
    their edges rounded to whole pixels, shared by neighbours.
  - The four tabs shown most lately keep their tiles.
  - Measured (`cargo test --release -p ink-render --test speed tiles --
    --ignored --nocapture`, every corpus file fitted to a 4K canvas):
    a screen of an icon with no shadows is 25 to 125 ms of one core,
    so sharp within a frame or two on the pool. One with drop shadows
    (most of Lantern's app and folder icons) is 1.5 to 3.5 s of one
    core, a tenth to four tenths of a second on the pool: each tile is
    drawn with its shadows' reach around it, and a filter is worked
    out in linear light over all of that. **For the renderer, when
    it's measured again:** a filter needn't touch the pixels of its
    frame that are clear.
  **As built in M4b** (only the tiles an edit touches are drawn again):
  - `Plan::changed_from` holds a drawing laid out against itself laid
    out a moment before, thing by thing in paint order, and gives the
    boxes where the two pictures can differ. A pixel is made of the
    things that paint it, in their order: where none of those changed,
    came or went, it's the same. Inside a layer, what changed counts as
    far as the layer's filter carries it (its stages' reach) and no
    further than the layer shows; a filter's region that moved with its
    element's box counts only where something is painted in the part
    that came or went (all of it, for a filter that floods).
  - A new level at the same zoom takes every tile of the level that
    shows that those boxes don't touch, picture and all (a tile's
    picture is shared by the levels that show it, and freed when the
    last lets go), and only the rest go to the pool. An edit that
    changes nothing that shows draws nothing.
  - **How it's proven:** `ink-render/tests/changed.rs` edits corpus
    files a dozen ways each (moved, scaled, repainted, faded, hidden,
    restacked, taken out, put in, a stop or a blur changed) and holds
    every tile the boxes leave alone against what it was, to the byte:
    every eighth file with `cargo test`; all 144 (5,609 edits, 399,268
    tiles the same, three quarters of all there were) with
    `cargo test --release -p ink-render --test changed -- --ignored`.
  - **A drag's levels:** while the look changes at every frame, the
    level on its way is let land before the next is begun (begun again
    each frame, none ever would), so the canvas follows a drag as fast
    as its tiles can be drawn. The selection's outline is the window's
    own, drawn every frame: that never waits.
  - **Measured** (`cargo test --release -p ink-render --test speed
    drags -- --ignored --nocapture`: a step of a drag on a 4K canvas,
    for the bottom, the top and a deep node of every corpus file, on
    27 threads): applying the Command is 0.1 ms, laying the drawing out
    0.1 to 1.5 ms (84 ms where there's text: shaping, every time),
    finding what changed 0.01 ms. The tiles are everything: **a step is
    79 ms at the median, 178 ms at nine in ten, 643 ms at worst; 38 %
    within a frame.** Half the tiles are kept on average, but one
    1024 px tile under a wide shadow is 150 to 640 ms on its own.
  - **So, for drags that keep up on shadowed icons** (most of Lantern's;
    not built, each to be measured first): (a) the things of a drawing
    that a drag doesn't touch drawn once and kept, per tile, under and
    over the ones it does, so that what's dragged over a shadowed body
    doesn't have the body's shadow worked out again; (b) wide blurs
    worked out small and stretched while a drag is going, exact again
    once it rests; (c) the filters themselves (every stage a frame of
    its own, in linear light, clear pixels and all).
- **Overlays** (selection boxes, handles, anchors, guides, the pixel
  grid) are drawn in screen px over the canvas with LUI2's own lines, so
  they stay the same size at every zoom.
  **The Pointer, as built in M4b** (`ink-app/src/pointer.rs`,
  `handles.rs`, `overlay.rs`):
  - **A click picks the thing on top at the level the Pointer is in**
    (`pointer::pick`, on `hit::at`): at the drawing's top level, the
    outermost group a shape is in. A double-click on a group goes into
    it, and clicks then pick one level down; a click outside it, or
    Escape, comes back out. Nothing locked is picked: a click goes
    through it. Shift adds or takes away; a drag on nothing is a
    marquee, which catches a shape by its box and a group by what it
    holds.
  - **The selection's box** is round what's selected and drawn (the
    boxes of `geometry::page_bounds`, strokes aside): sixteen-px white
    handles at its corners and sides, gold over a dark edge so it shows
    on the tan ground and over any drawing. Inside it a drag moves; a
    corner or a side scales (Shift keeps the shape, Alt about the
    middle); within 34 px outside a corner it turns (Shift in steps of
    15°). `handles::dragged` is what each drag comes to, as one affine
    in the drawing's coordinates.
  - **A drag of the box is a gesture** (§4.3): `Command::Transform` for
    a move or a turn, `Command::Resize` for a scale (Transform, with
    "Scale strokes" on), said again each frame from where the drag
    began. The box and its handles are worked out by the window from
    the box as it was and that affine, so they keep up with the pointer
    whatever the tiles are doing.
  - **A resize keeps lines as they are** (Alva's call; `settle.rs`,
    `keep`): a stroke's width and dashes, and a rect's corner rounding.
    So a stroked shape stretched one way still takes it into its own
    numbers, and a circle becomes an ellipse to do so. Where the
    numbers can't say it (a rect scaled across the way it's turned;
    anything under a shadow) it falls back to Transform's way, stroke
    and all.
  - The arrow keys nudge by a unit of the drawing (ten with Shift),
    each press a step. In the middle of a drag no key does anything but
    Escape, which gives the drag up.
  **The shape tools, as built in M4c** (`ink-app/src/shapes.rs`,
  `shaping.rs`):
  - **A drag draws one new shape, always** (LS3's rule), on top of the
    level the Pointer is in, in that level's own coordinates: a
    `<rect>`, an `<ellipse>` (a `<circle>` when it's round), a `<line>`
    or a `<polygon>`. It's a gesture (§4.3): `Command::Insert`, said
    again each frame with the shape as dragged so far, landing as one
    step. The shape just drawn is selected; the tool stays in hand.
  - Shift draws a square, a circle, a regular polygon, or a line at a
    multiple of 45°; Alt draws out from the middle (Alva's choices in
    LS3). A polygon fills the box dragged (stretched, unless Shift
    keeps it regular); a star is a polygon with as many corners again
    between, part of the way in.
  - **A new shape lands on the grid**, as LS3's land on whole pixels,
    or on a line of what's there; Ctrl draws free of it (snapping,
    below).
  - **A new shape is painted as the last one was** (`Ink::paints`: the
    fill, the stroke, its line and the opacity last set in the paint
    section). A line is all stroke: its own, or the fill's colour.
  - The Box holds a tool's own settings where it has any: a
    rectangle's corners; a polygon's sides, whether it's a star, and
    how deep its points go.
  **A drawn shape's own handles, as built in M4c** (`rounding.rs`,
  `polygons.rs`, `shapebox.rs`):
  - **Both come to one Command, `SetGeometry`**: the shape's own
    numbers (`ink-doc`'s `Geometry`), written only where they changed.
    A rectangle stays a `<rect>` with an `rx`; a polygon a `<polygon>`
    with other `points`.
  - **A rectangle picked alone has a round dot** inside its first
    corner (one, Alva's choice): dragged toward the middle it rounds
    all four corners, back toward the corner it squares them. It
    stands a little in from the middle of the corner's arc, so with no
    rounding at all it's clear of the box's handle on that corner, and
    it follows the pointer along its own line from wherever it was
    taken hold of. It's the rectangle's own (it turns and leans with
    it), where the box round it is always upright. Too small on the
    screen for the dot to be clear of the box's handles, a rectangle
    has none: the Box's number still rounds it.
  - **A polygon's sides are read off its corners.** The file says
    nothing of how it was drawn, so `Regular::read` finds the circle
    its corners best fit (a middle, and where the circle's one unit
    across and one down have gone: any stretch, turn or lean since is
    in those) and holds every corner to it, as a regular polygon and
    then as a star. Other sides go on the same circle (Alva's choice:
    its middle and reach stay, not its box), so a regular one stays
    regular.
  - **What the file's rounding costs:** the circle is read off rounded
    numbers, so it's the drawn one to the file's last decimal, and
    other sides and back again is the same polygon to that decimal,
    not always to the byte. The circle isn't moved onto rounder numbers
    to hide that: a polygon drawn to fill a box of whole units has a
    circle of awkward ones.
  - **The Box holds the selected shape's rows under the Pointer**, the
    same rows its tool shows for the next shape, after the place and
    size: set on the one in hand, they're set on every selected shape
    of its kind (LS3's rule). Dragged, a gesture; typed or ticked, a
    step at once.
  **The Node tool, as built in M4d** (`nodes.rs`, `anchors.rs`,
  `noding.rs`):
  - **Every selected shape shows its anchors** (white squares on the
    shape's line in gold; LS3's look for its pen's), in place of the
    selection's box. A press takes a handle of a picked anchor, else
    the nearest anchor, else a segment; else it's on a shape, which is
    picked itself, whatever group it's in, or on nothing, where a drag
    is a marquee over anchors.
  - **Each drag is a gesture of path edits**: the picked anchors moved
    (onto whole units, led by the one pressed; Ctrl frees it, Shift
    keeps to one axis), a handle moved, or a segment pulled by the
    point taken. They land as "Move Anchor(s)", "Handle" and "Bend".
  - **A shape that isn't a path yet shows the anchors it would have,
    and becomes a path with the first change to them.** Looking and
    picking change nothing. Its anchors are called what they'd be
    called were it made a path now, by itself; a change that makes
    several shapes paths at once begins with one `ToPath` of them all,
    whose order says what each anchor is then called.
  - **Smooth is how the handles lie**, not something kept: in line
    through their anchor, one dragged takes the other round (each its
    own length); Alt, or a corner, and each goes alone.
  - **What's done to the anchors picked** (`nodeops.rs`) is one step
    each: made smooth or corners, the path parted at them, two loose
    ends of one path joined, taken out, a new one put on a segment.
    They're the Box's buttons (greyed while there's nothing for them
    to do), the rows of the menu a right press opens, Delete, and
    double clicks: on a segment for a new anchor there, on an anchor
    to turn a corner smooth and back (LS3's pen's way). The Box also
    has the one picked anchor's X and Y, to type or drag along.
  **The Pen, as built in M4d** (`penning.rs`; LS3's, D17):
  - **A press on bare canvas places an anchor**, on whole units of the
    drawing (Ctrl frees it); dragged on, it pulls the anchor's handles
    out, one each way (Shift: at 45°). A click is a corner. The path
    goes on from the one anchor picked when that's a loose end of an
    open path, which is how the path being drawn is drawn, and how an
    old one is taken up again from either end; otherwise the press is
    a new path's first point.
  - **A path begins with its second point**: the first is the Pen's
    own until then, and so is the handle pulled out of a path's end,
    which has no segment to be in till the next point is placed.
  - **A press on the path's other end closes it.** Enter or Escape
    lets go of the path; Delete takes its end back off.
  - **The Node tool runs under the Pen**: whatever of the selected
    shapes the pointer is on (an anchor, a handle, a segment) is
    dragged as the Node tool drags it, so a path is bent without
    putting the Pen down.
  **The Path menu, as built in M4d** (`pathops.rs`): each row one of
  §3.4's Commands on the selection, lit while there's something for it
  to do. Union, Subtract, Intersect and Exclude make one shape of the
  shapes selected, the one furthest back taking the result and keeping
  its paint (Subtract: it, less the ones in front). Object to Path,
  Outline Stroke, Simplify and Reverse are for every shape the
  selection is or holds. The same rows are under "Path" in the menu a
  right-click on the selection opens.
  **Slice e's small things, as built** (`eyedrop.rs`, `effects.rs`,
  `shadows.rs`):
  - **The Eyedropper takes the colour you see** (Alva's choice): one
    pixel of the drawing itself, drawn eight times finer than the
    screen shows it with the point in its middle, so it's the colour
    there through gradients, opacity and shadows. It's the selection's
    fill and the next shape's (Shift: the stroke).
  - **Object > Clip**: the thing on top of the selection, a shape,
    cuts the rest (several under it are grouped, and the group is
    cut). **Release Clip** takes it off again. **Text > Text to Path**
    asks before making paths in a font the text didn't ask for.
  - **Object > Drop Shadow and Blur** give the selection a filter of
    one step, as Claude's `filter_set` does (the same arithmetic for
    its room: `ink_doc::filter::region`), and open the Box, where its
    settings are while the thing is selected. A filter is changed
    where it is while it's the selection's alone.
  **The Gradient tool, as built in M4e** (`grads.rs`, `grading.rs`):
  - **A drag across a shape is its gradient's line**: from end to end
    for a linear one, from the middle out for a radial one. A shape
    with a plain colour gets a new gradient of that colour, measured by
    its box so it goes where the shape goes. The line's ends stay on
    the canvas as handles.
  - **Its stops are on a bar in the Box**: put there by a press,
    dragged along, recoloured, and taken off.
  - **A gradient is changed where it is while it's that shape's
    alone**; a shared one is left, and the shape gets its own.
  **The Text tool, as built in M4e** (`typing.rs`, `texting.rs`,
  `textbox.rs`):
  - **A click and typing make a text**; a click on a text takes it up,
    the caret where the click is. Enter starts a line; the arrows, Home
    and End move about; Backspace and Delete take characters out.
  - **Typing is tried on the canvas as it goes and is one step once it
    pauses** for half a second, or when anything else is done (LS3's
    rule). A text begins with its first character, and goes with its
    last.
  - **A text is read as lines of stretches** (`text::written`) and
    written back the same (`SetText`), so the spans a file has are
    kept through typing. The caret is placed by each character's own
    cell in the text as it's set (`Laid::chars`).
  - **The Box letters it**: the font (every family installed, each in
    its own face), size, bold, italic, alignment, line height; and
    says so when the font a text asks for isn't on this machine, with
    what drew instead.
  **Snapping and the pixel grid, as built in M4f** (`snap.rs`,
  `snapping.rs`, `grid.rs`):
  - **Everything dragged lands** on the grid or on a line, each way by
    itself, and whichever is nearest wins. The grid is whole units of
    the drawing, and half ones once those are six px apart on the
    screen (on a page of only a few units, as fine as leaves it some
    sixteen steps across). The lines are the edges and middle of the
    page, of every shape and text that shows, and of what stands at
    the Pointer's level; the Node tool and the Pen add the anchors
    that stay where they are. A line draws things from eleven px away
    (LS3's reach), the grid from wherever they are; a line as near as
    the grid wins.
  - **What lands:** a shape tool's two corners, a Pen's point, a
    dragged anchor (the one pressed), a new text's place, and the
    selection's box: moved whole, its left, middle or right on a line
    or an edge of it on the grid (with Shift, only the way it goes); a
    side, the one way it goes; a corner, as a point, or with Shift
    along its diagonal. A turn, a handle of a curve, a bent segment
    and a gradient's ends are free.
  - **A line landed on shows** right across the canvas, in LS3's cyan,
    for as long as the drag is on it (the Pen's shows before the press:
    where a point would go). The grid isn't shown as landed on: the
    pixel grid is what shows it.
  - **A tool asks for its lines before it reads the drawing**
    (`Ink::snap_to`, kept from frame to frame while nothing they hang
    on changes), lands its point or box on them, and leaves what it
    landed on in `Ink::landed` for that frame's overlay.
  - **View > Snapping is the switch; Ctrl holds it off.** What it lands
    things on (the grid, the shapes) is Preferences'.
  - **The pixel grid** is a line at every whole unit across the page,
    drawn over the drawing in screen px, once units are eight px
    apart; View > Pixel Grid hides it.
  **The menus' work and the Box, as built in M4b** (`ops.rs`,
  `boxes.rs`, `toolbox.rs`, `controls/`):
  - **Every row of Edit and Object is one Command on the selection**
    (`Ink::op`): what's selected afterwards is what the step made or
    left (a copy, a new group, a group's children). The same rows are
    on the keys and in the menu a right-click opens, on the canvas or
    on a row of the tree (`selection_menu`).
  - **The clipboard is SVG text** (`ink-doc/src/paste.rs`). A copy is a
    drawing of its own (`Document::clipping`): the things picked, each
    out at the top level showing where it showed, with the definitions
    they're drawn with, under the drawing's own `<svg>`. A paste is
    `Command::Paste`: what that drawing draws goes on top of the level
    the Pointer is in, where it was copied from; a definition the
    drawing has already is used as it is, and any other `id` that's
    taken gets another. It goes out as plain text (LUI2 has text and
    pictures on its clipboard): apps that read pasted SVG markup take
    it; one that wants the `image/svg+xml` type would need that added
    to LUI2.
  - **Lining up is one reckoning** for the window and for Claude's
    `node_align` (`ink-doc/src/arrange.rs`): several things against the
    box round them all, one alone (or with "To the Page" ticked)
    against the page.
  - **The Box** is LS3's: a gold square at the canvas's top right that
    opens a panel. With the Pointer in hand it holds the selection's
    X, Y, W and H (dragged along, a gesture in the core that lands as
    one step; typed in, one step at once; scaled about the top left
    corner) and the Pointer's own "Scale strokes". Other tools'
    settings come with their tools.
  - **A group that would lose something by being ungrouped is asked
    about** (a dialog, "Ungroup anyway?") instead of refused: §3.4's
    refusal is for a caller who can say `drop`.
- **Tools, first set:** Pointer (select, move, scale, rotate), Node
  (anchors and handles; drag a segment to bend it), Pen (LS3's: a
  click places a corner, a press dragged on pulls its handles out;
  D17 as Alva changed it), Rectangle, Ellipse, Line,
  Polygon, Text, Gradient, Eyedropper, Hand and Zoom.
- **For icons:** a pixel grid when zoomed in, snapping to whole and half
  units, and the preview strip always in view.
- **Alva's rules are requirements:** 18 px minimum text, generous
  targets, the sheen and 2 px borders, no help toasts, draw once and
  adjust forever (persistent handles; the Pointer adjusts everything),
  the Box owns every tool and selection setting.
- **Robustness:** autosave on the job pool, a last autosave from the exit
  hook, unsaved-changes prompts on tabs and on close, file pickers through
  `lntrn-file-manager` out of process (LS3 D15).

---

## 9. Files, paths, processes

| What | Where |
|---|---|
| GUI binary | `~/.lantern/bin/lantern-ink` |
| MCP binary | `~/.lantern/bin/lantern-ink-mcp` |
| Config (prefs, recent, keymap) | `~/.lantern/config/lantern-ink/` |
| Autosave | `~/.lantern/config/lantern-ink/autosave/`, each file recording its owner's PID |
| Logs | `~/.lantern/log/lantern-ink.log`, `lantern-ink-mcp.log` |
| Live-bridge socket | `$XDG_RUNTIME_DIR/lantern-ink.sock` |
| Full-size preview PNGs | `~/.lantern/cache/lantern-ink/previews/` |

- **Documents are `.svg`.** Saves are atomic (a synced temporary file
  renamed over the old one).
- **A file has one document at a time** in a core: opening one that is
  open already gives the document it is, and no other document can be
  saved over it.
- The repo's first branch is `main`, like LS3's and LUI2's (it was
  created as `master` and has no commits yet).

---

## 10. Testing and determinism

- **The corpus:** the Lantern projects' own SVGs (193 files, 144 distinct,
  0.9 MB), copied into `tests/corpus/` so the tests need no other repo.
- **Round trip:** every corpus file opens and saves to the same bytes.
  After any Command and its undo, the same bytes again. After one edit,
  only the bytes of what it touched differ.
- **Geometry:** flattening tolerance, exact areas, stroke joins and caps,
  boolean ops, hit tests, as CPU unit tests.
- **Boolean operations** (as built in M3c) are proven three ways, none
  by eye. Thousands of pairs of shapes set on a grid (so that they
  share sides, corners and touches far more than chance would), the
  results fed back in, and every neighbouring pair of shapes in the
  corpus: each result is checked point by point (a place is in it
  exactly when the operation says so of the shapes, by counting
  windings), to the last digit (the four operations' exact areas add
  up as they must), and against the renderer, which draws one shape
  clipped by the other and knows nothing of how the results were
  made.
- **A stroke's outline** is held to the stroke's own definition, place
  by place (round all over, a stroke is everywhere within half its
  width of the line; cut off square, everywhere a square-on line from
  it reaches that far), drawn beside the renderer's own stroke of the
  same line, and tried on every stroke in the corpus, which has to
  draw as it did. **A simplified path** is measured against the path
  it was, both ways.
- **Render goldens:** fixed documents to RGBA, exact (pure CPU). Agreement
  with `lntrn-svg` and, when present, `rsvg-convert`, each test declaring
  its tolerance (D22).
- **Replay:** N Commands, save, load, render: must match. Undo
  everything: must match the start.
- **MCP transcripts:** lines in, lines out, in `cargo test`, both eras.
- **UI logic** with `lntrn_ui::testing::Harness`. **Visual checks are
  Alva's.** Nothing in this project ever captures the screen.
- **Determinism:** no wall clock or randomness in a Command; fonts named
  in the file, a missing one reported rather than silently swapped
  (it gives way to the next family named, and the text tools and
  `node_info` say which font was used and which weren't here, §5.5).
  Tests set text in fonts of their own (`tests/fonts/`).

---

## 11. Milestones

| M | Deliverable | Done when |
|---|---|---|
| **M0** ✅ | This doc and its decisions | Alva approves it and answers the "before M1" rows of §12: she did, 2026-10-05 |
| **M1** ✅ | The workspace; `ink-geom`, `ink-doc`, `ink-render`, `ink-core` | Every corpus file round-trips byte-identical, renders in agreement with `lntrn-svg`, and survives edit → undo unchanged. Core saves, loads and exports PNG, headless. Built 2026-10-06; the done-test is `ink-core/tests/m1.rs` |
| **M2** ✅ | `ink-tools` + `lantern-ink-mcp`, the \* tools | Registered (with approval). Claude draws an icon headless, previews it, and saves an `.svg` a Lantern app shows 🎉. Built, deployed and registered 2026-10-06 (17 tools); the done-test passed in a fresh Claude Code session the same day (a session's tools are fixed when it starts) |
| **M3** ✅ | Operations: every Command in §3.4 as a Command + tool + test, in five slices: **a** structure and transforms (built 2026-10-06), **b** paint (built 2026-10-07), **c** paths (built 2026-10-07), **d** text (built 2026-10-07), **e** tidy (built 2026-10-07) (Alva's order, 2026-10-06) | Path editing, transforms, align, gradients, clips, text, boolean ops, tidy export all work over MCP. They do: the done-test (a stress sheet, all 39 tools, a fresh session) ran 2026-10-07, and what it found was fixed the same day |
| **M4** | `lantern-ink`, the window, in the LS3 look, in six slices (Alva's order, 2026-10-07): **a** the shell and the viewer (built 2026-10-07), **b** the object tree, the Pointer and undo, **c** paint and the shape tools, **d** the Node tool and the Pen, **e** text, gradients and the eyedropper, **f** the icon aids and not losing work | The scope checklist written with Alva at M4's start (D20) is `docs/M4.md`: every box ticked or struck by her |
| **M5** | The live bridge | Alva watches Claude draw in her window, with shared undo |

LUI2 changes Ink is known to want so far: possibly one new crate (D11).
M1 needs none. Anything found later comes up one at a time, as with LS3.

---

## 12. Decisions

My recommendation is listed first for each. Rows marked "before M1" shape
the foundation; the rest wait for their milestone.

| # | Decision | Recommendation | When |
|---|---|---|---|
| D1 | Stack | ✅ **Decided 2026-10-05:** Rust + wgpu + LUI2, our own vector code. The May plan (iced, kurbo, usvg, vello) is retired | — |
| D2 | The file on disk | ✅ **Decided 2026-10-05:** a plain `.svg`; Ink's extras in `ink:` attributes; what Ink can't edit is kept | — |
| D3 | What it's for first | ✅ **Decided 2026-10-05:** Lantern icons and cursors, then logos and branding | — |
| D4 | Process | ✅ **Decided 2026-10-05:** this doc first, then LS3's order (core, MCP, GUI, bridge) | — |
| D5 | The document model | ✅ **Decided 2026-10-05, as recommended: the SVG's own tree is the truth**, with typed views on it; an untouched file saves byte-identical and a small edit is a small diff (§3). The alternative, a typed model of Ink's own that writes the whole file in Ink's formatting, is simpler to code against but reformats every file it touches and has to model all of SVG or lose it | Before M1 |
| D6 | Crates | ✅ **Decided 2026-10-05, as recommended:** the seven in §2 | Before M1 |
| D7 | The renderer | ✅ **Decided 2026-10-05, as recommended: one CPU renderer** for the window, previews and exports; no GPU below the window (§5.3). The alternative, a GPU renderer for the window, is a second renderer to keep in agreement, for speed D3's work doesn't need | Before M1 |
| D8 | Where the vector code lives | ✅ **Decided 2026-10-05, as recommended: in Ink for now** (`ink-geom`, `ink-render`), written so it can move to LUI2 and replace `lntrn-svg`'s and LS3's copies once it has settled. The alternative is building it in LUI2 from day one | Before M1 |
| D9 | Names | ✅ **Decided 2026-10-05, as recommended:** `lantern-ink`, `lantern-ink-mcp`, MCP server `ink`, crates `ink-*`, the paths in §9, branch `main` | Before M1 |
| D10 | Addresses | ✅ **Decided 2026-10-05, as recommended:** docs `d1` / `w1`, nodes `N7`, alive while the document is open and not written to the file; an element's own `id` is just an attribute | Before M1 |
| D11 | MCP plumbing | ✅ **Decided 2026-10-06, as recommended: a new LUI2 crate, `lntrn-mcp`** (JSON-RPC lines, both protocol eras, schema pieces, cancellation, the socket pipe): additive, nothing existing changes, and LS3 can move onto it whenever you like. The alternative is copying about 1.5k lines out of `studio-tools`, to be fixed twice whenever MCP changes | M2 |
| D12 | The Studio look | ✅ **Decided 2026-10-07, as recommended: copy** LS3's theme, layout, chrome and controls into `ink-app` (about 1.8k lines); a shared crate once we see what the two apps really share. U004 and U042 keep app looks out of LUI2 | M4 |
| D13 | Moving and scaling | ✅ **Decided 2026-10-06, as recommended: bake into the geometry whenever that's exact**; keep a `transform` only where it isn't (a rotated rect stays a `<rect>` with a `rotate`, so its radius stays adjustable). The alternatives were always a `transform` (the numbers stop saying where things are) and always baking (a rotated rect becomes a path). **For groups, decided the same day: passed down** to what's in them when all of it can take it exactly, kept as the group's own `transform` otherwise (§3.4). **And for what a node is drawn with** (2026-10-06 and 07): a gradient in its coordinates, and its clip path, go with it when they are its alone, and are never touched when anything else uses them | M3 |
| D14 | Where a style is written | ✅ **Decided 2026-10-06, as recommended:** where that node already has it (`style=""` or the attribute); a new property goes in as a presentation attribute | M3 |
| D15 | Numbers Ink writes | ✅ **Decided 2026-10-06, as recommended:** three decimals, trailing zeros dropped, settable per document | M3 |
| D16 | Coordinates Claude and the GUI speak | ✅ **Decided 2026-10-06, revising my first recommendation:** attributes are as the file writes them (the node's own coordinates, as in any SVG), which is also what `node_add_svg`'s raw markup means; what's reported back is where things show in the document's coordinates (§3.3) | M2 |
| D17 | Pen tool | ✅ **Decided 2026-10-09, changed from the plan:** LS3's Pen. A click places a corner; a press dragged on pulls the new anchor's handles out; segments bend after, as ever. (The plan was her May preference, click only with no handles while placing: a plain click is still that.) New points and dragged anchors land on whole units, Ctrl frees them | M4 |
| D18 | Path anchors' addresses | ✅ **Decided 2026-10-07, as recommended:** stable ids kept beside the path in memory (`A3`), so a selection survives a point being added; not written to the file. The alternative was a place in the path (run 1, anchor 3), which every add and delete renumbers | M3 |
| D19 | Ink's own attributes | ✅ **Decided 2026-10-07 (Alva): `ink:label` and `ink:locked` now** (built in M3e, §3.3), under `xmlns:ink="urn:lantern:ink"`; **guides wait for the window (M4)**, the first thing that can show or snap to them. Nothing else until something needs it | M3 |
| D20 | What "done" means for the window | ✅ **Decided 2026-10-07, as recommended: Ink replaces Boxy SVG for Lantern's icons.** All twelve tools of §8, the object tree, fill and stroke, the icon aids, and a menu row for every operation the MCP server has. The checklist is `docs/M4.md`. The first deploy takes the `lantern-ink` name from the May prototype | M4 |
| D21 | SVG features | §5.2's list for v1; `<use>`, masks, patterns, images and markers when something needs them. M1 draws what `lntrn-svg` does; text, `<style>` rules and blur follow in M3 | M1, then as needed |
| D22 | Golden tolerance | ✅ **Decided 2026-10-06, as revised by measurement:** exact for Ink's own renderer (three goldens, `ink-render/tests/goldens`). Against `lntrn-svg`, "within one level" can only hold on average, not per pixel (§5.4): a file's mean must be within 1.25 levels at 64 px and 0.5 at 256 px, and at most 4 % and 1.5 % of its pixels may be over 16 levels out. `rsvg-convert` is a report to read (`third_opinion`), not a test | M1 |
| D23 | MCP registration | ✅ **Decided and done 2026-10-06:** user scope, `alwaysLoad`, as LS3 (`claude mcp get ink` connects), and `mcp__ink` allowed in `~/.claude/settings.json` beside `mcp__studio` | M2 |
| D24 | Claude's edits on screen | Whether paths draw themselves as LS3's strokes do | M5 |
| D25 | Other editors' marks | ✅ **Decided 2026-10-05 (Alva):** Boxy SVG's are stripped and Lantern Ink's put in their place. How, which is mine and open to change: on opening, everything in Boxy's namespace goes and `xmlns:bx` becomes `xmlns:ink` (§3.1); the file changes when it's next saved. Boxy's export list and shape hints have no Ink equivalent yet, so they are dropped, not translated | M1 |
