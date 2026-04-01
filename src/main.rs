use std::env;
use std::fs;
use std::path::Path;

/// The musical content of a single note event.
///
/// Distinguishes between an audible note (carrying pitch and velocity) and a
/// silent rest (which still advances the timeline by its duration).
#[derive(Debug, Clone)]
enum EventKind {
    /// A pitched, audible note.
    Note {
        /// MIDI note number in the range `[0, 127]`.
        /// Middle C (`C4`) is `60`.
        pitch: u8,
        /// MIDI velocity (loudness) in the range `[0, 127]`.
        /// A value of `0` is conventionally treated as *Note Off* by many
        /// synthesisers; prefer `1` for the softest audible note.
        velocity: u8,
    },
    /// A silent pause that advances the track timeline without producing sound.
    Rest,
}

/// A single note or rest together with its duration.
///
/// Used both for stand-alone events ([`TrackEvent::Single`]) and as the
/// constituent notes of a [`ChordEvent`].
#[derive(Debug, Clone)]
struct NoteEvent {
    /// Whether this event produces sound and, if so, at what pitch and velocity.
    kind: EventKind,
    /// Length of the event expressed in MIDI ticks.
    ///
    /// The absolute time value depends on the song's *ticks-per-quarter-note*
    /// (`tpq`) setting.  At the default `tpq` of `480`:
    /// - whole note   → `1920` ticks
    /// - half note    → `960` ticks
    /// - quarter note → `480` ticks
    /// - eighth note  → `240` ticks
    /// - sixteenth    → `120` ticks
    duration_ticks: u32,
}

/// A set of notes that sound simultaneously.
///
/// All member notes share the same start tick.  The chord advances the track
/// timeline by the **maximum** duration among its members, allowing notes of
/// different lengths within the same chord.
#[derive(Debug, Clone)]
struct ChordEvent {
    /// The individual notes that make up the chord.
    ///
    /// Rests are valid members and contribute their duration to the maximum
    /// calculation, though they produce no sound.
    notes: Vec<NoteEvent>,
}

/// A timestamped event on a [`Track`], either a single note/rest or a chord.
#[derive(Debug, Clone)]
enum TrackEvent {
    /// A lone note or rest occupying one time slot.
    Single(NoteEvent),
    /// Two or more notes that begin at the same tick.
    Chord(ChordEvent),
}

/// One MIDI track, corresponding to a `track … end` block in the MNF source.
///
/// Compiles to a single `MTrk` chunk in the output MIDI file.
#[derive(Debug)]
struct Track {
    /// Human-readable track name, written as a MIDI *Sequence/Track Name*
    /// meta-event (`0xFF 0x03`) at tick 0.
    name: String,
    /// General MIDI program number in `[0, 127]` sent as a *Program Change*
    /// event at tick 0.  Defaults to `0` (Acoustic Grand Piano).
    ///
    /// Ignored on channel 10 (percussion) by compliant GM synthesisers.
    instrument: u8,
    /// 1-based MIDI channel number in `[1, 16]`.  Defaults to `1`.
    ///
    /// Channel `10` is reserved for percussion in the General MIDI standard.
    channel: u8,
    /// Ordered sequence of events for this track.
    events: Vec<TrackEvent>,
}

/// The complete in-memory representation of an MNF score.
///
/// Produced by [`parse`] and consumed by [`encode_midi`].
#[derive(Debug)]
struct Song {
    /// Tempo in beats per minute.  Written as a MIDI *Set Tempo* meta-event
    /// (`0xFF 0x51`) in the dedicated tempo track.
    bpm: u32,
    /// Numerator of the time signature (beats per measure).
    time_num: u8,
    /// Denominator of the time signature as a plain integer (e.g. `4` for
    /// common time).  Written to the MIDI *Time Signature* meta-event
    /// (`0xFF 0x58`) as its `log2` encoding.
    time_den: u8,
    /// Ticks per quarter note (MIDI timing resolution).  Written into the MIDI
    /// file header.  Higher values allow finer rhythmic quantisation.
    tpq: u16,
    /// All tracks declared in the source file, in declaration order.
    tracks: Vec<Track>,
}

impl Default for Song {
    /// Returns a `Song` with conventional defaults:
    /// - `bpm`      = `120`
    /// - `time_num` = `4`, `time_den` = `4`  (common time, 4/4)
    /// - `tpq`      = `480`
    /// - `tracks`   = empty
    fn default() -> Self {
        Song {
            bpm: 120,
            time_num: 4,
            time_den: 4,
            tpq: 480,
            tracks: Vec::new(),
        }
    }
}

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
///
/// # Examples
///
/// ```
/// assert_eq!(parse_pitch("C4").unwrap(),  60);  // middle C
/// assert_eq!(parse_pitch("A4").unwrap(),  69);  // concert A
/// assert_eq!(parse_pitch("D#3").unwrap(), 51);
/// assert_eq!(parse_pitch("Eb5").unwrap(), 75);
/// assert!(parse_pitch("X4").is_err());
/// ```
fn parse_pitch(s: &str) -> Result<u8, String> {
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
/// | Symbol | Meaning    | Ticks (`tpq = 480`) |
/// |--------|------------|---------------------|
/// | `w`    | Whole      | 1920                |
/// | `h`    | Half       | 960                 |
/// | `q`    | Quarter    | 480                 |
/// | `e`    | Eighth     | 240                 |
/// | `s`    | Sixteenth  | 120                 |
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
///
/// # Examples
///
/// ```
/// assert_eq!(parse_duration("q",  480).unwrap(), 480);
/// assert_eq!(parse_duration("q.", 480).unwrap(), 720);
/// assert_eq!(parse_duration("w",  480).unwrap(), 1920);
/// assert_eq!(parse_duration("960", 480).unwrap(), 960);
/// assert!(parse_duration("x", 480).is_err());
/// ```
fn parse_duration(s: &str, tpq: u16) -> Result<u32, String> {
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
/// | Index | Content                                     | Required |
/// |-------|---------------------------------------------|----------|
/// | `0`   | Pitch name (e.g. `"C4"`) or `"rest"`        | Yes      |
/// | `1`   | Duration symbol or tick count (e.g. `"q"`)  | Yes      |
/// | `2`   | Velocity `0–127` (e.g. `"80"`)              | No       |
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
///
/// # Examples
///
/// ```
/// let e = parse_note_event(&["C4", "q", "80"], 480).unwrap();
/// // e.kind == EventKind::Note { pitch: 60, velocity: 80 }
/// // e.duration_ticks == 480
///
/// let r = parse_note_event(&["rest", "h"], 480).unwrap();
/// // r.kind == EventKind::Rest
/// // r.duration_ticks == 960
/// ```
fn parse_note_event(parts: &[&str], tpq: u16) -> Result<NoteEvent, String> {
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
///
/// # Examples
///
/// ```
/// let src = "bpm 120\ntrack T\n  C4 q\nend\n";
/// let song = parse(src).unwrap();
/// assert_eq!(song.bpm, 120);
/// assert_eq!(song.tracks.len(), 1);
/// ```
fn parse(source: &str) -> Result<Song, String> {
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
                    let note_strs: Vec<&str> = inner.split(',').collect();
                    let mut chord_notes = Vec::new();
                    for ns in note_strs {
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

/// Appends a 16-bit unsigned integer to `buf` in big-endian byte order.
///
/// MIDI uses big-endian encoding for all fixed-width multi-byte integers.
/// This helper is used for the MIDI file header fields (format, track count,
/// and ticks-per-quarter-note).
///
/// # Examples
///
/// ```
/// let mut buf = Vec::new();
/// write_u16_be(&mut buf, 0x01E0);
/// assert_eq!(buf, &[0x01, 0xE0]);
/// ```
fn write_u16_be(buf: &mut Vec<u8>, v: u16) {
    buf.push((v >> 8) as u8);
    buf.push((v & 0xFF) as u8);
}

/// Appends a 32-bit unsigned integer to `buf` in big-endian byte order.
///
/// Used to write chunk lengths and the tempo value (microseconds per beat)
/// inside MIDI chunk headers and meta-events.
///
/// # Examples
///
/// ```
/// let mut buf = Vec::new();
/// write_u32_be(&mut buf, 0x0007A120);
/// assert_eq!(buf, &[0x00, 0x07, 0xA1, 0x20]);
/// ```
fn write_u32_be(buf: &mut Vec<u8>, v: u32) {
    buf.push(((v >> 24) & 0xFF) as u8);
    buf.push(((v >> 16) & 0xFF) as u8);
    buf.push(((v >> 8) & 0xFF) as u8);
    buf.push((v & 0xFF) as u8);
}

/// Encodes a 32-bit unsigned integer as a MIDI variable-length quantity (VLQ)
/// and appends the result to `buf`.
///
/// MIDI uses VLQ encoding for delta-time values.  Each byte contributes 7 bits
/// of data; the most-significant bit of each byte is set to `1` for all bytes
/// except the last, which has its MSB set to `0`.
///
/// The encoding is big-endian: the most-significant 7-bit group is written
/// first.
///
/// # Value range
///
/// The MIDI spec allows up to four bytes of VLQ, covering values `0x00000000`
/// through `0x0FFFFFFF` (268,435,455).
///
/// # Examples
///
/// ```
/// let mut buf = Vec::new();
/// write_var_len(&mut buf, 0);        // → [0x00]
/// write_var_len(&mut buf, 127);      // → [0x7F]
/// write_var_len(&mut buf, 128);      // → [0x81, 0x00]
/// write_var_len(&mut buf, 0x3FFF);   // → [0xFF, 0x7F]
/// ```
fn write_var_len(buf: &mut Vec<u8>, mut v: u32) {
    let mut bytes = Vec::new();
    bytes.push((v & 0x7F) as u8);
    v >>= 7;
    while v > 0 {
        bytes.push(((v & 0x7F) | 0x80) as u8);
        v >>= 7;
    }
    bytes.reverse();
    buf.extend_from_slice(&bytes);
}

/// Serialises a sequence of absolute-tick MIDI events into an `MTrk` chunk
/// and appends it to `output`.
///
/// Each element of `events` is a `(tick, raw_midi_bytes)` pair where `tick`
/// is the **absolute** tick position of the event.  The function converts
/// these to delta-time values before encoding.
///
/// # Chunk structure written
///
/// ```text
/// "MTrk"              4 bytes  — chunk type
/// <chunk_length>      4 bytes  — big-endian u32, byte count of what follows
/// [delta_time event]* — variable-length quantity + raw event bytes per event
/// 0x00 0xFF 0x2F 0x00 — mandatory End-of-Track meta-event (delta 0)
/// ```
///
/// # Preconditions
///
/// `events` **must** be sorted in non-decreasing tick order before calling
/// this function.  Unsorted input will produce negative delta values, which
/// wrap around when cast to `u32` and corrupt the MIDI file.
///
/// # Parameters
///
/// - `output` — destination buffer; the chunk is appended in place.
/// - `events` — slice of `(absolute_tick, event_bytes)` pairs.
fn write_track_chunk(output: &mut Vec<u8>, events: &[(u32, Vec<u8>)]) {
    output.extend_from_slice(b"MTrk");
    let mut track_data: Vec<u8> = Vec::new();
    let mut last_tick: u32 = 0;
    for (tick, event) in events {
        let delta = tick - last_tick;
        write_var_len(&mut track_data, delta);
        track_data.extend_from_slice(event);
        last_tick = *tick;
    }
    track_data.extend_from_slice(&[0x00, 0xFF, 0x2F, 0x00]); // end-of-track
    write_u32_be(output, track_data.len() as u32);
    output.extend_from_slice(&track_data);
}

/// Encodes a [`Song`] into a complete Standard MIDI File (SMF) byte stream.
///
/// The output conforms to **MIDI Format 1**: a dedicated tempo/meta track
/// followed by one `MTrk` chunk per [`Track`] in the song.
///
/// # Output layout
///
/// ```text
/// MThd chunk
///   format   = 1  (multi-track)
///   n_tracks = song.tracks.len() + 1  (includes the tempo track)
///   tpq      = song.tpq
///
/// MTrk chunk 0  — tempo track
///   Set Tempo  (0xFF 0x51) at tick 0
///   Time Sig   (0xFF 0x58) at tick 0
///
/// MTrk chunk 1..=n  — one per Track
///   Track Name   (0xFF 0x03) at tick 0
///   Program Change at tick 0
///   Note On / Note Off pairs for each event
/// ```
///
/// # Note On / Note Off pairing
///
/// Each [`EventKind::Note`] produces:
/// - A *Note On*  (`0x9n pitch velocity`) at the event's start tick.
/// - A *Note Off* (`0x8n pitch 0`)        at `start + duration_ticks`.
///
/// [`EventKind::Rest`] events advance the timeline silently (no MIDI bytes).
///
/// For [`TrackEvent::Chord`] events all constituent notes share the same start
/// tick; the timeline advances by the maximum member duration.
///
/// # Parameters
///
/// - `song` — the parsed score to encode.
///
/// # Returns
///
/// A `Vec<u8>` containing a valid, self-contained MIDI file that can be
/// written directly to disk.
fn encode_midi(song: &Song) -> Vec<u8> {
    let mut output: Vec<u8> = Vec::new();

    // Header chunk
    output.extend_from_slice(b"MThd");
    write_u32_be(&mut output, 6);
    write_u16_be(&mut output, 1); // format 1
    write_u16_be(&mut output, (song.tracks.len() + 1) as u16);
    write_u16_be(&mut output, song.tpq);

    // Tempo track
    let mut tempo_events: Vec<(u32, Vec<u8>)> = Vec::new();
    let us_per_beat: u32 = 60_000_000 / song.bpm;
    tempo_events.push((
        0,
        vec![
            0xFF,
            0x51,
            0x03,
            ((us_per_beat >> 16) & 0xFF) as u8,
            ((us_per_beat >> 8) & 0xFF) as u8,
            (us_per_beat & 0xFF) as u8,
        ],
    ));
    let den_power: u8 = match song.time_den {
        1 => 0,
        2 => 1,
        4 => 2,
        8 => 3,
        16 => 4,
        32 => 5,
        _ => 2,
    };
    tempo_events.push((0, vec![0xFF, 0x58, 0x04, song.time_num, den_power, 24, 8]));
    write_track_chunk(&mut output, &tempo_events);

    // Note tracks
    for track in &song.tracks {
        let ch = track.channel - 1;
        let mut events: Vec<(u32, Vec<u8>)> = Vec::new();

        // Track name
        let name_bytes = track.name.as_bytes();
        let mut name_event = vec![0xFF, 0x03, name_bytes.len() as u8];
        name_event.extend_from_slice(name_bytes);
        events.push((0, name_event));

        // Program change
        events.push((0, vec![0xC0 | ch, track.instrument]));

        let mut current_tick: u32 = 0;

        for te in &track.events {
            match te {
                TrackEvent::Single(ne) => {
                    if let EventKind::Note { pitch, velocity } = ne.kind {
                        events.push((current_tick, vec![0x90 | ch, pitch, velocity]));
                        events.push((current_tick + ne.duration_ticks, vec![0x80 | ch, pitch, 0]));
                    }
                    current_tick += ne.duration_ticks;
                }
                TrackEvent::Chord(chord) => {
                    let max_dur = chord
                        .notes
                        .iter()
                        .map(|n| n.duration_ticks)
                        .max()
                        .unwrap_or(0);
                    for ne in &chord.notes {
                        if let EventKind::Note { pitch, velocity } = ne.kind {
                            events.push((current_tick, vec![0x90 | ch, pitch, velocity]));
                            events.push((
                                current_tick + ne.duration_ticks,
                                vec![0x80 | ch, pitch, 0],
                            ));
                        }
                    }
                    current_tick += max_dur;
                }
            }
        }

        events.sort_by_key(|(tick, _)| *tick);
        write_track_chunk(&mut output, &events);
    }

    output
}

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
/// | 1        | Path to the `.mnf` source file (required). |
/// | 2        | Path for the `.mid` output file (optional). Defaults to the input path with its extension replaced by `.mid`. |
///
/// # Process
///
/// 1. Read the source file to a `String`.
/// 2. Call [`parse`] to obtain a [`Song`].
/// 3. Call [`encode_midi`] to obtain raw MIDI bytes.
/// 4. Write the bytes to the output path.
/// 5. Print a summary of parsed tracks to stdout.
///
/// # Exit codes
///
/// | Code | Meaning |
/// |------|---------|
/// | `0`  | Success. |
/// | `1`  | Missing arguments, I/O error, or parse error. |
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

    let song = parse(&source).unwrap_or_else(|e| {
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

    let midi_bytes = encode_midi(&song);

    fs::write(&output_path, &midi_bytes).unwrap_or_else(|e| {
        eprintln!("Error writing '{}': {}", output_path, e);
        std::process::exit(1);
    });

    println!("Written: {} ({} bytes)", output_path, midi_bytes.len());
}
