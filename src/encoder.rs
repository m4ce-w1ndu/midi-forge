//! MIDI binary encoder.
//!
//! Converts a [`Song`] data structure into a raw Standard MIDI File (SMF)
//! byte stream conforming to **MIDI Format 1**.
//!
//! The public API is a single function, [`encode_midi`].  The helpers
//! [`write_u16_be`], [`write_u32_be`], [`write_var_len`], and
//! [`write_track_chunk`] are the low-level building blocks and are exposed for
//! testability.
//!
//! # Output structure
//!
//! ```text
//! MThd  (header chunk)
//! MTrk  (track 0: tempo + time signature meta-events)
//! MTrk  (track 1: first note track)
//! MTrk  (track 2: second note track)
//! …
//! ```

use crate::model::{EventKind, Song, TrackEvent};

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
pub fn write_u16_be(buf: &mut Vec<u8>, v: u16) {
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
pub fn write_u32_be(buf: &mut Vec<u8>, v: u32) {
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
/// write_var_len(&mut buf, 0);       // → [0x00]
/// write_var_len(&mut buf, 127);     // → [0x7F]
/// write_var_len(&mut buf, 128);     // → [0x81, 0x00]
/// write_var_len(&mut buf, 0x3FFF);  // → [0xFF, 0x7F]
/// ```
pub fn write_var_len(buf: &mut Vec<u8>, mut v: u32) {
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
/// [delta_time event]* — VLQ delta + raw event bytes, one pair per event
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
pub fn write_track_chunk(output: &mut Vec<u8>, events: &[(u32, Vec<u8>)]) {
    output.extend_from_slice(b"MTrk");
    let mut track_data: Vec<u8> = Vec::new();
    let mut last_tick: u32 = 0;
    for (tick, event) in events {
        let delta = tick - last_tick;
        write_var_len(&mut track_data, delta);
        track_data.extend_from_slice(event);
        last_tick = *tick;
    }
    // Mandatory End-of-Track meta-event
    track_data.extend_from_slice(&[0x00, 0xFF, 0x2F, 0x00]);
    write_u32_be(output, track_data.len() as u32);
    output.extend_from_slice(&track_data);
}

/// Encodes a [`Song`] into a complete Standard MIDI File (SMF) byte stream.
///
/// The output conforms to **MIDI Format 1**: a dedicated tempo/meta track
/// followed by one `MTrk` chunk per [`crate::model::Track`] in the song.
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
///   Track Name    (0xFF 0x03) at tick 0
///   Program Change (0xCn)    at tick 0
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
/// # Returns
///
/// A `Vec<u8>` containing a valid, self-contained MIDI file that can be
/// written directly to disk.
pub fn encode_midi(song: &Song) -> Vec<u8> {
    let mut output: Vec<u8> = Vec::new();

    output.extend_from_slice(b"MThd");
    write_u32_be(&mut output, 6); // header length is always 6
    write_u16_be(&mut output, 1); // format 1: multi-track
    write_u16_be(&mut output, (song.tracks.len() + 1) as u16); // +1 for tempo track
    write_u16_be(&mut output, song.tpq);

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

    for track in &song.tracks {
        let ch = track.channel - 1; // convert 1-based channel to 0-based nibble
        let mut events: Vec<(u32, Vec<u8>)> = Vec::new();

        // Track name meta-event (0xFF 0x03)
        let name_bytes = track.name.as_bytes();
        let mut name_event = vec![0xFF, 0x03, name_bytes.len() as u8];
        name_event.extend_from_slice(name_bytes);
        events.push((0, name_event));

        // Program change: select GM instrument
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

        // Sort by tick before handing off (Note Off events can interleave with
        // next-note Note On events at the same tick boundary)
        events.sort_by_key(|(tick, _)| *tick);
        write_track_chunk(&mut output, &events);
    }

    output
}
