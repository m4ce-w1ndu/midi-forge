# MNF — MIDI Note Format
## Language Specification v1.0

---

## 1. Overview

**MNF** (MIDI Note Format) is a plain-text language for authoring standard MIDI files. It is designed to be human-readable, minimal, and unambiguous. A single `.mnf` file describes one song, which compiles to a Format-1 MIDI file (one tempo track + one track per declared `track` block).

---

## 2. File Structure

An MNF file is a UTF-8 text file with the extension `.mnf`. It is composed of two sections, in order:

1. **Header** — global song settings (BPM, time signature, resolution).
2. **Tracks** — one or more `track … end` blocks containing note events.

Both sections are optional in isolation, but at least one `track` block must be present for a valid file.

---

## 3. Lexical Rules

### 3.1 Comments
A `#` character and everything following it on the same line is a comment and is ignored by the parser.

```
bpm 120   # this is a comment
```

### 3.2 Whitespace
Tokens are separated by one or more ASCII space or tab characters. Leading and trailing whitespace on a line is ignored. Blank lines are ignored.

### 3.3 Case sensitivity
- Directive keywords (`bpm`, `time`, `tpq`, `track`, `end`, `instrument`, `channel`) are **case-insensitive**.
- Pitch names (`C`, `D#`, `Eb`, `rest`, …) are **case-insensitive**.
- Duration symbols (`w`, `h`, `q`, `e`, `s`) are **case-insensitive**.
- Track names are **case-sensitive** strings.

---

## 4. Header Directives

Header directives must appear **before** the first `track` block. All are optional; defaults apply when omitted.

| Directive | Syntax | Default | Description |
|-----------|--------|---------|-------------|
| `bpm` | `bpm <integer>` | `120` | Beats per minute. Range: 1–999. |
| `time` | `time <num>/<den>` | `4/4` | Time signature. Numerator: 1–99. Denominator: power of 2 (1, 2, 4, 8, 16, 32). |
| `tpq` | `tpq <integer>` | `480` | Ticks per quarter note (MIDI resolution). Range: 1–32767. |

### Examples
```
bpm 140
time 3/4
tpq 960
```

---

## 5. Track Blocks

A track block declares one MIDI track. Tracks are rendered in declaration order.

```
track <name>
  [instrument <program>]
  [channel <number>]
  <event> ...
end
```

- `<name>` — one or more whitespace-separated tokens forming the track name. Written to the MIDI track name meta-event.
- `instrument <program>` — General MIDI program number, 0–127. Default: `0` (Acoustic Grand Piano).
- `channel <number>` — MIDI channel, 1–16. Default: `1`.
- `end` — terminates the track block. Required.

### Example
```
track Lead Guitar
  instrument 30
  channel 3
  E4 q 90
  F#4 e 85
end
```

---

## 6. Note Events

Each line inside a track block (that is not a directive or `end`) is a **note event**.

### 6.1 Single Note / Rest

```
<pitch> <duration> [velocity]
```

| Field | Description |
|-------|-------------|
| `pitch` | Note name (see §7) or the literal `rest`. |
| `duration` | Duration symbol or tick count (see §8). |
| `velocity` | Integer 0–127. Optional; defaults to `100`. |

```
C4 q 80
D#5 e. 110
rest h
```

### 6.2 Chord

Multiple simultaneous notes are grouped on a single line between `{` and `}`, separated by commas.

```
{ <pitch> <duration> [velocity], <pitch> <duration> [velocity], ... }
```

- All notes in a chord begin at the same tick.
- The chord advances the timeline by the **maximum** duration among its member notes.
- A rest inside a chord is valid and simply contributes its duration.

```
{ C4 q 90, E4 q 85, G4 q 85 }
{ C4 h, E4 q 70 }   # E4 plays for a quarter; C4 for a half; timeline advances a half
```

---

## 7. Pitch Names

Pitch names follow standard English notation: letter name + optional accidental + octave number.

```
<letter>[<accidental>]<octave>
```

| Component | Values |
|-----------|--------|
| `letter` | `C` `D` `E` `F` `G` `A` `B` |
| `accidental` | `#` (sharp) or `b` (flat). Optional. |
| `octave` | Integer. Middle C is `C4` = MIDI note 60. |

**Enharmonic equivalents** — both spellings are accepted and resolve to the same MIDI pitch:

| Flat | Sharp |
|------|-------|
| `Db` | `C#` |
| `Eb` | `D#` |
| `Gb` | `F#` |
| `Ab` | `G#` |
| `Bb` | `A#` |

**MIDI range:** `C-1` (0) through `G9` (127). Notes outside this range are a parse error.

---

## 8. Durations

### 8.1 Named Symbols

| Symbol | Name | Ticks (at tpq=480) |
|--------|------|---------------------|
| `w` | Whole | 1920 |
| `h` | Half | 960 |
| `q` | Quarter | 480 |
| `e` | Eighth | 240 |
| `s` | Sixteenth | 120 |

### 8.2 Dotted Durations
Append `.` to any named symbol to get a dotted note (1.5× the base duration).

```
q.   # 720 ticks at tpq=480
h.   # 1440 ticks at tpq=480
```

### 8.3 Raw Tick Count
An integer literal is interpreted directly as a tick count, enabling arbitrary durations.

```
C4 960        # half note at tpq=480
G3 240 95     # eighth note with velocity 95
```

---

## 9. General MIDI Instrument Numbers (Selected)

| Range | Family |
|-------|--------|
| 0–7 | Piano |
| 8–15 | Chromatic Percussion |
| 16–23 | Organ |
| 24–31 | Guitar |
| 32–39 | Bass |
| 40–47 | Strings |
| 48–55 | Ensemble |
| 56–63 | Brass |
| 64–71 | Reed |
| 72–79 | Pipe |
| 80–87 | Synth Lead |
| 88–95 | Synth Pad |
| 112–119 | Ethnic |
| 120–127 | Sound Effects |

> Channel 10 (value `10`) is reserved for **percussion** in the General MIDI standard. Instrument number is ignored on channel 10.

---

## 10. MIDI Output Mapping

| MNF concept | MIDI representation |
|-------------|---------------------|
| `bpm` | Tempo meta-event (µs/beat) in track 0 |
| `time` | Time signature meta-event in track 0 |
| `track` | MTrk chunk (Format 1) |
| `instrument` | Program Change event at tick 0 |
| Note on | Note On (0x9n) at event start tick |
| Note off | Note Off (0x8n) at start + duration ticks |
| `rest` | No events; timeline advances |
| Chord | Overlapping Note On/Off pairs |

---

## 11. Error Conditions

The parser **must** reject files with any of the following:

- A header directive appearing inside a `track … end` block.
- A `track` block that is never closed with `end`.
- An unknown top-level keyword.
- A pitch name with an unrecognised letter or an out-of-range MIDI number.
- An unrecognised duration symbol (non-numeric, not `w/h/q/e/s`).
- A velocity value outside 0–127.
- A channel value outside 1–16.
- An instrument value outside 0–127.
- A file with no `track` blocks.

---

## 12. Complete Example

```mnf
# Moonlight sketch — two tracks
bpm 56
time 3/4
tpq 480

track Melody
  instrument 0    # Acoustic Grand Piano
  channel 1
  # Bar 1
  G#4 q 75
  A4  q 70
  B4  q 70
  # Bar 2
  C5  h. 85
  # Bar 3 — chord
  { E5 q 90, G#5 q 90 }
  rest h
end

track Bass
  instrument 42   # Cello
  channel 2
  C3  h  60
  G2  q  55
  F2  h. 65
  C3  h. 70
end
```
