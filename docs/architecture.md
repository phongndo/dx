# Architecture boundaries

This project keeps the terminal UI organized around a few explicit boundaries:

- `DiffApp` is the top-level state aggregate and compatibility shell. New logic should prefer narrower state/controller types instead of adding more coordinator methods directly to `DiffApp`.
- Event routing is component based. Key and mouse components receive focused context traits, not `&mut DiffApp`.
- Rendering is component based. The compositor talks to a render context, not directly to `DiffApp`.
- Rendering has a mutable prepare phase followed by mostly read-only drawing. Leaf menu/sidebar/status/toast renderers take `&DiffApp`; the diff viewport remains the intentionally mutable render path because it warms lazy syntax/context/inline caches while building visible rows.
- App modules should import concrete dependencies directly. Do not introduce app preludes or wildcard app facades.
- Side effects that leave the event loop should be modeled as app effects where practical. App code may queue `AppEffect`s during domain handling; the runner/effect executor performs external work such as editor launch, clipboard writes, reloads, toasts, and settings persistence.
- Keep modules cohesive. If a production module grows past its architecture budget, split it by responsibility before adding more behavior.

Run `scripts/check-architecture` before submitting broad refactors.

## Responsibility map

`DiffApp` remains the composition root. It owns the state graph and wires together
subsystems, but feature logic should live in the subsystem that owns the concept.

| Area | Owner | Notes |
| --- | --- | --- |
| Event ordering | `app/input/layers.rs`, `app/mouse.rs` | Routes components through focused context traits. |
| Key navigation | `app/controllers/navigation.rs` | Owns key-to-navigation behavior; context supplies narrow operations. |
| Filter input routing | `app/controllers/filter.rs` | Owns filter input routing; filter mutation remains with filter state/app methods. |
| Menu key routing | `app/controllers/menu.rs` | Owns open-menu precedence and routing outcomes; menu internals stay in their menu modules. |
| Render composition | `render/compositor.rs` | Generic compositor over `RenderContext`; no `DiffApp` dependency. |
| External effects | `app/effect.rs`, `app/runner.rs` | Domain/event code queues `AppEffect`s; effect execution owns I/O and runner-sensitive pauses. |
| Render planning | `render/mod.rs`, `render/snapshot.rs`, `render/screen_layout.rs` | May inspect/mutate app during preparation, then render through context; render state clamps sidebar/menu scroll before leaf drawing. |
| Diff rows | `render/diff.rs`, `render/diff/*` | Diff viewport row orchestration, split/unified line rendering, context controls, and shared content styling. |
| Diff text highlighting | `render/grep.rs`, `render/grep/*` | Grep match target mapping, span highlighting, and mouse-hover content highlighting. |
| Headers | `render/headers/*` | File headers, hunk headers, delta rendering, and fitting helpers. |
| Statusline | `render/statusline/*` | Header/statusline, filter bar, and error log rendering. |
| Diff loading/jobs | `app/diff_load.rs`, `app/diff_load/*`, `app/editor_reload.rs`, `JobState` | `diff_load.rs` starts/drains foreground loads; `diff_load/cache.rs` owns cache entries/invalidation; `diff_load/prefetch.rs` owns speculative loads; `diff_load/review.rs` owns review-target loads. Prefer explicit effects for new side effects. |
| Syntax highlighting | `app/syntax.rs`, `syntax/*` | Runtime scheduling, queueing, source building, and result application. |

## Adding new behavior

1. Put routing/order decisions in the relevant controller or layer module.
2. Put domain mutation on the state/controller that owns the invariant.
3. Keep `DiffApp` methods as orchestration shims only when several subsystems must
   be coordinated.
4. Prefer immutable render snapshots for drawing. If render must mutate app state,
   do it in the prepare/plan phase, not inside leaf draw helpers.
5. Queue an `AppEffect` for external side effects instead of performing I/O directly from event/domain code.
6. Add a focused unit test with a fake context when adding a new event component or
   controller branch.
