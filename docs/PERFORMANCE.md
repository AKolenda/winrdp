# Performance

Win RDP 0.8.0, measured on 2026-09-27 on a desktop with an AMD Ryzen 7 5800X, 64 GB of
RAM and Zorin OS 18.1 (GNOME).

## On a real Windows PC

A Windows 11 PC on the same local network. Win RDP and Remmina were run one after the
other with the same window drag.

| | Win RDP 0.8.0 | Remmina 1.4.43 |
|---|---|---|
| Resize larger: Windows switches to the new size after you let go | 0.20 s | 0.49 s |
| Resize smaller: Windows switches to the new size after you let go | 0.21 s | 0.55 s |
| Resize larger: window fully redrawn | 0.75 s | 1.13 s |
| Press Escape: Start menu starts closing | 84 ms | 92 ms |
| Press the Windows key: Start menu starts opening | 252 ms | 189 ms |

Win RDP resizes about 2.5 times faster. Remmina appears to wait about half a second after
you stop dragging before it asks Windows to resize. Win RDP's larger resize took 0.20 s in
two runs and 0.35 s in one.

Keypresses are close. Win RDP closes the Start menu slightly sooner, and Remmina opens it
about 60 ms sooner. Windows opens the menu when the key is released, so Win RDP may be
sending key releases late; that is not yet explained.

Win RDP connected and signed in within 0.36 s and showed the desktop within 2.4 s.
Remmina's connection time was not measured.

## Memory and CPU compared with Remmina

Both clients connected to the same local test server with a 1920×1080 desktop. The server
draws random patterns, so this compares the clients, not a real Windows desktop.

| | Win RDP 0.8.0 | Remmina 1.4.43 (FreeRDP 3.31) |
|---|---|---|
| Small screen changes, about 60 a second: CPU | 2.9% | 5.4% |
| Small screen changes: memory | 54 MiB | 151 MiB |
| Large screen changes, about 10 a second: CPU | 13.5% | 13.8% |
| Large screen changes: memory | 57 MiB | 167 MiB |

With small changes Win RDP uses about half the CPU; with large changes the two use about
the same. Win RDP uses about a third of the memory in both cases. How Win RDP redraws only
the changed parts of the screen is in
[FRAMEBUFFER-PERFORMANCE.md](FRAMEBUFFER-PERFORMANCE.md).

## The launcher

- Idle after 12 hours open: 0.1% CPU, 19 MiB of memory.
- The window appears about 0.2 s after starting.
- The package is a 13 MB download and about 60 MB installed. It does not need FFmpeg, Qt
  or FreeRDP.

## How these were measured

- CPU is the share of one CPU core, averaged over 20 seconds starting 10 seconds after
  connecting. Each figure is a single run.
- Memory is the resident memory reported by Linux, which counts shared system libraries
  in full, as most tools show it. Counting shared libraries proportionally narrows the
  Remmina comparison to 41 against 64 MiB (small changes) and 45 against 78 MiB (large).
- Win RDP ran in a 1920×1080 window and Remmina full screen without scaling, both on the
  same separate X11 display. The test server is IronRDP's example server; the command is in
  [FRAMEBUFFER-PERFORMANCE.md](FRAMEBUFFER-PERFORMANCE.md).
- On the Windows PC both clients ran in the same nested Wayland compositor, which adds a
  little delay to both. Win RDP connected over UDP; Remmina over TCP, with dynamic
  resolution and the graphics pipeline on. Resize times run from letting go of the window
  edge to each client's log reporting the new size; "fully redrawn" is when the screen
  stopped changing, from screenshots every 10 ms. Key times are to the first changed pixel
  in part of the Start menu, the median of 5 presses.
- Frame rates over UDP and TCP on a Windows host are in the
  [README](../README.md#measured-on-a-clean-lan).
