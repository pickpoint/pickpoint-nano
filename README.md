# pickpoint-nano

Tiny **HTTP ingest** helper for Pickpoint GPS beacons. This is **not** a mini Pickpoint SDK: there is no geocoding, routing, listener, WebSocket, GPS filter, or HTTP client.

The crate queues coordinates and builds a `POST` body of concatenated [`tracking.v2`](https://github.com/pickpoint/pickpoint-proto) **client** frames. HTTPS is done by firmware (typically a cellular modem AT stack).

```toml
[dependencies]
pickpoint-nano = "0.1"
```

```rust
use pickpoint_nano::Beacon;

fn main() -> Result<(), pickpoint_nano::Error> {
    let mut beacon = Beacon::new("device-uid", "device-secret");
    beacon.push(55.75, 37.62, 1_700_000_000_000)?;
    let req = beacon.flush_request().expect("queue not empty");
    // Modem: POST req.url  body=req.body  Content-Type: application/octet-stream
    let _ = req;
    Ok(())
}
```

`flush_request` always puts `client-id` / `client-secret` in the **query string**. If the module can set headers, firmware may copy them to `X-Client-Id` / `X-Client-Secret` and strip the query; the server accepts both.

Default endpoint: `https://tracking.pickpoint.io/v2/ingest`.

Queue default **256** (max **1024**); overflow drops the **oldest** points. No Tokio, rustls, serde, or uuid.

Apache-2.0. Wire spec: [pickpoint-proto](https://github.com/pickpoint/pickpoint-proto). Full SDK (geocoding + WS tracking): [`pickpoint`](https://crates.io/crates/pickpoint).
