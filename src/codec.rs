//! Minimal tracking.v2 client/server codec for HTTP ingest (no uuid crate).

pub const C_TRACK_START: u8 = 0x02;
pub const C_TRACK_STOP: u8 = 0x03;
pub const C_LOC: u8 = 0x04;

pub const S_RELOCATE: u8 = 0x81;
pub const S_TRACK_STARTED: u8 = 0x83;
pub const S_TRACK_STOPPED: u8 = 0x84;
pub const S_ACK: u8 = 0x85;
pub const S_ERROR: u8 = 0x88;

pub const MAX_LOC_POINTS: usize = 100;
pub const MAX_STRING: usize = 4096;

const PF_TIME: u8 = 1 << 4;
const LAT_MIN: i32 = -90_000_000;
const LAT_MAX: i32 = 90_000_000;
const LON_MIN: i32 = -180_000_000;
const LON_MAX: i32 = 180_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    Truncated,
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ErrorCode {
    Auth = 1,
    TrackNotFound = 2,
    Fenced = 3,
    TryAgain = 4,
    Invalid = 5,
    Unauthorized = 6,
}

impl ErrorCode {
    pub fn from_u8(v: u8) -> Option<Self> {
        Some(match v {
            1 => Self::Auth,
            2 => Self::TrackNotFound,
            3 => Self::Fenced,
            4 => Self::TryAgain,
            5 => Self::Invalid,
            6 => Self::Unauthorized,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ServerEvt {
    Relocate {
        endpoint: String,
        retry_after_ms: u32,
    },
    TrackStarted {
        track_uid: [u8; 16],
        metadata: Vec<u8>,
    },
    TrackStopped {
        track_uid: [u8; 16],
    },
    Ack {
        seq: u64,
    },
    Error {
        code: ErrorCode,
        message: String,
        track_uid: Option<[u8; 16]>,
        retry_after_ms: Option<u32>,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct Point {
    pub lat_micro: i32,
    pub lon_micro: i32,
    pub timestamp_ms: i64,
}

pub fn deg_to_micro(d: f64) -> i32 {
    (d * 1_000_000.0).round() as i32
}

pub fn micro_delta_fits(prev_lat: i32, prev_lon: i32, lat: i32, lon: i32) -> bool {
    let dlat = lat as i64 - prev_lat as i64;
    let dlon = lon as i64 - prev_lon as i64;
    dlat >= i16::MIN as i64
        && dlat <= i16::MAX as i64
        && dlon >= i16::MIN as i64
        && dlon <= i16::MAX as i64
}

struct R<'a>(&'a [u8]);

impl<'a> R<'a> {
    fn need(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        if self.0.len() < n {
            return Err(DecodeError::Truncated);
        }
        let (head, tail) = self.0.split_at(n);
        self.0 = tail;
        Ok(head)
    }
    fn u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.need(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, DecodeError> {
        Ok(u16::from_le_bytes(self.need(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, DecodeError> {
        Ok(u32::from_le_bytes(self.need(4)?.try_into().unwrap()))
    }
    fn uuid(&mut self) -> Result<[u8; 16], DecodeError> {
        Ok(self.need(16)?.try_into().unwrap())
    }
    fn uuid_opt(&mut self) -> Result<Option<[u8; 16]>, DecodeError> {
        let b = self.uuid()?;
        if b.iter().all(|x| *x == 0) {
            Ok(None)
        } else {
            Ok(Some(b))
        }
    }
    fn str(&mut self) -> Result<String, DecodeError> {
        let n = self.u16()? as usize;
        if n > MAX_STRING {
            return Err(DecodeError::Invalid);
        }
        let raw = self.need(n)?;
        String::from_utf8(raw.to_vec()).map_err(|_| DecodeError::Invalid)
    }
    fn bytes(&mut self) -> Result<Vec<u8>, DecodeError> {
        let n = self.u16()? as usize;
        if n > MAX_STRING {
            return Err(DecodeError::Invalid);
        }
        Ok(self.need(n)?.to_vec())
    }
}

fn put_u8(w: &mut Vec<u8>, v: u8) {
    w.push(v);
}
fn put_u16(w: &mut Vec<u8>, v: u16) {
    w.extend_from_slice(&v.to_le_bytes());
}
fn put_u32(w: &mut Vec<u8>, v: u32) {
    w.extend_from_slice(&v.to_le_bytes());
}
fn put_i16(w: &mut Vec<u8>, v: i16) {
    w.extend_from_slice(&v.to_le_bytes());
}
fn put_i32(w: &mut Vec<u8>, v: i32) {
    w.extend_from_slice(&v.to_le_bytes());
}
fn put_i64(w: &mut Vec<u8>, v: i64) {
    w.extend_from_slice(&v.to_le_bytes());
}

pub fn encode_track_start() -> Vec<u8> {
    let mut w = Vec::new();
    put_u8(&mut w, C_TRACK_START);
    put_u8(&mut w, 0);
    put_u16(&mut w, 0);
    put_u16(&mut w, 0);
    w
}

pub fn encode_track_stop() -> Vec<u8> {
    vec![C_TRACK_STOP]
}

fn write_point(w: &mut Vec<u8>, p: &Point, prev: Option<(i32, i32)>) {
    let mut flags = 0u8;
    if p.timestamp_ms != 0 {
        flags |= PF_TIME;
    }
    put_u8(w, flags);
    if let Some((plat, plon)) = prev {
        put_i16(w, (p.lat_micro - plat) as i16);
        put_i16(w, (p.lon_micro - plon) as i16);
    } else {
        put_i32(w, p.lat_micro);
        put_i32(w, p.lon_micro);
    }
    if p.timestamp_ms != 0 {
        put_i64(w, p.timestamp_ms);
    }
}

/// Loc frames for `points`. `last_seq` is the seq of the last point.
pub fn encode_loc_frames(last_seq: u64, points: &[Point]) -> Vec<Vec<u8>> {
    if points.is_empty() {
        return Vec::new();
    }
    let n = points.len() as u64;
    let first_seq = last_seq + 1 - n;
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < points.len() {
        let start = i;
        let mut prev = (points[i].lat_micro, points[i].lon_micro);
        i += 1;
        while i < points.len() && (i - start) < MAX_LOC_POINTS {
            let p = &points[i];
            if !micro_delta_fits(prev.0, prev.1, p.lat_micro, p.lon_micro) {
                break;
            }
            prev = (p.lat_micro, p.lon_micro);
            i += 1;
        }
        let chunk = &points[start..i];
        let seq = first_seq + i as u64 - 1;
        out.push(encode_loc_frame(seq, chunk));
    }
    out
}

fn encode_loc_frame(seq: u64, points: &[Point]) -> Vec<u8> {
    let mut w = Vec::new();
    put_u8(&mut w, C_LOC);
    put_u32(&mut w, seq as u32);
    put_u8(&mut w, points.len() as u8);
    let mut prev = None;
    for p in points {
        write_point(&mut w, p, prev);
        prev = Some((p.lat_micro, p.lon_micro));
    }
    w
}

pub fn decode_server_frame(bytes: &[u8]) -> Result<(ServerEvt, usize), DecodeError> {
    if bytes.is_empty() {
        return Err(DecodeError::Truncated);
    }
    let mut r = R(bytes);
    let typ = r.u8()?;
    let evt = match typ {
        S_RELOCATE => ServerEvt::Relocate {
            retry_after_ms: r.u32()?,
            endpoint: r.str()?,
        },
        S_TRACK_STARTED => ServerEvt::TrackStarted {
            track_uid: r.uuid()?,
            metadata: r.bytes()?,
        },
        S_TRACK_STOPPED => ServerEvt::TrackStopped {
            track_uid: r.uuid()?,
        },
        S_ACK => ServerEvt::Ack {
            seq: r.u32()? as u64,
        },
        S_ERROR => {
            let code = ErrorCode::from_u8(r.u8()?).ok_or(DecodeError::Invalid)?;
            let retry = r.u32()?;
            let track_uid = r.uuid_opt()?;
            let message = r.str()?;
            ServerEvt::Error {
                code,
                message,
                track_uid,
                retry_after_ms: if retry == 0 { None } else { Some(retry) },
            }
        }
        0x00 | 0x7F | 0xFF | 0x8C => return Err(DecodeError::Invalid),
        t if (0x80..=0xFE).contains(&t) => return Err(DecodeError::Invalid),
        _ => return Err(DecodeError::Invalid),
    };
    let consumed = bytes.len() - r.0.len();
    Ok((evt, consumed))
}

pub fn decode_server_concat(bytes: &[u8]) -> Result<Vec<ServerEvt>, DecodeError> {
    let mut rest = bytes;
    let mut out = Vec::new();
    while !rest.is_empty() {
        let (evt, n) = decode_server_frame(rest)?;
        if n == 0 {
            return Err(DecodeError::Invalid);
        }
        out.push(evt);
        rest = &rest[n..];
    }
    Ok(out)
}

/// Hyphenated lowercase UUID (16 bytes).
pub fn format_uuid(b: &[u8; 16]) -> String {
    fn hex(b: u8, out: &mut String) {
        const H: &[u8; 16] = b"0123456789abcdef";
        out.push(H[(b >> 4) as usize] as char);
        out.push(H[(b & 0x0f) as usize] as char);
    }
    let mut s = String::with_capacity(36);
    for (i, byte) in b.iter().enumerate() {
        if i == 4 || i == 6 || i == 8 || i == 10 {
            s.push('-');
        }
        hex(*byte, &mut s);
    }
    s
}

pub fn check_coord(lat: i32, lon: i32) -> bool {
    (LAT_MIN..=LAT_MAX).contains(&lat) && (LON_MIN..=LON_MAX).contains(&lon)
}
