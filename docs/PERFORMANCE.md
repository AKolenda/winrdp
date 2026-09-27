# Performance

Win RDP 0.8.0, measured on 2026-09-27 on a desktop with an AMD Ryzen 7 5800X, 64 GB of
RAM and Zorin OS 18.1 (GNOME).

## On a real Windows PC

A Windows 11 PC on the same local network, connected the way the launcher connects.

| | |
|---|---|
| Connect and sign in | 0.36 s |
| First picture of the desktop | within 2.4 s |
| Resize the window larger (1280×720 to 1660×840) | Windows redraws at the new size 0.35 s after you let go; the picture is complete by 0.65 s |
| Resize the window smaller (1660×840 to 960×460) | Windows redraws at the new size 0.21 s after you let go |
| Press a key, see the result | about 80 ms (54 to 89 ms over 5 tries) |

The key test pressed Escape to close the Start menu and timed the first change on screen.
Opening the Start menu takes about 250 ms, most of it Windows deciding to show the menu.

## Compared with Remmina

Both clients connected to the same local test server with a 1920×1080 desktop. The server
draws random patterns, so this compares the clients, not a real Windows desktop.

| | Win RDP 0.8.0 | Remmina 1.4.43 (FreeRDP 3.31) |
|---|---|---|
| Small screen changes, about 60 a second: CPU | 2.9% | 5.4% |
| Small screen changes: memory | 54 MiB | 151 MiB |
| Large screen changes, about 10 a second: CPU | 13.5% | 13.8% |
| Large screen changes: memory | 57 MiB | 167 MiB |

With small changes Win RDP uses about half the CPU; with large changes the two use about
the same. Win RDP uses about a third of the memory in both cases. How Win RDP redraws only the changed parts of the screen is in
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
- Resize times run from releasing the window edge to the session log reporting the new
  size, checked against screenshots taken about every 0.3 s. Key times were measured
  through a nested Wayland compositor, which adds a little delay of its own.
- Frame rates over UDP and TCP on a Windows host are in the
  [README](../README.md#measured-on-a-clean-lan).
