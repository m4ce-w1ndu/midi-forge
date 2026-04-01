//! MNF source text parser.
//!
//! Converts a raw MNF source string into a [`Song`] data structure ready for
//! encoding.  The public API is a single function, [`parse`]; the helpers
//! [`parse_pitch`], [`parse_duration`], and [`parse_note_event`] are exposed
//! so that they can be unit-tested independently.
//!
//! # Parsing strategy
//!
//! The parser is a single-pass line-oriented scanner.  It strips inline
//! comments, splits each line into whitespace-separated tokens, and dispatches
//! on the first token.  There is no separate lexer or AST phase.

use crate::model::{ChordEvent, EventKind, NoteEvent, Song, Track, TrackEvent};

/// Parses a pitch token (e.g. `"C4"`, `"D#3"`, `"Eb5"`) into a MIDI note number.
///
/// # Format
///
/// ```text
/// <letter>[<accidental>]<octave>
/// ```
///
/// - `letter`     — one of `C D E F G A B` (case-insensitive).
/// - `accidental` — optional `#` (sharp) or `b` (flat).
/// - `octave`     — a signed integer; middle C is octave `4`.
///
/// # MIDI mapping
///
/// ```text
/// midi_note = (octave + 1) * 12 + semitone + accidental_offset
/// ```
///
/// where semitones are `C=0, D=2, E=4, F=5, G=7, A=9, B=11`.
///
/// # Errors
///
/// Returns an error string if:
/// - The input is fewer than two characters.
/// - The octave suffix cannot be parsed as an integer.
/// - The note name (letter + accidental) is unrecognised.
/// - The computed MIDI note number falls outside `[0, 127]`.
pub fn parse_pitch(s: &str) -> Result<u8, String> {
    let s = s.trim();
    if s.len() < 2 {
        return Err(format!("pitch '{}' too short", s));
    }
    let (note_part, octave_str) = s.split_at(s.len() - 1);
    let octave: i32 = octave_str
        .parse()
        .map_err(|_| format!("bad octave in '{}'", s))?;

    let base: i32 = match note_part.to_uppercase().as_str() {
        "C" => 0,
        "C#" | "DB" => 1,
        "D" => 2,
        "D#" | "EB" => 3,
        "E" => 4,
        "F" => 5,
        "F#" | "GB" => 6,
        "G" => 7,
        "G#" | "AB" => 8,
        "A" => 9,
        "A#" | "BB" => 10,
        "B" => 11,
        other => return Err(format!("unknown note name '{}'", other)),
    };

    let midi = (octave + 1) * 12 + base;
    if !(0..=127).contains(&midi) {
        return Err(format!("note '{}' out of MIDI range", s));
    }
    Ok(midi as u8)
}

/// Parses a duration token into a tick count relative to `tpq`.
///
/// # Named symbols
///
/// | Symbol | Meaning   | Ticks (`tpq = 480`) |
/// |--------|-----------|---------------------|
/// | `w`    | Whole     | 1920                |
/// | `h`    | Half      | 960                 |
/// | `q`    | Quarter   | 480                 |
/// | `e`    | Eighth    | 240                 |
/// | `s`    | Sixteenth | 120                 |
///
/// All symbols are case-insensitive.
///
/// # Dotted durations
///
/// Appending `.` to any named symbol produces a *dotted* duration equal to
/// `base + base / 2` (i.e. 1.5× the base value):
///
/// ```text
/// "q."  →  480 + 240 = 720 ticks  (at tpq = 480)
/// "h."  →  960 + 480 = 1440 ticks
/// ```
///
/// # Raw tick count
///
/// If `s` is a plain integer it is used directly as the tick count, regardless
/// of `tpq`.  This form does not support the `.` dotted suffix.
///
/// # Errors
///
/// Returns an error string if the token is neither a recognised symbol nor a
/// parseable unsigned integer.
pub fn parse_duration(s: &str, tpq: u16) -> Result<u32, String> {
    let s = s.trim();
    let (core, dotted) = if s.ends_with('.') {
        (&s[..s.len() - 1], true)
    } else {
        (s, false)
    };

    let tpq = tpq as u32;
    let base: u32 = match core {
        "w" => tpq * 4,
        "h" => tpq * 2,
        "q" => tpq,
        "e" => tpq / 2,
        "s" => tpq / 4,
        other => other
            .parse::<u32>()
            .map_err(|_| format!("unknown duration '{}'", other))?,
    };

    Ok(if dotted { base + base / 2 } else { base })
}

/// Parses a whitespace-split token slice into a [`NoteEvent`].
///
/// # Expected token layout
///
/// | Index | Content                                    | Required |
/// |-------|--------------------------------------------|----------|
/// | `0`   | Pitch name (e.g. `"C4"`) or `"rest"`       | Yes      |
/// | `1`   | Duration symbol or tick count (e.g. `"q"`) | Yes      |
/// | `2`   | Velocity `0–127` (e.g. `"80"`)             | No       |
///
/// When the velocity token is absent it defaults to `100`.
///
/// # Errors
///
/// Returns an error string if:
/// - Fewer than two tokens are provided.
/// - [`parse_duration`] rejects the duration token.
/// - [`parse_pitch`] rejects the pitch token (non-rest events only).
/// - The velocity token cannot be parsed as a `u8`.
pub fn parse_note_event(parts: &[&str], tpq: u16) -> Result<NoteEvent, String> {
    if parts.len() < 2 {
        return Err(format!("too few tokens: {:?}", parts));
    }
    let pitch_str = parts[0];
    let dur_str = parts[1];
    let velocity: u8 = if parts.len() >= 3 {
        parts[2]
            .parse()
            .map_err(|_| format!("bad velocity '{}'", parts[2]))?
    } else {
        100
    };

    let duration_ticks = parse_duration(dur_str, tpq)?;

    if pitch_str.eq_ignore_ascii_case("rest") {
        Ok(NoteEvent {
            kind: EventKind::Rest,
            duration_ticks,
        })
    } else {
        let pitch = parse_pitch(pitch_str)?;
        Ok(NoteEvent {
            kind: EventKind::Note { pitch, velocity },
            duration_ticks,
        })
    }
}

/// Parses an entire MNF source string into a [`Song`].
///
/// This is the top-level entry point for the parsing pipeline.  The function
/// processes the source line by line, stripping inline comments (`#` to end of
/// line) and delegating individual tokens to [`parse_pitch`],
/// [`parse_duration`], and [`parse_note_event`].
///
/// # Parsing rules
///
/// - Blank lines and comment-only lines are silently skipped.
/// - Header directives (`bpm`, `time`, `tpq`) must appear **before** the first
///   `track` keyword.
/// - Each `track … end` block is parsed in order; multiple tracks are allowed.
/// - Chord lines begin with `{` and must be entirely contained on one line in
///   the form `{ note, note, … }`.
///
/// # Errors
///
/// Returns an error string (with a 1-based line number prefix) for any of the
/// following conditions:
///
/// - An unrecognised top-level keyword.
/// - A missing or malformed directive value.
/// - A `track` block that is never closed with `end`.
/// - Any error propagated from [`parse_note_event`].
/// - A channel value outside `[1, 16]`.
/// - A file that contains no `track` blocks at all.
pub fn parse(source: &str) -> Result<Song, String> {
    let mut song = Song::default();
    let mut in_track = false;
    let mut current_track: Option<Track> = None;

    for (lineno, raw_line) in source.lines().enumerate() {
        let lineno = lineno + 1;
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }

        let tokens: Vec<&str> = line.split_whitespace().collect();

        if in_track {
            match tokens[0].to_lowercase().as_str() {
                "end" => {
                    song.tracks.push(current_track.take().unwrap());
                    in_track = false;
                }
                "instrument" => {
                    let v: u8 = tokens
                        .get(1)
                        .ok_or(format!("line {}: missing instrument value", lineno))?
                        .parse()
                        .map_err(|_| format!("line {}: bad instrument", lineno))?;
                    current_track.as_mut().unwrap().instrument = v;
                }
                "channel" => {
                    let v: u8 = tokens
                        .get(1)
                        .ok_or(format!("line {}: missing channel value", lineno))?
                        .parse()
                        .map_err(|_| format!("line {}: bad channel", lineno))?;
                    if !(1..=16).contains(&v) {
                        return Err(format!("line {}: channel must be 1-16", lineno));
                    }
                    current_track.as_mut().unwrap().channel = v;
                }
                _ if line.starts_with('{') => {
                    let inner = line.trim_start_matches('{').trim_end_matches('}');
                    let mut chord_notes = Vec::new();
                    for ns in inner.split(',') {
                        let parts: Vec<&str> = ns.split_whitespace().collect();
                        if parts.is_empty() {
                            continue;
                        }
                        let ne = parse_note_event(&parts, song.tpq)
                            .map_err(|e| format!("line {}: {}", lineno, e))?;
                        chord_notes.push(ne);
                    }
                    if !chord_notes.is_empty() {
                        current_track
                            .as_mut()
                            .unwrap()
                            .events
                            .push(TrackEvent::Chord(ChordEvent { notes: chord_notes }));
                    }
                }
                _ => {
                    let ne = parse_note_event(&tokens, song.tpq)
                        .map_err(|e| format!("line {}: {}", lineno, e))?;
                    current_track
                        .as_mut()
                        .unwrap()
                        .events
                        .push(TrackEvent::Single(ne));
                }
            }
            continue;
        }

        match tokens[0].to_lowercase().as_str() {
            "bpm" => {
                song.bpm = tokens
                    .get(1)
                    .ok_or(format!("line {}: missing bpm value", lineno))?
                    .parse()
                    .map_err(|_| format!("line {}: bad bpm", lineno))?;
            }
            "tpq" => {
                song.tpq = tokens
                    .get(1)
                    .ok_or(format!("line {}: missing tpq value", lineno))?
                    .parse()
                    .map_err(|_| format!("line {}: bad tpq", lineno))?;
            }
            "time" => {
                let ts = tokens
                    .get(1)
                    .ok_or(format!("line {}: missing time signature", lineno))?;
                let parts: Vec<&str> = ts.split('/').collect();
                if parts.len() != 2 {
                    return Err(format!("line {}: time signature must be N/D", lineno));
                }
                song.time_num = parts[0]
                    .parse()
                    .map_err(|_| format!("line {}: bad time numerator", lineno))?;
                song.time_den = parts[1]
                    .parse()
                    .map_err(|_| format!("line {}: bad time denominator", lineno))?;
            }
            "track" => {
                let name = if tokens.len() > 1 {
                    tokens[1..].join(" ")
                } else {
                    "Unnamed".into()
                };
                current_track = Some(Track {
                    name,
                    instrument: 0,
                    channel: 1,
                    events: Vec::new(),
                });
                in_track = true;
            }
            other => return Err(format!("line {}: unexpected token '{}'", lineno, other)),
        }
    }

    if in_track {
        return Err("unterminated track block (missing 'end')".into());
    }
    if song.tracks.is_empty() {
        return Err("no tracks found in file".into());
    }

    Ok(song)
}
