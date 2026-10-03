# TFC Forge Planner

A fully local native desktop app for the **TerraFirmaGreg Modern 0.13.10** modpack
(**TerraFirmaCraft 1.20.1 v3.2.25**) that:

1. Computes the **shortest, source-correct forge action sequence** from a current work value
   to a target while satisfying 1-3 forge rules, keeping every intermediate value inside the
   legal range.
2. **Recognizes** the current value, target and all three rules from a variable-sized
   Minecraft anvil screenshot entirely on your machine, from the raw pixel buffer (no OCR
   service, no upload).
3. Provides an anvil-like native workbench UI, keyboard shortcuts, and a local SQLite recipe
   library with fuzzy search.

The app is written entirely in Rust: a **native `eframe` / `egui` 0.36.2** window (glow
renderer, no webview), the domain/solver, the SQLite store with versioned migrations, and the
recognizer. There is no web server, no Node.js, and no runtime JSON persistence.

---

## The 0..150 correction (read this first)

The upper work limit is sometimes quoted as **146**. That is **not** what the v3.2.25 source
does. The authoritative numbers come from the exact source tag commit:

> `b158c9c968cad7afb99cea2c937b301ff275e2e6`

Relevant files:

| Path (v3.2.25) | What it confirms |
| --- | --- |
| [`src/main/java/net/dries007/tfc/common/capabilities/forge/ForgeStep.java`](https://github.com/TerraFirmaCraft/TerraFirmaCraft/blob/b158c9c968cad7afb99cea2c937b301ff275e2e6/src/main/java/net/dries007/tfc/common/capabilities/forge/ForgeStep.java) | `public static final int LIMIT = 150;` and the eight step deltas |
| [`src/main/java/net/dries007/tfc/common/capabilities/forge/ForgeRule.java`](https://github.com/TerraFirmaCraft/TerraFirmaCraft/blob/b158c9c968cad7afb99cea2c937b301ff275e2e6/src/main/java/net/dries007/tfc/common/capabilities/forge/ForgeRule.java) | The `Order` values and the exact `matches(...)` semantics |
| [`src/main/java/net/dries007/tfc/common/blockentities/AnvilBlockEntity.java`](https://github.com/TerraFirmaCraft/TerraFirmaCraft/blob/b158c9c968cad7afb99cea2c937b301ff275e2e6/src/main/java/net/dries007/tfc/common/blockentities/AnvilBlockEntity.java) | Work is accepted only while `work >= 0 && work <= LIMIT`; the item is destroyed when `work < 0 || work > LIMIT` |
| [`src/main/java/net/dries007/tfc/client/screen/AnvilScreen.java`](https://github.com/TerraFirmaCraft/TerraFirmaCraft/blob/b158c9c968cad7afb99cea2c937b301ff275e2e6/src/main/java/net/dries007/tfc/client/screen/AnvilScreen.java) | The GUI geometry used by the recognizer (rule overlays, work/target markers, button grid) |

So the source-confirmed legal range is **inclusive `0..150`**. This planner never emits an
intermediate value outside it. This specifically prevents the classic broken path such as
`0 -> -6` (a Medium Hit from an empty anvil), which the game would treat as destroying the item.

### Action deltas

| Action | Delta |
| --- | --- |
| Light Hit (`hit_light`) | -3 |
| Medium Hit (`hit_medium`) | -6 |
| Hard Hit (`hit_hard`) | -9 |
| Draw (`draw`) | -15 |
| Punch (`punch`) | +2 |
| Bend (`bend`) | +7 |
| Upset (`upset`) | +13 |
| Shrink (`shrink`) | +16 |

### Rule semantics (verified against `ForgeRule.match`)

Rules combine a generic type (`hit`, `draw`, `punch`, `bend`, `upset`, `shrink`) with an order
(`any`, `last`, `not_last`, `second_last`, `third_last`). Generic `hit` matches Light, Medium and
Hard hits. For the last three actions (most recent first):

| Order | Meaning |
| --- | --- |
| `last` | the type matches the last action |
| `second_last` | matches the second-last action |
| `third_last` | matches the third-last action |
| `not_last` | matches the second-last OR third-last action |
| `any` | matches any of the last three actions |

All 30 combinations are supported. A rule set is validated by **matching**: it is accepted exactly
when every rule can be assigned to a distinct one of the three history positions it allows.

| Order | Allowed position(s) | Capacity in a rule set |
| --- | --- | --- |
| `last` | last | at most one |
| `second_last` | second-last | at most one |
| `third_last` | third-last | at most one |
| `not_last` | second-last or third-last | at most two |
| `any` | last, second-last or third-last | at most three |

So two `not_last` rules are valid (they take second-last and third-last), but three `not_last`
rules are rejected. Mixed sets without a one-to-one rule-to-position assignment are rejected too.
The UI never lets you build an impossible set: changing an order skips candidates that would
make the set unassignable, and any surviving invalid state is shown as an error instead of being
silently accepted.

---

## Features

* Native `eframe` / `egui` window titled **TFC Forge Planner** with a neutral black-to-charcoal,
  flat, editor-like UI. Wide windows use a main workbench plus a right utility column inside a
  shared vertical scroll region; narrow windows stack the sections.
* **Anvil workbench** with 1..=3 rule slots, each with a click-to-cycle action square and an
  order ladder. Add/remove rules without ever creating an impossible distinct order-slot
  assignment.
* The in-game 2x4 **action grid** in exact order: Light Hit, Medium Hit, Punch, Bend / Hard Hit,
  Draw, Upset, Shrink. Moves outside `0..=150` are disabled; clicking simulates by changing the
  current work value.
* A horizontal **0..150 work track** with green current and red target flags, tick labels at
  0/25/.../150, and compact integer `DragValue` controls. Each flag is independently draggable
  only by grabbing the flag itself and is clamped to `0..150`.
* **Solution panel** with no-solution/out-of-range messaging, the compact sequence with a copy
  button (egui clipboard), step/min/max/range metrics, a numeric path strip, and a per-step
  table (step / action / delta / work).
* **Save section** using a category and recipe name. The category field is editable and existing
  categories are offered in a compact dropdown attached to the field. Field tooltips explain that
  a category describes a reusable material/family grouping and that a recipe name may be reused in
  another category. Recipes are always displayed as `Category / Recipe name` (for example
  `Rods / Rod` vs `Steel / Rod`).
* **Library** with a category filter, fuzzy search over the combined `Category / Name` (and the
  compact notation) using `fuzzy-matcher`, stable results, select-to-load of every field, and
  delete.
* **Screenshot recognition** via the `Open image` native chooser (`rfd`), the `Paste image`
  button or `Cmd`/`Ctrl`+`V` clipboard paste (`arboard`), or OS drag/drop. Clipboard paste
  accepts both pixel images and copied image *files* (`text/uri-list` from a file manager such
  as Dolphin, `CF_HDROP`, and macOS file URLs included). Images are decoded with the `image`
  crate, run through the local recognizer, and summarized in two compact rows: `Current` and
  `Target` values (missing shown as `-`), then the recognized `Rules` joined with ` | `
  (empty shown as `-`). Apply replaces only the fields that were recognized (rules only when
  nonempty).
* **Keyboard shortcuts** using egui command semantics so `Cmd` works on macOS and `Ctrl`
  elsewhere (see below).
* **Local SQLite persistence** with automatic migrations on every launch.

### Keyboard shortcuts

| Shortcut | Action |
| --- | --- |
| `Cmd`/`Ctrl` + `F` | Focus the library search box |
| `Cmd`/`Ctrl` + `S` | Save the current recipe |
| `Cmd`/`Ctrl` + `O` | Open a screenshot chooser |
| `Cmd`/`Ctrl` + `V` | Paste an image or copied image file from the clipboard (skipped only when a text editor owns focus; the `Paste image` button is never skipped) |
| `Escape` | Clear the library search when it is focused, or close the help panel |
| `F1` | Toggle the shortcuts help panel |

`Cmd`/`Ctrl`+`V` is deliberately conservative: it only routes to image paste when no text editor
owns focus, so ordinary text editing keeps working; the `Paste image` button performs the same
paste directly and is not gated on focus. On Linux image paste works under X11 (including
XWayland) and on native Wayland compositors that advertise `ext-data-control-v1` or
`wlr-data-control`; a bare Wayland session without either protocol (for example stock GNOME) is
not supported. On those supported native Wayland compositors the clipboard may hold either a
pixel image or copied image *files* (`text/uri-list`, for example copying a file in Dolphin), and
both are recognized. Native Wayland file **drag/drop** is not available with the pinned winit
0.30.13, so on Wayland use `Paste image` (with the image or file copied) or `Open image` instead
of dropping a file. The Current and Target `DragValue` controls also respond to the usual native
keys: click to select, type a value, use the arrow keys, or drag.

---

## Requirements and commands

The build environment (Rust stable, `pkg-config`, SQLite, `sea-orm-cli`, and the Linux native
GUI/X11/Wayland libraries) is provided by [devenv](https://devenv.sh). After entering or
initializing the devenv shell, plain `cargo run` is the normal launch command:

```sh
devenv shell                 # enter the development shell (first run generates .devenv/profile)
cargo run                    # build and launch the native app
```

On Linux, the devenv shell exports `LD_LIBRARY_PATH` pointing at the native GUI libraries that
winit/glutin load at runtime (Wayland, X11, xkbcommon, GL, Vulkan), so plain `cargo run` and
`cargo test` work without exporting a library path by hand. On macOS and Windows no such path is
set and Cargo behaves normally.

The devenv helper scripts remain available:

```sh
devenv shell -- run          # launch the native app (cargo run)
devenv shell -- test         # cargo test --workspace
devenv shell -- check        # cargo check --workspace --all-targets
devenv shell -- clippy       # cargo clippy --workspace --all-targets -- -D warnings
devenv shell -- fmt          # cargo fmt --all
```

You can also start an interactive shell with `devenv shell` and run `run`, `test`, `check`,
`clippy`, or `fmt` directly.

### CI artifacts

The `Build executables` GitHub Actions workflow (`.github/workflows/build.yml`) runs on every
push, pull request, and manual dispatch on native hosted runners. It tests and builds the release
binary for Linux x86_64 and Windows x86_64 and uploads them as workflow artifacts for 14 days:

| Artifact | Executable |
| --- | --- |
| `tfc-forge-linux-x64` | `tfc-forge` |
| `tfc-forge-windows-x64` | `tfc-forge.exe` |

These artifacts are unsigned build outputs, not releases or installers.

### Migrations

Schema and data migrations live in the `migration/` workspace crate and are registered in
timestamp order. **Every launch applies all pending migrations before any query or UI**, because
`RecipeStore::open_default()` calls `Migrator::up` before the recipe list is loaded. A migration
failure is a fatal startup error: the app prints a clear message and exits nonzero rather than
starting without persistence.

Generate a new migration with:

```sh
devenv shell -- migration:new <name>
```

or, inside the shell:

```sh
migration:new <name>
```

---

## Persistence and data location

Saved recipes live in a single SQLite database managed by the app. The path is resolved at
runtime via `dirs::config_dir()`, so it follows each platform's convention:

| Platform | Resulting location |
| --- | --- |
| Linux | `$XDG_CONFIG_HOME/tfc-forge/recipes.sqlite3`, or `~/.config/tfc-forge/recipes.sqlite3` when `XDG_CONFIG_HOME` is unset |
| macOS | `~/Library/Application Support/tfc-forge/recipes.sqlite3` |
| Windows | `%APPDATA%\tfc-forge\recipes.sqlite3` |

The directory is created if missing and the database is migrated on first open. Fresh databases
start empty; a forward migration removes the historical `default:*` rows from older databases
while preserving recipes the user created.

---

## Project layout

```
devenv.nix / devenv.yaml      Rust devenv shell + helper scripts
Cargo.toml                    Crate manifest (native deps pinned to egui/eframe 0.36.2)
src/main.rs                   Native entry point: runtime, migrate, load, run_native
src/app.rs                    Central state, update loop, shortcuts, DB actions
src/theme.rs                  Neutral black-to-charcoal palette and egui visual style
src/ui/mod.rs                 Shared UI helpers + pure state/filter tests
src/ui/anvil.rs               Rule slots, 2x4 action grid, 0..150 work track
src/ui/solution.rs            Solution metrics/path/table and the save section
src/ui/library.rs             Category filter, fuzzy search, select/delete
src/ui/recognizer.rs          Chooser/drop/paste, summary, apply
src/domain.rs                 Types, deltas, rule tables, helpers, validation
src/solver.rs                 BFS shortest-path solver
src/recognition.rs            Local screenshot recognizer
src/db.rs                     SQLite store (SeaORM + SeaQuery), migrations on open
migration/                    Versioned schema/data migrations
tests/*.rs                    Domain, solver, store, schema and recognition tests
assets/anvil-hit-draw-bend-t109.png   Reference screenshot (target 109) for the recognition tests
assets/anvil-punch-bend-draw-t54.png  Reference screenshot (target 54) for the recognition tests
assets/anvil-punch-hit-upset-t60.png  Reference screenshot (target 60) for the recognition tests
assets/anvil-punch-bend-draw-t100.png Reference screenshot (target 100) for the recognition tests
```

The four bundled screenshots under `assets/` are reference fixtures and are only read by the tests.

---

## License note

This project is an independent implementation. It does **not** import code or assets from
`iagocq/tfc-anvil` (AGPL) or any other calculator; the behavior and geometry above were derived
from the TFC source and the reference screenshots. No game textures are redistributed.
