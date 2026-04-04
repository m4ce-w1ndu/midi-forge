# 🎼 midi_forge

A command-line compiler that converts plain-text music scores written in **MNF** (MIDI Note Format) into standard `.mid` files. No DAW, no GUI, no dependencies — just a readable text file in and a playable MIDI file out.

```
bpm 140
time 4/4

track Piano
  instrument 0
  channel 1
  C4 q 80
  { E4 q 85, G4 q 85 }
  C5 h. 90
end
```

```sh
midi_forge score.mnf          # → score.mid
midi_forge score.mnf out.mid  # explicit output path
```

---

## Contents

- [Features](#features)
- [Installation](#installation)
- [Quick start](#quick-start)
- [MNF format](#mnf-format)
- [Project structure](#project-structure)
- [Building from source](#building-from-source)
- [Contributing](#contributing)
- [License](#license)

---

## Features

- **Human-readable syntax** — pitch names (`C4`, `D#3`, `Eb5`), duration symbols (`w h q e s`), velocity values
- **Chords** — simultaneous notes grouped in `{ }` on a single line
- **Dotted durations** — append `.` to any duration symbol (`q.`, `h.`, …)
- **Multiple tracks** — each `track … end` block maps to one MIDI track
- **Full header control** — BPM, time signature (`3/4`, `6/8`, …), and ticks-per-quarter-note
- **General MIDI instruments** — program numbers 0–127 per track
- **Zero runtime dependencies** — pure Rust standard library, no external crates
- **MIDI Format 1 output** — compatible with any DAW, notation software, or hardware sequencer

---

## Installation

### Pre-built binary

Download the latest release from the [Releases](../../releases) page and place the binary somewhere on your `$PATH`.

### Cargo

```sh
cargo install --path .
```

### Arch Linux (AUR)

> An AUR package is planned for a future release.

---

## Quick start

**1. Write a score** — create `hello.mnf`:

```
# My first MNF score
bpm 120
time 4/4

track Lead
  instrument 0    # Acoustic Grand Piano
  channel 1
  C4 q 80
  E4 q 80
  G4 q 80
  C5 h 90
  rest h
end
```

**2. Compile it:**

```sh
midi_forge hello.mnf
# Parsed: 1 track(s), BPM=120, time=4/4, tpq=480
#   Track 'Lead': ch=1, instrument=0, 5 event(s)
# Written: hello.mid (96 bytes)
```

**3. Play it** with any MIDI-capable application — [VLC](https://www.videolan.org/vlc/), [MuseScore](https://musescore.org/), [timidity](https://timidity.sourceforge.net/), your DAW of choice, or a hardware synth.

Ready-to-run examples are in the [`examples/`](examples/) directory.

---

## MNF format

MNF is a line-oriented text format. Lines beginning with `#` are comments.

### Header

```
bpm   <integer>       # beats per minute          (default: 120)
time  <num>/<den>     # time signature            (default: 4/4)
tpq   <integer>       # ticks per quarter note    (default: 480)
```

### Track block

```
track <name>
  instrument <0-127>  # General MIDI program number  (default: 0)
  channel    <1-16>   # MIDI channel                 (default: 1)
  …events…
end
```

### Note events

```
<pitch> <duration> [velocity]
```

| Field | Values | Example |
|-------|--------|---------|
| `pitch` | Note name + octave, or `rest` | `C4` `D#3` `Eb5` `rest` |
| `duration` | `w` `h` `q` `e` `s` (append `.` for dotted), or raw tick count | `q` `h.` `960` |
| `velocity` | Integer `0–127` | `80` *(default: 100)* |

Middle C is `C4` = MIDI note 60. Enharmonic spellings (`C#` / `Db`, etc.) are both accepted.

### Chords

Wrap simultaneous notes in `{ }`, separated by commas:

```
{ C4 q 90, E4 q 85, G4 q 85 }
```

All notes in a chord start at the same tick. The timeline advances by the **longest** member duration.

### Duration reference

| Symbol | Name | Ticks at `tpq=480` |
|--------|------|---------------------|
| `w` | Whole | 1920 |
| `h` | Half | 960 |
| `q` | Quarter | 480 |
| `e` | Eighth | 240 |
| `s` | Sixteenth | 120 |
| `q.` | Dotted quarter | 720 |
| `h.` | Dotted half | 1440 |

### Complete example

```
# Waltz — 3/4 time, violin melody over piano accompaniment
bpm 160
time 3/4

track Melody
  instrument 40   # violin
  channel 1
  E5 q 85
  D5 q 75
  C5 q 75
  B4 h 90
  G4 q 70
  A4 q. 80
  B4 e  80
  C5 h. 95
end

track Accompaniment
  instrument 0    # piano
  channel 2
  { C3 q 60, E3 q 60, G3 q 60 }
  { C3 q 55, E3 q 55, G3 q 55 }
  { C3 q 55, E3 q 55, G3 q 55 }
end
```

For the full language reference see [`docs/MNF_SPEC.md`](docs/MNF_SPEC.md).  
For the formal grammar see [`docs/MNF_GRAMMAR.abnf`](docs/MNF_GRAMMAR.abnf).

---

## Project structure

```
midi_forge/
├── src/
│   ├── main.rs        # CLI entry point
│   ├── model.rs       # Data types: Song, Track, NoteEvent, …
│   ├── parser.rs      # MNF text → Song
│   └── encoder.rs     # Song → MIDI bytes
│
├── examples/
│   ├── progression.mnf  # Two-track piano + bass progression (4/4)
│   └── waltz.mnf        # Violin melody + piano accompaniment (3/4)
│
├── docs/
│   ├── MNF_SPEC.md      # Full MNF language specification
│   └── MNF_GRAMMAR.abnf # Formal ABNF grammar (RFC 5234)
│
├── Cargo.toml
└── README.md
```

### Module responsibilities

| Module | Responsibility |
|--------|----------------|
| `model` | All MNF data types; no I/O, no parsing |
| `parser` | `parse()` and its helpers; text → `Song` |
| `encoder` | `encode_midi()` and MIDI binary writers; `Song` → bytes |
| `main` | CLI: argument handling, file I/O, wiring |

---

## Building from source

**Requirements:** Rust 1.75+ (edition 2021), no external crates.

```sh
git clone https://github.com/your-org/midi_forge
cd midi_forge

# Debug build
cargo build

# Release build (recommended for use)
cargo build --release

# Run directly
cargo run --release -- examples/waltz.mnf

# Generate documentation
cargo doc --no-deps --open
```

---

## Contributing

Contributions are welcome. A few areas that would benefit from work:

- **Repeats** — `repeat N … end` blocks
- **Tempo changes** — multiple `bpm` events mid-track
- **Ties** — hold a note across bar boundaries
- **More duration subdivisions** — triplets, 32nd notes
- **Unit tests** — especially for `parser` and `encoder`

Please open an issue before starting larger changes so we can discuss the design.

---

## License

MIT — see [`LICENSE`](LICENSE) for details.
