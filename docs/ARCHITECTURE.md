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
            units), Commands and their validation.
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
- **The 500 / 600 line file rule applies throughout.**

**What exists already, and what we do with it** (from the audit):

| Need | Where it is today | Plan |
|---|---|---|
| Flattening, strokes (joins, caps), exact-area rasterizer on all cores | LS3 `studio-core/src/vector` (856 lines, f64; 4096² with 118k edges in 50 ms) | Flattening and strokes ported into `ink-geom`. The rasterizer is Ink's own (§5.1): LS3's counts a pixel twice where a stroke's pieces overlap in it |
| Path `d` parser, colour and transform parsers, gradients, clip paths, drop shadows, group layers, dashes | LUI2 `lntrn-svg` (3.3k lines, f32, all private behind one `render(svg, size)`) | Port the knowledge, in f64 (LUI2 D004). `lntrn-svg` itself stays as it is and becomes our reference (§5.4) |
| XML | `lntrn-svg/src/xml.rs` drops comments, text and formatting | A new lossless one in `ink-doc` (§3.2) |
| Text shaping, glyph outlines | `lntrn-text` (`place_outlines`, U057) | Use as is |
| PNG, JPEG, WebP encoders | `lntrn-image` | Use as is |
| JSON; JSON-RPC lines, both MCP eras, schemas, arguments, replies, the stdio loop | `lntrn-data`; LS3 `studio-tools` (about 1.5k generic lines among 7.2k) | D11: lifted into a new LUI2 crate, `lntrn-mcp` (LUI2 U082). LS3's staged calls and socket pipe stay with it until the live bridge (M5) needs them shared |
| The Studio look: theme, layout, chrome, controls | LS3 `studio-app` (about 1.8k lines) | D12: copy at M4, or a shared crate |

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
  `<text>`'s words, where white space means something).
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
  programs ignore: `ink:locked`, `ink:label`, guides on the root (D19).
  The root declares `xmlns:ink` in a document Ink made, in one it took
  over from another editor (D25), and from the first time an `ink:`
  attribute is needed; a file from elsewhere is otherwise left unstamped.
- **Dirty state is derived:** `modified = version != saved_version`.

### 3.4 Commands

Typed, in document space, naming their nodes, deterministic, validated
all-or-nothing before anything changes. Families:

| | Commands |
|---|---|
| Structure | `Insert`, `Delete`, `Move` (reorder or re-parent, keeping the look), `Duplicate`, `Group`, `Ungroup` |
| Geometry | `SetGeometry` (a shape's own numbers), `SetPath`, `Transform` (any affine on any nodes, D13), `ToPath` |
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

---

## 4. Commands, history, gestures (ink-core)

### 4.1 Core API
The same shape as LS3's (LS3 §4.1): `apply(doc, Command, Actor)`,
`begin / update / commit / cancel` for gestures, `render`, plus queries
(`hit`, `bounds`, `snap_points`). `Applied { changed: Vec<NodeId>,
structure_changed, dirty: Rect, warnings }` tells the GUI what to redraw
and the MCP server what to say.

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
- **Paints:** colours, linear and radial gradients (units, transform,
  spread, `href`), `currentColor`.
- **Groups** that fade, clip or filter as one draw into a layer of their
  own, laid on when done.
- **Clip paths**, nested, in either units.
- **Filters:** `feDropShadow` and `feGaussianBlur` first (135 and 11 of
  the corpus's files), in the colour space the file asks for.
- **Text** through `lntrn-text`'s glyph outlines, filled like any path.
- **Colour:** 8-bit sRGB, composited in sRGB as SVG does.

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
Those three come with M3's operations.

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
  corpus file (D22). They can't to the pixel: `lntrn-svg` samples 16
  heights per pixel row where Ink takes exact areas. Measured, the worst
  file is 0.95 levels apart on average at 64 px and 0.30 at 256 px. Two
  real differences remain, a few pixels each: dashes round a curve (Ink
  measures along the true curve), and a shadow thrown in from past the
  picture's edge (`lntrn-svg` and `rsvg-convert` have nothing there to
  throw; Ink draws a margin so it does).

`rsvg-convert` is on this machine (`/usr/bin/rsvg-convert`). Tests use it
as a second opinion when it's there and skip when it isn't; nothing links
to it.

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
| Documents | `doc_new`\*, `doc_open`\*, `doc_list`\*, `doc_info`\* (the tree, front to back, as a layers panel shows it), `doc_preview`\*, `doc_source`\*, `doc_save`\*, `doc_export`\* (PNG / JPEG / WebP at any size; a tidied SVG in M3), `doc_close`\*, `doc_set` (viewBox, size) |
| Nodes | `node_add`\* (any element, with its attributes as the file writes them), `node_add_svg`\*, `node_set`\* (any attribute; null takes one off), `node_info`, `node_move`\*, `node_duplicate`, `node_delete`\*, `node_group`, `node_ungroup`, `node_transform`, `node_align` |
| Paths | `path_set`, `path_edit` (anchors and handles), `path_op` (boolean ops, outline stroke, simplify, reverse, to path) |
| Paint | `gradient_add`, `gradient_set`, `clip_set`, `filter_set` |
| Text | `text_add`, `text_set`, `text_to_path`, `font_list` |
| Queries | `doc_query` (what's at a point, bounds of nodes) |
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
- **The canvas:** the CPU renderer draws 256 px tiles on the job pool at
  the current zoom, and the window shows them as LUI2 images. While
  zooming, the old tiles stretch until sharp ones land; a frame never
  waits for a tile. Only tiles an edit touches are redrawn. If handing
  tiles through LUI2 proves too slow at 4K, a present pass of Ink's own
  goes in one module of `ink-app`, measured first.
- **Overlays** (selection boxes, handles, anchors, guides, the pixel
  grid) are drawn in screen px over the canvas with LUI2's own lines, so
  they stay the same size at every zoom.
- **Tools, first set:** Pointer (select, move, scale, rotate), Node
  (anchors and handles; drag a segment to bend it), Pen (click points,
  bend after: Alva's May preference, D17), Rectangle, Ellipse, Line,
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
- **Render goldens:** fixed documents to RGBA, exact (pure CPU). Agreement
  with `lntrn-svg` and, when present, `rsvg-convert`, each test declaring
  its tolerance (D22).
- **Replay:** N Commands, save, load, render: must match. Undo
  everything: must match the start.
- **MCP transcripts:** lines in, lines out, in `cargo test`, both eras.
- **UI logic** with `lntrn_ui::testing::Harness`. **Visual checks are
  Alva's.** Nothing in this project ever captures the screen.
- **Determinism:** no wall clock or randomness in a Command; fonts named
  in the file, a missing one reported rather than silently swapped.

---

## 11. Milestones

| M | Deliverable | Done when |
|---|---|---|
| **M0** ✅ | This doc and its decisions | Alva approves it and answers the "before M1" rows of §12: she did, 2026-10-05 |
| **M1** ✅ | The workspace; `ink-geom`, `ink-doc`, `ink-render`, `ink-core` | Every corpus file round-trips byte-identical, renders in agreement with `lntrn-svg`, and survives edit → undo unchanged. Core saves, loads and exports PNG, headless. Built 2026-10-06; the done-test is `ink-core/tests/m1.rs` |
| **M2** ✅ | `ink-tools` + `lantern-ink-mcp`, the \* tools | Registered (with approval). Claude draws an icon headless, previews it, and saves an `.svg` a Lantern app shows 🎉. Built, deployed and registered 2026-10-06 (17 tools); the done-test passed in a fresh Claude Code session the same day (a session's tools are fixed when it starts) |
| **M3** | Operations: every Command in §3.4 as a Command + tool + test, in five slices: **a** structure and transforms, **b** paint, **c** paths, **d** text, **e** tidy (Alva's order, 2026-10-06) | Path editing, transforms, align, gradients, clips, text, boolean ops, tidy export all work over MCP |
| **M4** | `lantern-ink`, the window, in the LS3 look | A scope checklist written with Alva at M4's start (D20), every box ticked or struck by her |
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
| D12 | The Studio look | **Copy** LS3's theme, layout, chrome and controls into `ink-app` (about 1.8k lines); consider a shared crate once we see what the two apps really share. U004 and U042 keep app looks out of LUI2 | M4 |
| D13 | Moving and scaling | ✅ **Decided 2026-10-06, as recommended: bake into the geometry whenever that's exact**; keep a `transform` only where it isn't (a rotated rect stays a `<rect>` with a `rotate`, so its radius stays adjustable). The alternatives were always a `transform` (the numbers stop saying where things are) and always baking (a rotated rect becomes a path) | M3 |
| D14 | Where a style is written | Where that node already has it (`style=""` or the attribute); a new property goes in as a presentation attribute | M3 |
| D15 | Numbers Ink writes | ✅ **Decided 2026-10-06, as recommended:** three decimals, trailing zeros dropped, settable per document | M3 |
| D16 | Coordinates Claude and the GUI speak | ✅ **Decided 2026-10-06, revising my first recommendation:** attributes are as the file writes them (the node's own coordinates, as in any SVG), which is also what `node_add_svg`'s raw markup means; what's reported back is where things show in the document's coordinates (§3.3) | M2 |
| D17 | Pen tool | Click points, bend the segments after (your May preference); no click-drag handles while placing. Still what you want? | M4 |
| D18 | Path anchors' addresses | Stable ids kept beside the path in memory (`A3`), so a selection survives a point being added; not written to the file | M3 |
| D19 | Ink's own attributes | `xmlns:ink="urn:lantern:ink"`; `ink:locked`, `ink:label`, guides on the root. Nothing else until something needs it | M3 |
| D20 | What "done" means for the window | Ink replaces Boxy SVG for Lantern's icons. I'm inferring Boxy from the `bx:` marks in 107 files; the checklist gets written with you at M4's start | M4 |
| D21 | SVG features | §5.2's list for v1; `<use>`, masks, patterns, images and markers when something needs them. M1 draws what `lntrn-svg` does; text, `<style>` rules and blur follow in M3 | M1, then as needed |
| D22 | Golden tolerance | ✅ **Decided 2026-10-06, as revised by measurement:** exact for Ink's own renderer (three goldens, `ink-render/tests/goldens`). Against `lntrn-svg`, "within one level" can only hold on average, not per pixel (§5.4): a file's mean must be within 1.25 levels at 64 px and 0.5 at 256 px, and at most 4 % and 1.5 % of its pixels may be over 16 levels out. `rsvg-convert` is a report to read (`third_opinion`), not a test | M1 |
| D23 | MCP registration | ✅ **Decided and done 2026-10-06:** user scope, `alwaysLoad`, as LS3 (`claude mcp get ink` connects), and `mcp__ink` allowed in `~/.claude/settings.json` beside `mcp__studio` | M2 |
| D24 | Claude's edits on screen | Whether paths draw themselves as LS3's strokes do | M5 |
| D25 | Other editors' marks | ✅ **Decided 2026-10-05 (Alva):** Boxy SVG's are stripped and Lantern Ink's put in their place. How, which is mine and open to change: on opening, everything in Boxy's namespace goes and `xmlns:bx` becomes `xmlns:ink` (§3.1); the file changes when it's next saved. Boxy's export list and shape hints have no Ink equivalent yet, so they are dropped, not translated | M1 |
