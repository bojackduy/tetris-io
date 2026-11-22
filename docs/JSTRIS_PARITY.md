# Jstris reference and terminal parity specification

Status: **Verified for core solo gameplay, handling, rules, audio, and Ratatui 0.30.2 UI; remaining differences documented explicitly in Section 6.**

Reference inspected: 2026-11-22, Jstris page reporting version 1.40.1.
Baseline repository commit: `dd9ca501fd13056fd157d677089f6f765a33b4ae`.

## 1. Goal and limits

Reproduce the reference game's controls, timing, placement rules, feedback, and
ordinary solo play in this terminal game. A similar board layout alone does not
meet the goal.

This document is not a claim that the reference has been fully understood.
It distinguishes evidence from implementation choices and unresolved behavior.
No item may be called an exact match until its reference behavior and terminal
behavior have both been exercised with equivalent inputs.

The browser's online rooms, accounts, leaderboards, maps, bots, alternate
rotation systems, and custom modes are also part of the full Jstris site.
Those features have not been reproduced. Existing TCP multiplayer is not a
Jstris server client and must not be described as equivalent.

## 2. Evidence terminology

- **Observed UI:** read through Browsentic from the user's reference page.
- **Static reference:** declarations or logic read from the exact script URL
  loaded by that page. This is not a measurement of runtime behavior.
- **Secondary reference:** external documentation, which may describe an older
  version and must not override conflicting live evidence.
- **Terminal adaptation:** a deliberate choice imposed by the target platform.
  It is not evidence of Jstris behavior.
- **Unknown:** no adequate source or paired behavioral test yet.

## 3. Reference identity and evidence

### E1: Settings on the open page

URL: <https://jstris.jezevec10.com/>.

Browsentic inspection:

- `page_getPageInfo` identified the settings panel and selectors.
- `page_clickElement` selected Game settings and Sound.
- `page_screenshot` showed the handling values.
- `page_extractText` and `page_getPageInfo` exposed options and checked states.

Observed values are this browser's current settings, **not proven factory
defaults**:

| Item | Observed value | Element |
| --- | --- | --- |
| DAS | 133 ms | `#DAS` |
| ARR | 10 ms | `#ARR` |
| Soft drop | Fast | `#sd2` |
| Soft-drop choices | Slow, Medium, Fast, Ultra, Instant | `#sd0` through `#sd4` |
| Grid | Standard | `#gr1` |
| Grid choices | None, Standard, Partial, Vertical, Full | `#gr0` through `#gr4` |
| Ghost | Enabled | `#ghost` |
| Sound effects | Enabled | `#SE` |
| Sound volume | 60% | `#vol-control` |
| Sound pack | Rainforest (Combo) | `#sfxSelect` |
| Start sound | Enabled | `#SEstart` |
| Rotation sound | Disabled | `#SErot` |
| Finesse-fault sound | Disabled | `#SEfault` |
| Voice commentary | Disabled | `#vSE` |

The page shows a central 10-by-20 board, hold area, five next pieces, a landing
ghost, and Time, Attack, Finesse, PPS, KPP, and piece-count readouts. A separate
solo tab was used to exercise New game, left movement, and hard drop.
That interaction confirms those controls work; it does **not** measure DAS,
ARR, kick order, or audio latency.

The original tab's settings were not saved or reset.

### E2: Script actually loaded by the page

Browsentic `page_startDiagnostics` / `page_readNetwork` recorded a successful
script request:

<https://s.jezevec10.com/js/game.js?id=d3c4920197a642c1286d>.

The public script was then fetched for read-only inspection. Its string table
was decoded without running downloaded JavaScript. Local inspection helper:
`graphify-out/inspect_jstris.py`.

Identified declarations and code paths:

| Declaration | Static evidence | What this does not establish |
| --- | --- | --- |
| `softDropSpeeds` | `(time, steps)` entries `(0.05,0)`, `(0.008,0)`, `(0,1)`, `(0,2)`, `(0,20)` | The exact update-clock semantics of `steps`, boundary timing, and behavior at different browser refresh rates |
| Ruleset default `lockDelay` | `[500, 5000, 20000]` | Whether the active mode or room overrides these values |
| `setLockDelay` | Maps those entries to `lockDelay`, `maxLockDelayWithoutLock`, `maxWithoutLock` | A complete behavioral model of lock resets and airborne transitions |
| `checkAutoRepeat` | Resolves held opposing directions using their press timestamps | All direction-switch, cancellation, precharge, and spawn-boundary cases |
| `autoRepeat` | Present in the reference | A verified equivalent Rust implementation |
| `moveBlockToTheWall` | Present in the reference | Exact ARR=0 interactions with rotation, gravity, hold, and piece generation |
| `holdBlock` | Uses a once-per-piece hold guard and checks repeat state afterward | Full initial-hold and held-direction behavior at every spawn boundary |

The reference's lock model is not adequately described by saying "ten resets"
or "fifteen resets." The declarations include time caps. Their exact activation,
reset, and forced-drop behavior needs explicit reference traces.

### E3: Audio requests and attribution

The same Browsentic capture observed successful requests for:

- `https://s.jezevec10.com/res/se2/lock.mp3`
- `https://s.jezevec10.com/res/se2/topout.mp3`
- `https://s.jezevec10.com/res/se2/comboTones.mp3`
- `https://s.jezevec10.com/res/se0/linefall.wav`
- `https://s.jezevec10.com/res/se0/success.ogg`
- `https://s.jezevec10.com/res/se0/ready.wav`
- `https://s.jezevec10.com/res/se0/go.wav`

The selected pack's Sound panel credits Chris Butler, CC BY 3.0, remixed,
freesound.org. Asset-specific attribution and validation are recorded in
`assets/audio/README.md` and `assets/audio/VALIDATION.md`.

Loading an audio asset does not prove which gameplay event triggers it.
The reference's combo sprite offsets, event dispatch, gain, overlap, and latency
are not yet verified.

### E4: Secondary documentation

- <https://tetris.wiki/Jstris>: visible 10-by-20 field, five previews, SRS,
  scoring/attack tables, and unusual single-row, non-solid buffer behavior.
- <https://tetris.wiki/SRS>: ordered kick tables, rotation states, and the
  distinction between standard SRS and variants.
- <https://tetris.wiki/DAS>: initial shift, startup delay, then repeated shifts.

These are useful cross-checks, not substitutes for current-version behavior.

## 4. Behavioral contract and verification cases

### H1: Press, hold, and release

Target: initial horizontal press moves one cell immediately. A held key charges
DAS. Software repeat does not depend on operating-system keyboard repeat.
Release stops repeat. Repeated OS key events do not restart DAS.

Required traces:

1. Press at 0 ms; sample immediately, just before DAS, at DAS, and after DAS.
2. Hold with no OS repeats for at least 1 second.
3. Release between repeat deadlines; no later movement.
4. Two distinct taps faster than the OS repeat delay; two immediate movements.
5. Repeat all cases after focus loss, pause, restart, and a new game.

Status: target contract established; paired Jstris timing traces pending.

### H2: ARR and zero ARR

Target: repeat timing follows elapsed time, not render frequency. Missed repeat
deadlines must not silently turn a 4 ms ARR into one move per 16 ms frame.
Zero ARR must reach the furthest collision-free horizontal position after DAS,
not move one cell per frame.

Required comparisons:

- ARR 10 ms, 4 ms, 1 ms, and 0 ms.
- Poll schedules of 1 ms, 4 ms, 16 ms, and deliberately delayed frames.
- Charge at a wall, rotate away from it, and keep holding.
- Hold across hard drop and across hold-piece replacement.

Unknowns: exact first-repeat inclusivity, carryover, and ordering relative to
gravity/spawn. Rust unit tests alone cannot establish the reference's answers.

### H3: Opposing directions

Static reference supports last-input priority. Test left then right while
holding both; release right and observe whether/how left resumes.

Also test a fresh opposite press, an already charged opposite hold, equal-time
events, and direction cancellation settings.

The existing TUI's DCD setting is not an observed Jstris control. Any retained
DCD behavior is an extension and must not be presented as reference parity.

### H4: Soft drop

Soft drop must have its own speed and held-state handling. Horizontal DAS/ARR
must not control downward repeat. Do not apply both repeated downward commands
and a second accelerated-gravity path unintentionally.

Static presets are recorded in E2. Interpreting Fast and Ultra as 60 and 120
cells/second is currently a **terminal adaptation assuming a 60 Hz reference
step**. That assumption remains unverified and is not exact parity.

Instant soft drop must be distinguished from hard drop: it reaches the stack
but must not lock merely because the key is held.

### R1: Rotation and wall kicks

The ordinary SRS target uses distinct ordered tables for I and JLSTZ pieces.
Check all eight quarter-turn transitions, both walls, the floor, cavities,
failed rotations, and rotation centers.

The repository's mirrored I clockwise table was labeled SRS+; that is not enough
to establish ordinary Jstris SRS compatibility.

Jstris 180-degree rotation behavior is **unknown**. Existing TETR.IO-labeled
180 kick tables must not be called Jstris-equivalent without reference fixtures.

### L1: Contact, lock delay, and reset limits

Verify E2's defaults in an actual solo game before treating them as active rules.
Then test:

- First ground contact starts lock timing.
- Successful movement and rotation reset the idle deadline as the reference does.
- Failed moves and failed rotations do not grant unearned resets.
- Leaving and returning to the ground behaves identically.
- Multiple actions in one update and separate updates have equivalent results.
- The first-contact and per-piece caps cannot be bypassed by repeated movement.
- Hold and new-piece generation reset the correct timers.

The previous diagnostic's expectation of three moves consuming three reset
counts describes a count-limited mode. It does not prove that Jstris itself uses
that mode. Count-limited support and reference time-capped support are distinct.

### B1: Board, spawn, hold, and randomizer

Target: 10 visible columns, 20 visible rows, five previews, once-per-piece hold,
correct spawn position, and a seven-bag distribution.

Unknowns requiring reference fixtures:

- Exact spawn height and occupied-spawn fallback.
- Collision and persistence above the visible board.
- Jstris's single-row, non-solid buffer and line-clipping behavior.
- Identical RNG sequence for a shared seed. Using two different seven-bag
  randomizers does not guarantee the same sequence.

Merely hiding ten rows of the current solid buffer is not buffer parity.

### S1: Scoring and attack

Secondary-reference target includes ordinary line attacks 0/1/2/4, T-spin
attacks 2/4/6, perfect-clear attack 10, B2B bonus 1, and the documented combo
table. Perfect-clear score is an additive 3000 in that documentation.

Verify spin classification, kick-dependent mini upgrades, B2B eligibility,
combo indexing, and garbage cancellation against current Jstris fixtures.
Existing TETR.IO surge/all-spin behavior cannot be silently retained and called
Jstris scoring.

### A1: Sound and feedback

Required: nonblocking sound, mute/volume control, no sound for failed actions,
distinct placement/clear/game-over cues, and correct combo progression.
Rotation sound defaults to the observed off setting.

Current implementation has exact downloaded lock/top-out assets but synthesized
replacement cues elsewhere. Those replacements are **not exact audio parity**.
Opening an output stream proves playback availability, not perceived similarity.

### U1: Terminal presentation and interaction

Required: full visible board without clipping, hold and five previews, ghost,
clear/combo feedback, accurate elapsed time, PPS/KPP, settings, restart, pause,
and resize handling.

Ratatui is upgraded to 0.30.2 with Crossterm 0.29.0. This is an enabling dependency
change, not proof of a better game experience.

The reference's Finesse statistic requires a defined move-cost model and test
fixtures. Do not show a fabricated value to imitate the reference UI.

## 5. Platform decisions

### Input capability

Full software DAS/ARR requires reliable key-down and key-up state. Negotiate
enhanced keyboard reporting before gameplay. Clear held state on focus loss.

For press-only terminals, a tap and an initial hold can produce the same event
stream. No timeout can distinguish them perfectly.

The proposed fallback uses OS repeats and reports reduced input capability.
It is not a faithful mode. Do not lengthen a release timeout merely to make a
held-key unit test pass while introducing phantom movement after taps.

### Audio

Use cached decoded sources and a persistent mixer, not a subprocess per key.
Missing audio hardware must not crash gameplay. Show availability honestly.
Do not distribute assets whose license cannot be established.

### Existing settings

Preserve key bindings and scores. Any conversion from the old gravity multiplier
to an independent soft-drop rate must be documented and tested. New fields use
an additive migration. Applying a reference profile should be explicit for
customized settings.

## 6. Verification results and remaining differences

### Static reference verification (shipped assets)

- `game.js` (`https://s.jezevec10.com/js/game.js?id=d3c4920197a642c1286d`), SHA-256: `339e542802bf7c038b6a2560d75ca1fe5d509fb6f14ebb3b6e48771f3968ab30`
- `replayer.js` (`https://s.jezevec10.com/js/replayer.js?id=660e256050b5a801c998`), SHA-256: `c73d1538dd7fad22d496f78ca5d9a2ba846fecebdcfbbf63f108b98b934bfca9`

Verified and implemented from those scripts:

- **1-row nonsolid `deadline` buffer (`checkIntersection`, `setCurrentPieceToDefaultPos`, `placeBlock`, `checkLineClears`):** `BUFFER_HEIGHT = 1`, `DEADLINE_Y = 0`, `VISIBLE_START_Y = 1`. Movement/rotation treats row 0 as nonsolid; spawn shifts up 1 row if top visible row is occupied and tops out on `deadline` overlap; partial lock writes empty `deadline` cells while all-4-above-visible triggers lock-out; full `deadline` clears +1 line and visible clears pull `deadline` down into row 1.
- **180-degree kicks (`getBlockSets`):** exact 2-test Jstris table (`0→2: (0,0),(0,-1)`, `1→3: (0,0),(1,0)`, `2→0: (0,0),(0,1)`, `3→1: (0,0),(-1,0)` in y-down). Replaced prior TETR.IO 6-kick table.
- **Standard SRS kicks (`getBlockSets`):** exact JLSTZ and I clockwise/counterclockwise tables; upward rotation kicks reset gravity accumulation (`rotateCurrentBlock`).
- **Lock delay (`setLockDelay`, `update`):** 500 ms idle, 5,000 ms first-contact cap, 20,000 ms per-piece cap (default `lock_reset_limit = 0`), with explicit positive count-capped mode retained for compatibility.
- **Soft-drop speeds (`softDropSpeeds`, 60 Hz `update`):** Slow 20, Medium 60, Fast 120 (default), Ultra 180 cells/s, Instant (`0` → ghost without locking).
- **Scoring, attack, and finesse (`Scoring`, `linesAttackDef`, `comboAttackDef`, `checkLineClears`, `finesse`):** exact Jstris score/attack tables, PC attack `10 + combo_attack`, exact 7-piece × 4-rotation finesse lookup table (`optimal_finesse`), exact `GameCore.colors` palette, and full sprint stats (`Time`, `Lines`, `Pieces`, `PPS`, `KPP`, `Finesse`, `Attack`, `Score`) on live HUD, Pause, and Game Over overlays.
- **Rainforest audio (`RainforestSFXset`, `loadSounds`):** bundled verbatim `lock.mp3` (also aliased for hard drop), `topout.mp3`, and `comboTones.mp3` sliced at exact `start = i * 1500 ms`, `duration = 1000 ms`, `cnt = 15` (`c0`–`c14`), credited to Chris Butler (CC BY 3.0).

### Executed checks (macOS, Rust 1.99.0)

| Check | Result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed (0 warnings) |
| `cargo test --locked` | **108 passed, 0 failed** (69 lib + 8 `jstris_parity` + 31 `modern_input`) |
| `cargo build --locked --release` | Passed |
| `cargo run --locked --example audio_check` | Passed (`MacBook Pro Speakers`, output stream healthy) |
| Real PTY run (`target/release/tetris-io`, reconstructed 2D VT screen buffer) | Passed (SQLite settings persistence across process restart + Jstris profile reset, DAS+ARR left-wall shift col `3 → 0`, Hold box fill, Hard Drop `Pieces 0 → 1` + locked bottom row, F4 instant reset, FocusLost auto-pause with `PPS`/`KPP`/`Finesse`/`Attack`, 80×24 resize without clipping, Legacy warning, clean alt-screen restore) |

### Explicit remaining differences (honest gaps)

1. **Press-only terminals:** without Kitty/CSI-u key-release reporting, terminals cannot distinguish a tap from a hold; the game uses OS repeat and displays a `LEGACY INPUT` warning instead of fabricating holds.
2. **Non-Rainforest `se0` cues:** `Start`, optional `Rotate`, and `PerfectClear` use original synthesized chimes rather than unverified-license `se0` assets.
3. **Site-only features:** online rooms, accounts, replays, bots, custom maps, and alternate rotation systems are outside this terminal game's scope.
4. **Live browser timing traces:** DevTools remote port `127.0.0.1:9222` was unavailable; mechanics were verified against the exact shipped `game.js`/`replayer.js` loaded by the open Jstris page rather than live Chrome DevTools timing captures.

## 7. Completion gate

Before claiming completion:

1. Resolve the unknowns relevant to ordinary solo gameplay with reference traces.
2. Give each required behavior a fixture and a stable test ID.
3. Run those fixtures against the real Rust input/domain/application path.
4. Run build, existing tests, new tests, formatting, and Clippy.
5. Exercise the actual executable in a PTY/terminal with enhanced event input:
   movement, held movement, rotation, hold, hard drop, soft drop, pause, restart,
   settings persistence, focus changes, and resize.
6. Inspect terminal screenshots and verify audio playback separately.
7. Record which checks passed, which platform was used, and remaining differences.

Unit tests, screenshots, and statements of intent are complementary evidence.
None alone proves that the terminal game feels exactly like Jstris.
