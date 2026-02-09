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
src/
  core/           # Pure, deterministic game logic
    board.rs      # Grid management, line clears
    piece.rs      # Piece types, rotations, shapes
    rng.rs        # 7-bag random number generator
    state.rs      # Game state (board, active, queue, hold)
    rules.rs      # Collision, lock, gravity, clearing
    command.rs    # Input command enum
    events.rs     # Game events (line clear, lock, etc.)
    timing.rs     # Tick/step helpers
    garbage.rs    # Garbage system for multiplayer
    multiplayer.rs # Multiplayer support
  ui/             # Terminal UI layer
    render.rs     # Ratatui rendering
    input.rs      # Key mapping to commands
  app/            # Application layer
    loop.rs       # Main game loop, fixed timestep
    config.rs     # Keybindings, tick rates
    net.rs        # Networking for multiplayer
    storage.rs    # Save/load functionality
  main.rs         # Entry point
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
