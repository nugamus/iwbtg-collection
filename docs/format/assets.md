# Assets

Part of [format v0](v0.md).

- **Images:** PNG only, straight alpha, at most 4096 px on a side (a sprite wraps its frames into rows with `columns`).
- **Sprites:** `sprites/<id>.png` is a horizontal strip, or a grid of rows when the sidecar sets `columns`. Its sidecar:

  ```toml
  #:schema ../../../client/docs/format/schema/v0/sprite.schema.json
  frames = 4
  frame_size = [64, 64]
  columns = 4                # optional; frames per row of the image, default all frames in one row
  origin = [32, 32]
  collision = "precise"      # precise | rect | disk | diamond | none, as in GM8 sprite settings
  bbox = [0, 0, 63, 63]      # optional; computed from alpha if absent
  alpha_tolerance = 0
  separate_masks = false     # GM "separate collision masks": one mask per frame
  mask = "boss1_mask"        # optional separate mask sprite
  ```

  `columns` wraps the frames into a grid read row by row: frame `i` is at column `i % columns`, row `i / columns`, so the image is `columns * frame_width` by `ceil(frames / columns) * frame_height` px (the last row may be partly empty). It is at least 1 and at most `frames`. A sprite whose strip would pass 4096 px sets it (converter request CR15); both image sides stay at most 4096.

  `disk` and `diamond` are rasterized inside the box at load. `none` is GM's empty mask (an inverted box or a fully transparent sprite). `frames = 0` is a sprite without images: it needs no PNG and has an empty mask.

  `mask` names another sprite whose masks this sprite uses. A chain (`a` -> `b` -> `c`) resolves to the last sprite's own masks, so the result does not depend on the sidecar order; a sprite that names itself, or a chain that returns to a sprite, is a load error.

  Collision masks are built from PNG alpha at load, so masks never drift from the art.

  A sidecar may carry an optional `[collider]` table: the canonical collider baked from the same alpha data. It is a typed form — `none`, `rect` (the whole box) or `bits` (one hex row per pixel row, the most significant bit first, `ceil(width / 4)` digits, and padding bits clear) — so a degenerate mask is an explicit `none`, never an inverted rectangle. The table declares its own required `alpha_tolerance`, which must equal the sidecar's, and holds either the shared `shape`/`bbox`/`bits` keys or one `frames` entry per frame when `separate_masks` builds separate masks. The loader re-derives the mask from the PNG and rejects a collider that differs, so a stale or hand-edited bake cannot change geometry; a package without the table loads exactly as before.

  ```toml
  #:schema ../../../client/docs/format/schema/v0/sprite.schema.json
  frames = 2
  frame_size = [2, 1]
  origin = [0, 0]
  separate_masks = true

  [collider]
  alpha_tolerance = 0
  frames = [
    { shape = "bits", bbox = [0, 0, 0, 0], bits = ["8"] },
    { shape = "none" },
  ]
  ```

  A sprite with the id `cherry` (at least two 21x24 frames) is also the client's pause menu selector, the delicious fruit (docs/product.md, Pause); without one the client draws its own.
- **Backgrounds and tilesets:** `backgrounds/<id>.png`, with a sidecar for tile size and separation when used as a tileset (see Rooms).
- **Audio:** Ogg Vorbis. Sound effects in `sounds/`, music in `music/`. Loop points and preview start in `music/<id>.toml`. FMOD and SuperSound calls in originals become plain sound and music ids. Audio never affects the sim.
- **Fonts:** bitmap PNG plus sidecar.
- **Package size:** at most 512 MB. At most 50,000 instances per room.
- Every file that is not the creator's own work or ours is listed in `[[third_party]]`.
