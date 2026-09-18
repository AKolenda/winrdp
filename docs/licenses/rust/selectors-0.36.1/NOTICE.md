# selectors 0.36.1 — licence text provenance

`selectors` declares `license = "MPL-2.0"` in its `Cargo.toml`, but neither the published
crate archive nor its upstream repository carries a copy of the licence text.

Checked and found to contain no applicable licence file:

- the published archive `selectors-0.36.1.crate`
  (`https://crates.io/api/v1/crates/selectors/0.36.1/download`,
  registry checksum `c5d9c0c92a92d33f08817311cf3f2c29a3538a8240e94a6a3c622ce652d7e00c`)
- the upstream tree `servo/stylo` at the commit the crate was published from,
  `635e1a19d02960588a00e189bd4bd5bdb150ec3d` (recorded in the archive's
  `.cargo_vcs_info.json`, `path_in_vcs = selectors`). The only licence files in that tree
  belong to other crates (`malloc_size_of`, `servo_arc`) or to vendored Python.

`LICENSE-MPL-2.0` in this directory is therefore the canonical Mozilla Public License 2.0
text as published by Mozilla at <https://www.mozilla.org/media/MPL/2.0/index.txt>, which is
the licence the crate's SPDX expression names. It is reproduced here so that this
distribution carries the licence text MPL-2.0 section 3.2 requires; it is not a file taken
from the crate's own repository.

Each `selectors` source file carries the MPL-2.0 Exhibit A header
("This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0"),
which is how the crate applies the licence.
