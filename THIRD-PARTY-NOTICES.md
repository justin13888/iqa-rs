# Third-Party Notices

The `iqa` crate itself is licensed under **MIT OR Apache-2.0** (see `LICENSE-MIT` and
`LICENSE-APACHE`).

When built with the `ssimulacra2` and/or `butteraugli` features (both on by default), `iqa`
compiles and statically links vendored native C/C++ libraries. The published crate therefore
**bundles and redistributes** the third-party components listed below, each under its own
license. This file collects their copyright notices so downstream redistributors — including
of compiled binaries, where these licenses require reproducing the notice in accompanying
materials — have them in one place.

The full, verbatim license text of each component ships alongside its sources under
`third_party/<component>/` (paths given per entry). Pure-Rust metrics (`psnr`, `ssim`,
`dssim`, `ms-ssim`, `psnr-hvs-m`, `ciede2000`) link none of this code.

> Note: the lcms2 submodule also contains GPL-3.0 plugins and IJG-licensed utilities, but
> these are excluded from the published package (`Cargo.toml` `exclude`) and are never
> compiled by `build.rs` — only `third_party/lcms2/src/*.c` is built. They are not
> redistributed and so are not listed here.

---

## ssimulacra2

- Upstream: <https://github.com/cloudinary/ssimulacra2>
- License: BSD-3-Clause
- Bundled license text: `third_party/ssimulacra2/LICENSE`

```
Copyright (c) Cloudinary.
All rights reserved.
```

## libjxl (JPEG XL) subset

Used by both `ssimulacra2` and `butteraugli`. The shared subset is vendored inside the
ssimulacra2 submodule (`third_party/ssimulacra2/src/lib/`); the few additional files that the
`butteraugli` metric needs are hand-vendored under `third_party/butteraugli/` (extracted from
libjxl; see that directory's `README.md`).

- Upstream: <https://github.com/libjxl/libjxl>
- License: BSD-3-Clause
- Bundled license text: `third_party/ssimulacra2/src/lib/LICENSE` and
  `third_party/butteraugli/LICENSE`

```
Copyright (c) the JPEG XL Project Authors.
All rights reserved.
```

## APNG Disassembler (apngdis)

Vendored within the libjxl subset at `third_party/ssimulacra2/src/lib/extras/`.

- Upstream: <http://apngdis.sourceforge.net>
- License: Zlib
- Bundled license text: `third_party/ssimulacra2/src/lib/extras/LICENSE.apngdis`

```
APNG Disassembler 2.8
Copyright (c) 2010-2015 Max Stepin
maxst at users.sourceforge.net
```

This product includes software based in part on APNG Disassembler by Max Stepin.

## Highway

- Upstream: <https://github.com/google/highway>
- License: Apache-2.0 (the project is also available under BSD-3-Clause)
- Bundled license text: `third_party/highway/LICENSE` (Apache-2.0),
  `third_party/highway/LICENSE-BSD3` (BSD-3-Clause)

```
Copyright (c) the Highway Project Authors.
All rights reserved.
```

Highway is vendored unmodified and ships no `NOTICE` file.

## Little CMS (lcms2)

- Upstream: <https://github.com/mm2/Little-CMS>
- License: MIT
- Bundled license text: `third_party/lcms2/LICENSE`; contributor list: `third_party/lcms2/AUTHORS`

```
MIT License

Copyright (c) 2023 Marti Maria Saguer
```
