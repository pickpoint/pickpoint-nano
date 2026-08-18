//! Tiny HTTP ingest helper for Pickpoint GPS beacons.
//!
//! This is **not** a mini Pickpoint SDK: no geocoding, routing, listener, WebSocket,
//! GPS filter, or HTTP client. The firmware (usually a cellular modem) performs HTTPS.
//! Frames are `tracking.v2` as specified in pickpoint-proto.

mod beacon;
mod codec;

pub use beacon::{
    Beacon, Error, FlushResult, HttpRequest, DEFAULT_INGEST_URL, DEFAULT_QUEUE, MAX_BODY,
    MAX_FRAMES, MAX_QUEUE,
};
pub use codec::{
    deg_to_micro, encode_loc_frames, encode_track_start, encode_track_stop, format_uuid, ErrorCode,
    Point, ServerEvt,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{decode_server_concat, S_ACK, S_TRACK_STARTED};

    #[test]
    fn flush_puts_auth_in_query() {
        let mut b = Beacon::new("dev-1", "s ecret");
        b.push(55.0, 37.0, 1_700_000_000_000).unwrap();
        let req = b.flush_request().unwrap();
        assert!(req.url.starts_with(DEFAULT_INGEST_URL));
        assert!(req.url.contains("client-id=dev-1"));
        assert!(req.url.contains("client-secret=s%20ecret"));
        assert_eq!(req.body[0], 0x02);
        assert!(req.body.contains(&0x04));
    }

    #[test]
    fn ack_drops_prefix() {
        let mut b = Beacon::new("d", "s");
        b.push(55.0, 37.0, 0).unwrap();
        b.push(55.0001, 37.0001, 0).unwrap();
        let req = b.flush_request().unwrap();
        assert_eq!(b.queue_len(), 2);
        let mut ack = vec![S_ACK];
        ack.extend_from_slice(&1u32.to_le_bytes());
        b.handle_response(&ack).unwrap();
        assert_eq!(b.last_acked(), 1);
        assert_eq!(b.queue_len(), 1);
        let _ = req;
    }

    #[test]
    fn overflow_drops_oldest() {
        let mut b = Beacon::new("d", "s").with_queue_cap(2).unwrap();
        b.push(1.0, 2.0, 0).unwrap();
        b.push(1.0, 2.0001, 0).unwrap();
        b.push(1.0, 2.0002, 0).unwrap();
        assert_eq!(b.queue_len(), 2);
        assert!(!b.flush_request().unwrap().body.is_empty());
    }

    #[test]
    fn stop_appended() {
        let mut b = Beacon::new("d", "s");
        b.request_stop();
        let req = b.flush_request().unwrap();
        assert_eq!(req.body, vec![0x03]);
    }

    #[test]
    fn track_started_uuid() {
        let mut body = vec![S_TRACK_STARTED];
        body.extend_from_slice(&[0x11u8; 16]);
        body.extend_from_slice(&0u16.to_le_bytes());
        let evts = decode_server_concat(&body).unwrap();
        match &evts[0] {
            ServerEvt::TrackStarted { track_uid, .. } => {
                assert_eq!(format_uuid(track_uid).len(), 36);
            }
            other => panic!("{other:?}"),
        }
        let mut b = Beacon::new("d", "s");
        b.handle_response(&body).unwrap();
        assert!(b.track_uid().is_some());
    }

    #[test]
    fn empty_flush() {
        let mut b = Beacon::new("d", "s");
        assert!(b.flush_request().is_none());
    }
}
