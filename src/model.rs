//! Core data model for MNF scores.
//!
//! This module defines the in-memory representation of every musical concept
//! that MNF can express: individual notes, rests, chords, tracks, and the
//! top-level [`Song`] that ties them together.
//!
//! All types in this module are produced by [`crate::parser`] and consumed by
//! [`crate::encoder`]; nothing here performs I/O or byte manipulation.

/// The musical content of a single note event.
///
/// Distinguishes between an audible note (carrying pitch and velocity) and a
/// silent rest (which still advances the timeline by its duration).
#[derive(Debug, Clone)]
pub enum EventKind {
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
pub struct NoteEvent {
    /// Whether this event produces sound and, if so, at what pitch and velocity.
    pub kind: EventKind,
    /// Length of the event expressed in MIDI ticks.
    ///
    /// The absolute time value depends on the song's *ticks-per-quarter-note*
    /// (`tpq`) setting.  At the default `tpq` of `480`:
    /// - whole note   → `1920` ticks
    /// - half note    → `960` ticks
    /// - quarter note → `480` ticks
    /// - eighth note  → `240` ticks
    /// - sixteenth    → `120` ticks
    pub duration_ticks: u32,
}

/// A set of notes that sound simultaneously.
///
/// All member notes share the same start tick.  The chord advances the track
/// timeline by the **maximum** duration among its members, allowing notes of
/// different lengths within the same chord.
#[derive(Debug, Clone)]
pub struct ChordEvent {
    /// The individual notes that make up the chord.
    ///
    /// Rests are valid members and contribute their duration to the maximum
    /// calculation, though they produce no sound.
    pub notes: Vec<NoteEvent>,
}

/// A timestamped event on a [`Track`], either a single note/rest or a chord.
#[derive(Debug, Clone)]
pub enum TrackEvent {
    /// A lone note or rest occupying one time slot.
    Single(NoteEvent),
    /// Two or more notes that begin at the same tick.
    Chord(ChordEvent),
}

// ─── Track ───────────────────────────────────────────────────────────────────

/// One MIDI track, corresponding to a `track … end` block in the MNF source.
///
/// Compiles to a single `MTrk` chunk in the output MIDI file.
#[derive(Debug)]
pub struct Track {
    /// Human-readable track name, written as a MIDI *Sequence/Track Name*
    /// meta-event (`0xFF 0x03`) at tick 0.
    pub name: String,
    /// General MIDI program number in `[0, 127]` sent as a *Program Change*
    /// event at tick 0.  Defaults to `0` (Acoustic Grand Piano).
    ///
    /// Ignored on channel 10 (percussion) by compliant GM synthesisers.
    pub instrument: u8,
    /// 1-based MIDI channel number in `[1, 16]`.  Defaults to `1`.
    ///
    /// Channel `10` is reserved for percussion in the General MIDI standard.
    pub channel: u8,
    /// Ordered sequence of events for this track.
    pub events: Vec<TrackEvent>,
}

/// The complete in-memory representation of an MNF score.
///
/// Produced by [`crate::parser::parse`] and consumed by
/// [`crate::encoder::encode_midi`].
#[derive(Debug)]
pub struct Song {
    /// Tempo in beats per minute.  Written as a MIDI *Set Tempo* meta-event
    /// (`0xFF 0x51`) in the dedicated tempo track.
    pub bpm: u32,
    /// Numerator of the time signature (beats per measure).
    pub time_num: u8,
    /// Denominator of the time signature as a plain integer (e.g. `4` for
    /// common time).  Written to the MIDI *Time Signature* meta-event
    /// (`0xFF 0x58`) as its `log2` encoding.
    pub time_den: u8,
    /// Ticks per quarter note (MIDI timing resolution).  Written into the MIDI
    /// file header.  Higher values allow finer rhythmic quantisation.
    pub tpq: u16,
    /// All tracks declared in the source file, in declaration order.
    pub tracks: Vec<Track>,
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
