# Audio adapter validation

Run date: 2026-11-22. Host: `aarch64-apple-darwin`.
Compiler: `rustc 1.99.0 (b940084d7 2026-09-28)`.
Dependency: `rodio 0.23.0`, with playback/mp3/vorbis/wav features.

Source base: `dd9ca501fd13056fd157d677089f6f765a33b4ae`, with uncommitted changes.
Exact tested source SHA-256:

```text
269e8be479ac3dc87db26bd356bdbfad2c5fa7725ca4be875ff59be987fce852  src/adapters/audio.rs
0f56ea629089a19410b56690c64925d9392832f1172919c3eb6efbcb0b0df302  examples/audio_check.rs
```

## Device-free tests

`cargo test --locked --example audio_check`:

```text
running 8 tests
test audio::tests::invalid_audio_falls_back_without_panicking ... ok
test audio::tests::volume_is_linear_and_clamped_to_a_percentage ... ok
test audio::tests::synthesized_notes_start_and_end_without_a_step ... ok
test audio::tests::bundled_rainforest_assets_decode_as_stereo_mp3 ... ok
test audio::tests::disabled_or_failed_output_does_not_panic ... ok
test audio::tests::every_cue_is_audible_finite_normalized_and_short ... ok
test audio::tests::zero_volume_and_empty_clear_or_combo_are_silent ... ok
test audio::tests::out_of_range_counts_use_last_cached_sound ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.16s
```

## Real-device smoke check

`cargo run --locked --example audio_check`:

```text
Audio available: MacBook Pro Speakers. Playing three short cues at 30%.
Audio check finished. Output stream healthy: true.
```

The example submitted `Start`, `Lock`, and `Combo(3)` to the live output stream,
then retained it for 1.28 seconds in total. No stream errors or shutdown logging
were reported. This proves a device opened and accepted playback, not that a
listener heard it or that terminal gameplay emits the right cues.

## Static checks

- `rustfmt --edition 2024 --check src/adapters/audio.rs examples/audio_check.rs`: passed.
- `cargo clippy --locked --example audio_check -- -D warnings`: passed.
- `git diff --check`: passed.

Source URL, HTTP, hash, format, size, duration, and licensing evidence for both
downloaded samples is recorded in [README.md](README.md).

Not exercised: physically disconnected output device, missing-device OS
environment, listener assessment, or the main runtime's gameplay cue mapping.
The missing-output branch and post-failure disabled state were tested without
opening a device.
