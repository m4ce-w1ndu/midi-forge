//! # midi_forge
//!
//! Compiles `.mnf` (MIDI Note Format) plain-text scores into standard MIDI
//! files (SMF Format 1).
//!
//! ## Usage
//!
//! ```text
//! midi_forge <input.mnf> [output.mid]
//! ```
//!
//! If `output.mid` is omitted the output path is derived from the input path
//! by replacing its extension with `.mid`.
//!
//! ## Pipeline
//!
//! ```text
//! .mnf source
//!   └─► parser::parse()
//!         └─► model::Song
//!               └─► encoder::encode_midi()
//!                     └─► Vec<u8>  →  .mid file
//! ```
//!
//! ## Modules
//!
//! | Module | Responsibility |
//! |--------|---------------|
//! | [`model`] | All MNF data types (`Song`, `Track`, `NoteEvent`, …) |
//! | [`parser`] | MNF text → `Song` |
//! | [`encoder`] | `Song` → raw MIDI bytes |
//!
//! See `MNF_SPEC.md` for the full language specification and `MNF_GRAMMAR.abnf`
//! for the formal ABNF grammar.

mod encoder;
mod model;
mod parser;

use std::env;
use std::fs;
use std::path::Path;

/// Entry point: reads an MNF file, compiles it, and writes the MIDI output.
///
/// # Arguments (command-line)
///
/// ```text
/// midi_forge <input.mnf> [output.mid]
/// ```
///
/// | Position | Description |
/// |----------|-------------|
/// | 1 | Path to the `.mnf` source file (required). |
/// | 2 | Path for the `.mid` output file (optional). Defaults to the input path with its extension replaced by `.mid`. |
///
/// # Process
///
/// 1. Read the source file to a `String`.
/// 2. Call [`parser::parse`] to obtain a [`model::Song`].
/// 3. Call [`encoder::encode_midi`] to obtain raw MIDI bytes.
/// 4. Write the bytes to the output path.
/// 5. Print a summary of parsed tracks to stdout.
///
/// # Exit codes
///
/// | Code | Meaning |
/// |------|---------|
/// | `0` | Success. |
/// | `1` | Missing arguments, I/O error, or parse error. |
fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: midi_forge <input.mnf> [output.mid]");
        eprintln!("  Converts a .mnf text score to a standard MIDI file.");
        std::process::exit(1);
    }

    let input_path = &args[1];
    let output_path = if args.len() >= 3 {
        args[2].clone()
    } else {
        Path::new(input_path)
            .with_extension("mid")
            .to_string_lossy()
            .to_string()
    };

    let source = fs::read_to_string(input_path).unwrap_or_else(|e| {
        eprintln!("Error reading '{}': {}", input_path, e);
        std::process::exit(1);
    });

    let song = parser::parse(&source).unwrap_or_else(|e| {
        eprintln!("Parse error: {}", e);
        std::process::exit(1);
    });

    println!(
        "Parsed: {} track(s), BPM={}, time={}/{}, tpq={}",
        song.tracks.len(),
        song.bpm,
        song.time_num,
        song.time_den,
        song.tpq
    );
    for t in &song.tracks {
        println!(
            "  Track '{}': ch={}, instrument={}, {} event(s)",
            t.name,
            t.channel,
            t.instrument,
            t.events.len()
        );
    }

    let midi_bytes = encoder::encode_midi(&song);

    fs::write(&output_path, &midi_bytes).unwrap_or_else(|e| {
        eprintln!("Error writing '{}': {}", output_path, e);
        std::process::exit(1);
    });

    println!("Written: {} ({} bytes)", output_path, midi_bytes.len());
}
