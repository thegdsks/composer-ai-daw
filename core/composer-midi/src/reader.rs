//! Low level SMF parsing. Every length read from the file is checked against
//! the bytes that are really there before anything is allocated or sliced.
use crate::error::MidiError;
use std::collections::{BTreeMap, VecDeque};

pub(crate) struct RawNote {
    pub start: u64,
    pub end: u64,
    pub pitch: u8,
    pub velocity: u8,
}

#[derive(Default)]
pub(crate) struct RawTrack {
    pub name: Option<Vec<u8>>,
    pub tempos: Vec<(u64, u32)>,
    pub meters: Vec<(u64, u8, u8)>,
    /// Closed notes per channel.
    pub notes: BTreeMap<u8, Vec<RawNote>>,
    pub unmatched_offs: usize,
    pub unterminated: usize,
}

pub(crate) struct Smf {
    pub format: u16,
    pub ppq: u16,
    pub tracks: Vec<RawTrack>,
}

pub(crate) struct Limits {
    pub max_tracks: usize,
    pub max_notes: usize,
}

struct Cursor<'a> {
    buf: &'a [u8],
    pos: usize,
    /// Offset of `buf[0]` in the whole file, for error positions.
    base: usize,
}

impl<'a> Cursor<'a> {
    fn left(&self) -> usize {
        self.buf.len() - self.pos
    }
    fn here(&self) -> usize {
        self.base + self.pos
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], MidiError> {
        if n > self.left() {
            return Err(MidiError::Truncated {
                offset: self.base + self.buf.len(),
            });
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, MidiError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, MidiError> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> Result<u32, MidiError> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn varlen(&mut self) -> Result<u32, MidiError> {
        let start = self.here();
        let mut v = 0u32;
        for _ in 0..4 {
            let b = self.u8()?;
            v = (v << 7) | u32::from(b & 0x7f);
            if b & 0x80 == 0 {
                return Ok(v);
            }
        }
        Err(MidiError::BadVarLen { offset: start })
    }
}

pub(crate) fn parse(bytes: &[u8], limits: &Limits) -> Result<Smf, MidiError> {
    let mut c = Cursor {
        buf: bytes,
        pos: 0,
        base: 0,
    };
    if c.take(4).map_err(|_| MidiError::BadHeader("too short"))? != b"MThd" {
        return Err(MidiError::BadHeader("missing MThd"));
    }
    let hlen = c.u32()? as usize;
    if hlen < 6 {
        return Err(MidiError::BadHeader("header shorter than 6 bytes"));
    }
    let hbase = c.here();
    let mut h = Cursor {
        buf: c.take(hlen)?,
        pos: 0,
        base: hbase,
    };
    let format = h.u16()?;
    let ntrks = h.u16()?;
    let division = h.u16()?;
    match format {
        0 | 1 => {}
        2 => return Err(MidiError::UnsupportedFormat(2)),
        _ => return Err(MidiError::BadHeader("unknown format")),
    }
    if division & 0x8000 != 0 {
        return Err(MidiError::UnsupportedDivision);
    }
    if division == 0 {
        return Err(MidiError::BadHeader("zero division"));
    }
    if ntrks == 0 {
        return Err(MidiError::NoTracks);
    }
    if usize::from(ntrks) > limits.max_tracks {
        return Err(MidiError::TooManyTracks);
    }
    let mut tracks = Vec::new();
    let mut notes_left = limits.max_notes;
    while tracks.len() < usize::from(ntrks) {
        let id = c.take(4)?;
        let len = c.u32()? as usize;
        let base = c.here();
        let body = c.take(len)?;
        if id == b"MTrk" {
            tracks.push(parse_track(body, base, &mut notes_left)?);
        }
    }
    Ok(Smf {
        format,
        ppq: division,
        tracks,
    })
}

fn parse_track(body: &[u8], base: usize, notes_left: &mut usize) -> Result<RawTrack, MidiError> {
    let mut c = Cursor {
        buf: body,
        pos: 0,
        base,
    };
    let mut out = RawTrack::default();
    let mut open: BTreeMap<(u8, u8), VecDeque<(u64, u8)>> = BTreeMap::new();
    let mut tick = 0u64;
    let mut running: Option<u8> = None;
    while c.left() > 0 {
        tick += u64::from(c.varlen()?);
        let first = c.u8()?;
        let status = if first & 0x80 != 0 {
            first
        } else {
            c.pos -= 1;
            running.ok_or(MidiError::MissingStatus { offset: c.here() })?
        };
        match status {
            0x80..=0xEF => {
                running = Some(status);
                let ch = status & 0x0f;
                let kind = status >> 4;
                let want = if kind == 0xC || kind == 0xD { 1 } else { 2 };
                let mut d = [0u8; 2];
                for slot in d.iter_mut().take(want) {
                    let b = c.u8()?;
                    if b & 0x80 != 0 {
                        return Err(MidiError::BadData {
                            offset: c.here() - 1,
                        });
                    }
                    *slot = b;
                }
                match kind {
                    0x9 if d[1] > 0 => {
                        if *notes_left == 0 {
                            return Err(MidiError::TooManyNotes);
                        }
                        *notes_left -= 1;
                        open.entry((ch, d[0])).or_default().push_back((tick, d[1]));
                    }
                    0x8 | 0x9 => match open.get_mut(&(ch, d[0])).and_then(VecDeque::pop_front) {
                        Some((start, velocity)) => out.notes.entry(ch).or_default().push(RawNote {
                            start,
                            end: tick,
                            pitch: d[0],
                            velocity,
                        }),
                        None => out.unmatched_offs += 1,
                    },
                    _ => {}
                }
            }
            0xF0 | 0xF7 => {
                running = None;
                let n = c.varlen()? as usize;
                c.take(n)?;
            }
            0xFF => {
                running = None;
                let ty = c.u8()?;
                let n = c.varlen()? as usize;
                let data = c.take(n)?;
                match (ty, data.len()) {
                    (0x03, _) if out.name.is_none() => out.name = Some(data.to_vec()),
                    (0x51, 3) => {
                        let us = u32::from_be_bytes([0, data[0], data[1], data[2]]);
                        if us > 0 {
                            out.tempos.push((tick, us));
                        }
                    }
                    (0x58, 2..) => out.meters.push((tick, data[0], data[1])),
                    (0x2F, _) => break,
                    _ => {}
                }
            }
            _ => {
                return Err(MidiError::BadStatus {
                    offset: c.here() - 1,
                    status,
                });
            }
        }
    }
    // Notes still sounding at the end of the track end there.
    for ((ch, pitch), pending) in open {
        for (start, velocity) in pending {
            out.unterminated += 1;
            out.notes.entry(ch).or_default().push(RawNote {
                start,
                end: tick,
                pitch,
                velocity,
            });
        }
    }
    Ok(out)
}
