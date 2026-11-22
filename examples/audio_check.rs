//! Brief, quiet output-device smoke check. No terminal UI or persistent state.

#[allow(dead_code)]
#[path = "../src/adapters/audio.rs"]
mod audio;

use std::thread;
use std::time::Duration;

use audio::{Audio, SoundCue};
use rodio::DeviceTrait;
use rodio::cpal::traits::HostTrait;

fn main() {
    let audio = Audio::new();
    if !audio.available() {
        println!("Audio unavailable; gameplay will remain silent.");
        return;
    }
    let device_name = rodio::cpal::default_host()
        .default_output_device()
        .and_then(|device| device.description().ok())
        .map(|description| description.name().to_owned())
        .unwrap_or_else(|| "unnamed default device".to_owned());
    println!("Audio available: {device_name}. Playing three short cues at 30%.");
    for (cue, wait_ms) in [
        (SoundCue::Start, 320),
        (SoundCue::Lock, 720),
        (SoundCue::Combo(3), 240),
    ] {
        audio.play(cue, 30);
        thread::sleep(Duration::from_millis(wait_ms));
    }
    println!(
        "Audio check finished. Output stream healthy: {}.",
        audio.available()
    );
}
