# pickpoint-nano

Tiny **HTTP ingest** helper for Pickpoint GPS beacons. This is **not** a mini Pickpoint SDK: there is no geocoding, routing, listener, WebSocket, GPS filter, or HTTP client.

The crate queues coordinates and builds a `POST` body of concatenated [`tracking.v2`](https://github.com/pickpoint/pickpoint-proto) **client** frames. HTTPS is done by firmware (typically a cellular modem AT stack).

```toml
[dependencies]
pickpoint-nano = "2"
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

### CI & release

- **PR to `dev`** → `.github/workflows/ci.yml` (`fmt`, `clippy`, `test`)
- **Merge `dev` → `main`** (untagged HEAD) → bump **patch** in `Cargo.toml`, tag `vX.Y.Z`, `cargo publish` (OIDC) + GitHub Release in the same job  
  (tag push via `GITHUB_TOKEN` does not start new workflows — publish cannot wait on the tag event)
- **Manual tag `v*`** (pushed by a human) → publish + GitHub Release

Minor/major: bump `version` in `Cargo.toml` in a PR, merge with `[skip release]` in the commit message, then:

```bash
git tag v2.1.0
git push origin v2.1.0
```

First `2.0.0` on crates.io: merge the initial commit with `[skip release]`, then `git tag v2.0.0 && git push origin v2.0.0`. A plain push to `main` would auto-bump to `2.0.1`.

crates.io Trusted Publishing must match this workflow: repo `pickpoint-nano`, workflow `release.yml` (leave Environment empty).

## Contributing

Fork and open a PR against **`dev`**. [CONTRIBUTING.md](CONTRIBUTING.md).
