use crate::codec::{
    check_coord, decode_server_concat, deg_to_micro, encode_loc_frames, encode_track_start,
    encode_track_stop, format_uuid, ErrorCode, Point, ServerEvt, MAX_LOC_POINTS,
};

pub const DEFAULT_INGEST_URL: &str = "https://tracking.pickpoint.io/v2/ingest";
pub const DEFAULT_QUEUE: usize = 256;
pub const MAX_QUEUE: usize = 1024;
pub const MAX_BODY: usize = 8 * 1024;
pub const MAX_FRAMES: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    BadCoord,
    Decode,
    QueueCap,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::BadCoord => write!(f, "coordinate out of range"),
            Self::Decode => write!(f, "invalid server frame"),
            Self::QueueCap => write!(f, "queue capacity must be 1..=1024"),
        }
    }
}

impl std::error::Error for Error {}

/// POST the modem should send. Auth is always in the query string.
#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub url: String,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct FlushResult {
    pub last_acked: u64,
    pub track_uid: Option<String>,
    pub relocate_endpoint: Option<String>,
    pub retry_after_ms: Option<u32>,
    pub error_code: Option<ErrorCode>,
    pub error_message: Option<String>,
    pub stopped: bool,
}

struct Queued {
    seq: u64,
    point: Point,
}

/// Beacon ingest helper: queue + `tracking.v2` body. No HTTP client, no GPS filter.
pub struct Beacon {
    client_id: String,
    client_secret: String,
    ingest_url: String,
    cap: usize,
    queue: Vec<Queued>,
    next_seq: u64,
    last_acked: u64,
    started: bool,
    stop_pending: bool,
    track_uid: Option<String>,
}

impl Beacon {
    pub fn new(client_id: impl Into<String>, client_secret: impl Into<String>) -> Self {
        Self {
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            ingest_url: DEFAULT_INGEST_URL.into(),
            cap: DEFAULT_QUEUE,
            queue: Vec::new(),
            next_seq: 0,
            last_acked: 0,
            started: false,
            stop_pending: false,
            track_uid: None,
        }
    }

    /// Full ingest URL without query, e.g. `https://tracking.pickpoint.io/v2/ingest`.
    pub fn with_ingest_url(mut self, url: impl Into<String>) -> Self {
        self.ingest_url = url.into();
        self
    }

    /// Queue capacity 1..=1024 (default 256). Overflow drops the oldest points.
    pub fn with_queue_cap(mut self, cap: usize) -> Result<Self, Error> {
        if cap == 0 || cap > MAX_QUEUE {
            return Err(Error::QueueCap);
        }
        self.cap = cap;
        self.enforce_cap();
        Ok(self)
    }

    pub fn queue_len(&self) -> usize {
        self.queue.len()
    }

    pub fn last_acked(&self) -> u64 {
        self.last_acked
    }

    pub fn track_uid(&self) -> Option<&str> {
        self.track_uid.as_deref()
    }

    /// Enqueue a sample. `timestamp_ms` 0 omits the time flag on the wire.
    pub fn push(&mut self, lat_deg: f64, lon_deg: f64, timestamp_ms: i64) -> Result<(), Error> {
        let lat = deg_to_micro(lat_deg);
        let lon = deg_to_micro(lon_deg);
        if !check_coord(lat, lon) {
            return Err(Error::BadCoord);
        }
        self.next_seq = self.next_seq.saturating_add(1);
        self.queue.push(Queued {
            seq: self.next_seq,
            point: Point {
                lat_micro: lat,
                lon_micro: lon,
                timestamp_ms,
            },
        });
        self.enforce_cap();
        Ok(())
    }

    pub fn request_stop(&mut self) {
        self.stop_pending = true;
    }

    /// Build one POST. `None` if the queue is empty and stop was not requested.
    pub fn flush_request(&mut self) -> Option<HttpRequest> {
        if self.queue.is_empty() && !self.stop_pending {
            return None;
        }
        let mut body = Vec::new();
        let mut frames = 0usize;

        if !self.started && !self.queue.is_empty() {
            body.extend_from_slice(&encode_track_start());
            frames += 1;
        }

        if !self.queue.is_empty() {
            let take = self.queue.len().min(MAX_LOC_POINTS);
            let slice: Vec<Point> = self.queue[..take].iter().map(|q| q.point).collect();
            let last_seq = self.queue[take - 1].seq;
            for frame in encode_loc_frames(last_seq, &slice) {
                if frames >= MAX_FRAMES || body.len() + frame.len() > MAX_BODY {
                    break;
                }
                body.extend_from_slice(&frame);
                frames += 1;
            }
        }

        if self.stop_pending && frames < MAX_FRAMES && body.len() < MAX_BODY {
            body.extend_from_slice(&encode_track_stop());
        }

        if body.is_empty() {
            return None;
        }

        Some(HttpRequest {
            url: self.auth_url(),
            body,
        })
    }

    pub fn handle_response(&mut self, body: &[u8]) -> Result<FlushResult, Error> {
        let evts = decode_server_concat(body).map_err(|_| Error::Decode)?;
        let mut result = FlushResult {
            last_acked: self.last_acked,
            track_uid: self.track_uid.clone(),
            ..FlushResult::default()
        };
        for evt in evts {
            match evt {
                ServerEvt::Ack { seq } => {
                    self.last_acked = self.last_acked.max(seq);
                    self.queue.retain(|q| q.seq > seq);
                    result.last_acked = self.last_acked;
                }
                ServerEvt::TrackStarted { track_uid, .. } => {
                    self.started = true;
                    let s = format_uuid(&track_uid);
                    self.track_uid = Some(s.clone());
                    result.track_uid = Some(s);
                }
                ServerEvt::TrackStopped { .. } => {
                    self.started = false;
                    self.stop_pending = false;
                    self.track_uid = None;
                    result.stopped = true;
                }
                ServerEvt::Relocate {
                    endpoint,
                    retry_after_ms,
                } => {
                    result.relocate_endpoint = Some(endpoint);
                    result.retry_after_ms = Some(retry_after_ms);
                }
                ServerEvt::Error {
                    code,
                    message,
                    retry_after_ms,
                    ..
                } => {
                    result.error_code = Some(code);
                    result.error_message = Some(message);
                    result.retry_after_ms = retry_after_ms;
                }
            }
        }
        Ok(result)
    }

    fn auth_url(&self) -> String {
        let sep = if self.ingest_url.contains('?') {
            '&'
        } else {
            '?'
        };
        format!(
            "{}{}client-id={}&client-secret={}",
            self.ingest_url,
            sep,
            query_encode(&self.client_id),
            query_encode(&self.client_secret)
        )
    }

    fn enforce_cap(&mut self) {
        while self.queue.len() > self.cap {
            self.queue.remove(0);
        }
    }
}

fn query_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                use core::fmt::Write;
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}
