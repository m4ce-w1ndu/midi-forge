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

/// Prints the full help message to stdout and returns.
fn print_help() {
    println!(
        "midi-forge {ver}
Compile a MIDI Note Format (.mnf) text score into a standard MIDI file.

USAGE
  midi_forge <input.mnf> [output.mid]
  midi_forge -h | --help

ARGUMENTS
  <input.mnf>     Path to the .mnf source file (required).
  [output.mid]    Path for the output MIDI file (optional).
                  Defaults to <input> with its extension replaced by .mid.

OPTIONS
  -h, --help      Show this help message and exit.

MNF FORMAT OVERVIEW
  An .mnf file has two sections: a header (global settings) followed by
  one or more track blocks.  Comments start with # and run to end-of-line.
  Keywords are case-insensitive; track names are case-sensitive.

  HEADER DIRECTIVES  (must appear before the first track)
    bpm <integer>         Tempo in beats per minute.  Default: 120
    time <num>/<den>      Time signature.  Denominator must be a power of 2.
                          Default: 4/4
    tpq <integer>         Ticks per quarter note (1–32767).  Default: 480

  TRACK BLOCK
    track <name>
      instrument <0-127>  GM program number.  Default: 0
      channel <1-16>      MIDI channel.       Default: 1
      <events...>
    end

  NOTE EVENTS
    <pitch> <duration> [velocity]

    Pitch:    <letter>[<accidental>]<octave>
              Letters  : A B C D E F G  (case-insensitive)
              Accidentals: # (sharp)  b (flat, lowercase only)
              Octave  : -1 to 9   (middle C = C4 = MIDI 60)
              Examples: C4  D#3  Eb5  G-1  F#9

    Duration: named symbol or raw tick count
              w   whole        (1920 ticks at tpq=480)
              h   half         ( 960 ticks)
              q   quarter      ( 480 ticks)
              e   eighth       ( 240 ticks)
              s   sixteenth    ( 120 ticks)
              Append . for a dotted value (e.g. q. = 720 ticks).
              A plain integer is used as an exact tick count.

    Velocity: integer 0–127.  Default: 100
    Rest:     use the keyword 'rest' in place of a pitch.

  CHORDS
    {{ C4 q 90, E4 q 85, G4 q 85 }}
    All notes share the same start tick; the timeline advances by the
    longest member duration.  Rests are valid chord members.

EXAMPLES
  midi_forge song.mnf              # writes song.mid
  midi_forge song.mnf out.mid      # explicit output path

  # minimal .mnf file:
  bpm 120
  track Piano
    instrument 0
    channel 1
    C4 q
    E4 q
    G4 q
  end

  See the examples/ directory for complete scores (progression.mnf, waltz.mnf).

FULL SPECIFICATION
  docs/MNF_spec.md    Language specification
  docs/MNF_grammar.abnf  Formal ABNF grammar",
        ver = env!("CARGO_PKG_VERSION")
    );
}

/// Entry point: reads an MNF file, compiles it, and writes the MIDI output.
///
/// # Arguments (command-line)
///
/// ```text
/// midi_forge <input.mnf> [output.mid]
/// midi_forge -h | --help
/// ```
///
/// | Position | Description |
/// |----------|-------------|
/// | 1 | Path to the `.mnf` source file (required), or a help flag. |
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
/// | `0` | Success or `--help`. |
/// | `1` | Missing arguments, I/O error, or parse error. |
fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() >= 2 && (args[1] == "-h" || args[1] == "--help") {
        print_help();
        return;
    }

    if args.len() < 2 {
        eprintln!("Usage: midi_forge <input.mnf> [output.mid]");
        eprintln!("  Converts a .mnf text score to a standard MIDI file.");
        eprintln!("  Run 'midi_forge --help' for full documentation.");
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
