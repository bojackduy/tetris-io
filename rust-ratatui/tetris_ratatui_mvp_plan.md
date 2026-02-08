# Tetris Rust + ratatui MVP Notes

## Intent / Scope
- Build a **simple single-player Tetris MVP** in Rust with a terminal UI using `ratatui`.
- Keep the core **deterministic and UI-agnostic** so advanced competitive systems can be added later.
- Defer advanced systems (SRS, scoring, attack, combo, garbage, etc.) until after MVP.

## Mental Model
- **Core engine** is a pure, deterministic state machine.
- **UI** is a thin layer that translates input to commands and renders state.
- Future features should be **plug-in modules** (rotation system, scoring, attack model, etc.) without rewriting UI or core loop.

## UI (Terminal) Layout
- **Left column**: HOLD at top, COMBO space in the middle for future combo names, SCORE panel bottom-aligned with board.
- **Middle**: BOARD (10x20).
- **Right**: NEXT panel showing 5 upcoming pieces.

ASCII mockup (current target):

```text
┌───────────┐  ┌───────────────────────┐  ┌───────────────┐
│   HOLD    │  │       BOARD 10x20     │  │     NEXT      │
└───────────┘  │                       │  └───────────────┘
┌───────────┐  │        ▓▓             │  ┌───────────────┐
│  COMBO    │  │        ▓▓             │  │ 1) ▓▓▓        │
│  (empty)  │  │                       │  │      ▓       │
│           │  │     ▓▓▓▓              │  └───────────────┘
│           │  │                       │  ┌───────────────┐
│           │  │   ▓▓                  │  │ 2) ▓▓▓▓       │
│           │  │   ▓▓                  │  └───────────────┘
│           │  │                       │  ┌───────────────┐
│           │  │        ▓▓▓            │  │ 3) ▓▓         │
│           │  │                       │  │    ▓▓        │
│           │  │  ▓▓▓▓▓▓               │  └───────────────┘
│           │  │                       │  ┌───────────────┐
│           │  │            ▓▓▓▓       │  │ 4) ▓▓▓        │
│           │  │                       │  │   ▓         │
└───────────┘  │          ▓▓           │  └───────────────┘
┌───────────┐  │          ▓▓           │  ┌───────────────┐
│  Score    │  │                       │  │ 5) ▓▓▓▓       │
│  000000   │  └───────────────────────┘  └───────────────┘
│  Level 01 │
│  Lines 00 │
│  Time 0:0 │
└───────────┘
```

## Architecture (MVP but scalable)
Single crate with clean module boundaries (core is pure, UI is thin):

```
src/
  core/
    mod.rs
    board.rs        // grid, line clears
    piece.rs        // piece types, rotations
    rng.rs          // 7-bag or stub, deterministic
    state.rs        // game state (board, active, queue, hold)
    rules.rs        // collision, lock, gravity, clear
    command.rs      // input commands enum
    events.rs       // emitted events (line clear, lock)
    timing.rs       // tick/step helpers
  ui/
    mod.rs
    render.rs       // ratatui rendering
    input.rs        // key mapping -> Command
  app/
    mod.rs
    loop.rs         // main loop, fixed timestep
    config.rs       // keybinds, tick rates
  main.rs
```

Key design choices:
- `core::state::GameState` is the **single source of truth**.
- `core::rules::step(state, dt, cmds) -> Events` is **pure and deterministic**.
- `core::command::Command` is the **only input surface** for the core.
- Use traits for future systems:
  - `RotationSystem` (start with naive, swap to SRS later)
  - `Scoring` (start with no-op)
  - `AttackModel` (start with no-op)
  - `PieceRng` (start with 7-bag)

## Current Goal (MVP)
- Single-player Tetris with:
  - movement, rotation (SRS+ kicks), soft drop, hard drop
  - gravity tick and lock
  - line clears
  - deterministic 7-bag queue
  - ghost piece (landing projection)
  - render in terminal using ratatui

## Future Goal (Advanced Competitive Systems)
- SRS rotation + wall kicks
- Scoring rules
- Combos, T-spin, back-to-back
- Garbage + attack + cancel
- Replays, deterministic input logs
- Netcode (low-latency, possibly rollback)

## MVP Plan
1. **Skeleton**: Create crate, wire `ratatui` + `crossterm`, render static board.
2. **Core types**: Implement `PieceType`, `Rotation`, `Position`, `Board`, `GameState`.
3. **Rules + collision**: Move, rotate, soft drop, gravity tick, lock.
4. **Line clears**: Detect full lines, clear + shift, emit events.
5. **Spawn & queue**: 7-bag queue, spawn next piece, detect game over.
6. **UI integration**: Key mapping -> `Command`, draw board + active piece.
7. **Basic tests**: Collision, line clear, spawn, bag distribution.

## MVP Keybindings
- `Left`: move left
- `Right`: move right
- `Down`: soft drop
- `Space`: hard drop
- `Z`: rotate left
- `Up`: rotate right
- `A`: rotate 180
- `C`: hold

## Input Timing (MVP)
- DAS: `167` ms
- ARR: `33` ms
- DCD: `33` ms

## Soft Drop (MVP)
- Soft drop speed: `6x` current gravity

## MVP Mechanics (Modern Tetris Feel)
- Lock delay: default `700ms`. Timer starts when a piece first touches the ground.
- Move reset: any successful move or rotation while grounded resets the lock timer (up to a cap).
- Leave-ground reset: if a grounded piece lifts off, lock timer resets (counts against cap).
- Reset cap (conservative): `10` resets max before the timer no longer resets.
- Hard drop: locks immediately.
- Rotation system: SRS wall kicks for 90-degree rotations (JLSTZ standard SRS; I uses SRS+ symmetry: CW kicks mirror CCW).
- 180 rotation: MVP uses a dedicated 180-kick table (TETR.IO-style, 6 tests). Replace with exact TETR.IO table if ours differs.

## MVP Gaps / Follow-ups (Rotation Focus)
- Verify piece spawn orientations + rotation centers match SRS assumptions (current shapes are 4x4, but need confirm pivot alignment).
- Add rotation regression tests for SRS kick cases (wall kick, floor kick, TST/DT scenarios).
- Confirm 180-kick table ordering/offsets against authoritative TETR.IO reference.
