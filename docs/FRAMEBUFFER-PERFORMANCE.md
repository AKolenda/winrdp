# Framebuffer rendering

The session shares a persistent framebuffer with the client engine. Each graphics
update converts its changed rectangle into that framebuffer; pending rectangles
are combined until the window copies them. A queued notification contains no
pixels, so replacing a notification cannot discard an intermediate region.

`session/src/damage.rs` handles window clipping, black padding and softbuffer
buffer ages. A reused buffer receives the changes made since its last use. Window
or desktop size changes force a full copy. The connection bar and close dialog
contribute damage for both their previous and current locations, which restores
the desktop when they disappear. Full-frame RPC delivery remains supported.

## Local comparison, 2026-09-26

Both executables connected to the same seeded IronRDP example server over TCP on
loopback and presented to an isolated Xephyr X11 display. Each measurement used
15 seconds after the first performance report and a further three-second warmup.
CPU is the session process's user plus system CPU time from `/proc/PID/stat`, as
a percentage of one core. These are single samples of synthetic workloads on a
shared development machine, not a Windows desktop benchmark.

| Desktop | Update workload | Before CPU | After CPU | Before / after received Mbps |
| --- | --- | ---: | ---: | ---: |
| 1920 × 1080 | Rectangles up to 64 × 64, 16 ms delay | 26.0% | 3.0% | 1.15 / 1.27 |
| 2560 × 1440 | Rectangles up to 64 × 64, 16 ms delay | 27.5% | 2.7% | 1.38 / 1.39 |
| 1920 × 1080 | Random rectangles up to desktop size, 100 ms delay | 13.5% | 11.9% | 18.10 / 18.76 |
| 2560 × 1440 | Random rectangles up to desktop size, 100 ms delay | 27.2% | 22.9% | 35.67 / 34.48 |

The small-update workload shows the expected reduction in full-frame conversion
and copying. Large bitmap updates still spend most of their time decoding. Server
scheduling and encoding time affect the number of updates received in a fixed
measurement interval, even with the same random seed.

The after-build logs reported 0.2–0.3% of a core presenting small updates. For the
larger workload they reported approximately 0.1–0.2% converting and 0.1–0.2%
presenting. These counters have limited precision. The example uses the bitmap
path, so its EGFX frame counter remains zero; no FPS comparison is inferred.

Executable SHA-256 values:

- Preserved baseline: `6bd825205cbb5fd19bb36f20601c6b12c43ef70e5b37e50750085e9c68eaf397`
- Shared-framebuffer build: `e5bc91f76d85230aca1325d54ab8a0c15ce8dc64bc5e5cbef2b244dc9f30e01c`

### Reproducing the workload

Build the session with `cargo build --release -p winrdp-session`. From
`third_party/ironrdp`, build the test server with:

```sh
cargo build --release -p ironrdp --example server --features cliprdr,connector,rdpsnd,server
```

Run the example with a local test certificate and key:

```sh
target/release/examples/server \
  --bind-addr 127.0.0.1:3392 --cert test-cert.pem --key test-key.pem \
  --user tester --pass Test-pass-1 --sec hybrid \
  --size 1920x1080 --seed 42 --interval-ms 16 --max-rect 64
```

Connect a session on an isolated X11 display with `RDP_HOSTNAME=127.0.0.1:3392`,
`RDP_USERNAME=tester`, `RDP_PASSWORD=Test-pass-1`, and matching
`--desktop-width 1920 --desktop-height 1080` arguments. For larger updates, omit
`--interval-ms` and `--max-rect`; for 1440p, change the server and client dimensions
together. Restart the server for each measurement to reset seed 42.

## Validation

- Session release unit tests: 25 passed, including buffer age, resize, clipping,
  overlay restoration and damage wholly inside the right-hand padding.
- Fork client integration tests: 56 passed, including conversion bounds,
  accumulated damage, resize, reactivation, full-frame replacement and notification
  coalescing before and after delivery.
- Session Clippy with `--all-targets --no-deps -- -D warnings`: passed.
- Fork extra-test Clippy with `--no-deps -- -D warnings`: passed.
- Fork client Clippy: no new warnings; the isolated branch retains six existing
  performance-counter conversion warnings. The example-server check retains the
  existing workspace MSRV configuration warning. Dependency warnings also remain
  in the isolated branch; integration with the separate hygiene work resolves them.
- Rustfmt checks passed for the changed Rust files. The release session and
  example server built successfully.

Native Wayland presentation, Windows-host behavior and UDP performance were not
measured here. The example server does not implement desktop resizing, so a live
resize handshake also requires a separate host test.
