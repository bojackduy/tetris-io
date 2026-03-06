# Tetris (Rust + Ratatui)

A terminal-based Tetris game written in Rust using the [Ratatui](https://ratatui.rs/) library.

## Overview

This is a modern Tetris implementation featuring a clean, deterministic core engine with a thin terminal UI layer. The architecture is designed to be extensible, allowing future competitive features like SRS rotation, scoring systems, combos, and multiplayer support.

## Features

- **Classic Tetris gameplay** in the terminal
- **Modern mechanics**: SRS rotation, wall kicks, lock delay, move reset
- **Game modes**: Endless, Blitz (2:00), 40 Lines
- **Deterministic 7-bag RNG** for fair piece distribution
- **Ghost piece** showing landing position
- **Hold queue** and **next piece preview** (5 pieces)
- **Combo and T-Spin detection**
- **Keyboard controls** with DAS/ARR timing

## Controls

| Key | Action |
|-----|--------|
| Left/Right | Move piece |
| Down | Soft drop |
| Space | Hard drop |
| Up | Rotate right |
| Z | Rotate left |
| A | Rotate 180 |
| C | Hold |
| Escape | Pause/Quit |

## Architecture

```
tetris-io/                           # Root crate (Cargo.toml at project root)
└── src/
    ├── domain/                      # Pure deterministic game engine
    │   ├── board.rs                 # Board representation and operations
    │   ├── piece.rs                 # Piece definitions and rotations
    │   ├── rules.rs                 # Game rules, scoring, SRS rotation
    │   ├── state.rs                 # Game state management
    │   ├── garbage.rs               # Garbage handling for multiplayer
    │   ├── rng.rs                   # Deterministic 7-bag RNG
    │   └── timing.rs                # DAS/ARR timing constants
    ├── application/                 # App flow/reducers/session/services
    │   ├── flow.rs                  # Game flow state machine
    │   ├── reducer.rs               # State reducer functions
    │   ├── session.rs               # Session management
    │   └── gameplay.rs              # Gameplay service layer
    ├── adapters/                    # External adapters
    │   ├── render.rs               # Ratatui rendering adapter
    │   ├── input.rs                # Crossterm input handling
    │   ├── net.rs                  # TCP networking adapter
    │   └── storage.rs              # SQLite storage adapter
    ├── app/                         # Runtime orchestration
    │   ├── loop.rs                 # Main game loop
    │   ├── config.rs               # Configuration management
    │   └── storage.rs              # App-level storage glue
    ├── lib.rs                       # Root library exports
    └── main.rs                      # Executable entrypoint
```

## Building

```bash
cargo build --release
```

## Running

```bash
cargo run --release
```

## Dependencies

- Rust 2024 Edition
- ratatui 0.26
- crossterm 0.27
- rand 0.8
- rusqlite 0.31

## Design Philosophy

- **Core-first**: The game engine is pure and deterministic, making it easy to test and extend
- **UI-agnostic core**: The core logic knows nothing about rendering, enabling future frontends
- **Plugin-based systems**: Rotation systems, scoring, and attack models can be swapped without rewriting core logic

## Future Enhancements

- Full SRS rotation with all wall kicks
- Advanced scoring with back-to-back bonuses
- T-Spin detection and Mini T-Spin recognition
- Online multiplayer with rollback netcode
- Replay system with deterministic input logs
