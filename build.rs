//! Build script.
//!
//! With default features this does nothing: the `psnr` metric is pure Rust, so
//! no C or C++ compiler is invoked. When the `ssimulacra2` feature is enabled
//! it compiles the vendored C++ reference (see `mod ssimulacra2` below).

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    #[cfg(feature = "ssimulacra2")]
    ssimulacra2::build();
}

/// Compiles the vendored SSIMULACRA2 C++ reference.
///
/// This module is only compiled when the `ssimulacra2` feature is enabled, so
/// the `cc` and `pkg-config` build-dependencies stay out of the graph for a
/// default build.
#[cfg(feature = "ssimulacra2")]
mod ssimulacra2 {
    use std::path::{Path, PathBuf};
    use std::{env, fs};

    // SSIMULACRA2's color management needs exactly one lcms2 backend. Cargo
    // features are additive, so a plain pair of flags could be both enabled
    // (e.g. `--all-features`) or both disabled; neither is a valid build.
    // These guards make the choice a structural XOR: enable exactly one.
    #[cfg(all(feature = "vendored-lcms2", feature = "system-lcms2"))]
    compile_error!(
        "features `vendored-lcms2` and `system-lcms2` are mutually exclusive: enable exactly one"
    );
    #[cfg(not(any(feature = "vendored-lcms2", feature = "system-lcms2")))]
    compile_error!(
        "the `ssimulacra2` feature needs an lcms2 backend: enable exactly one of \
         `vendored-lcms2` or `system-lcms2`"
    );

    /// Minimal stand-in for the `jxl_export.h` that libjxl's CMake generates.
    ///
    /// We link the libjxl subset statically, so every visibility macro is a
    /// no-op.
    const JXL_EXPORT_H: &str = "\
#ifndef JXL_EXPORT_H
#define JXL_EXPORT_H
#define JXL_EXPORT
#define JXL_NO_EXPORT
#define JXL_DEPRECATED
#define JXL_DEPRECATED_EXPORT
#define JXL_DEPRECATED_NO_EXPORT
#define JXL_EXPORT_DEPRECATED
#endif
";

    /// Google Highway library sources (the `hwy` CMake target).
    const HIGHWAY_SOURCES: &[&str] = &[
        "aligned_allocator.cc",
        "nanobenchmark.cc",
        "per_target.cc",
        "print.cc",
        "targets.cc",
        "timer.cc",
    ];

    pub fn build() {
        let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
        let out = PathBuf::from(env::var("OUT_DIR").unwrap());

        let third_party = manifest.join("third_party");
        let s2_src = third_party.join("ssimulacra2/src");
        let s2_lib = s2_src.join("lib");
        let highway = third_party.join("highway");
        let lcms2 = third_party.join("lcms2");
        let shim = third_party.join("shim");

        assert_submodule(&s2_src.join("ssimulacra2.cc"), "ssimulacra2");
        assert_submodule(&highway.join("hwy/highway.h"), "highway");

        for path in [&third_party, &shim] {
            println!("cargo:rerun-if-changed={}", path.display());
        }

        // Write the stand-in jxl_export.h that libjxl headers include as
        // "jxl/jxl_export.h"; `out` is added to the include path below.
        let export_dir = out.join("jxl");
        fs::create_dir_all(&export_dir).expect("create OUT_DIR/jxl");
        fs::write(export_dir.join("jxl_export.h"), JXL_EXPORT_H).expect("write jxl_export.h");

        // The libjxl subset + ssimulacra2.cc + our shim, as one archive.
        // Compiled before highway/lcms2 so the static-link order resolves
        // dependencies left-to-right.
        let mut jxl = base_build();
        jxl.include(&s2_src) // resolves "lib/jxl/..." and "ssimulacra2.h"
            .include(s2_lib.join("include")) // resolves "jxl/cms_interface.h"
            .include(&highway)
            .include(&out) // resolves the generated "jxl/jxl_export.h"
            .include(&shim)
            .define("JPEGXL_MAJOR_VERSION", "0")
            .define("JPEGXL_MINOR_VERSION", "8")
            .define("JPEGXL_PATCH_VERSION", "0")
            .define("JXL_INTERNAL_LIBRARY_BUILD", None);

        // lcms2 headers for the jxl color-management sources. Vendored by
        // default; the system path returns pkg-config's include dirs instead.
        for dir in lcms2_include_dirs(&lcms2) {
            jxl.include(dir);
        }

        for src in jxl_cmake_sources(&s2_lib.join("jxl.cmake")) {
            jxl.file(s2_lib.join(src));
        }
        jxl.file(s2_src.join("ssimulacra2.cc"));
        jxl.file(shim.join("ssimulacra2_shim.cc"));
        jxl.compile("ssimulacra2_shim");

        // Highway, then lcms2: emitted after the archive that needs them.
        let mut hwy = base_build();
        hwy.include(&highway);
        for src in HIGHWAY_SOURCES {
            hwy.file(highway.join("hwy").join(src));
        }
        hwy.compile("hwy");

        // lcms2 last: compile the vendored archive (or re-probe the system
        // library) so its link directives land after the archive needing them.
        link_lcms2(&lcms2);
    }

    /// Resolves the lcms2 header include directories for the jxl build.
    ///
    /// The vendored and system backends are mutually exclusive (the
    /// `compile_error!` guards above reject both-on and both-off). The system
    /// variant additionally excludes `vendored-lcms2` so that an erroneous
    /// both-on build emits only the XOR error, not a duplicate-definition one.
    #[cfg(feature = "vendored-lcms2")]
    fn lcms2_include_dirs(lcms2: &Path) -> Vec<PathBuf> {
        assert_submodule(&lcms2.join("include/lcms2.h"), "lcms2");
        vec![lcms2.join("include")]
    }

    #[cfg(all(feature = "system-lcms2", not(feature = "vendored-lcms2")))]
    fn lcms2_include_dirs(_lcms2: &Path) -> Vec<PathBuf> {
        pkg_config::Config::new()
            .cargo_metadata(false)
            .probe("lcms2")
            .expect("lcms2 not found — install liblcms2-dev (Debian) / lcms2 (Homebrew)")
            .include_paths
    }

    /// Compiles the vendored lcms2 sources into `liblcms2.a`.
    ///
    /// lcms2 is plain C (unlike the C++ jxl subset), so it gets its own
    /// `cc::Build`. The math library is added explicitly: lcms2 calls `pow`/
    /// `sqrt`, and a static C archive does not pull `libm` in on its own.
    #[cfg(feature = "vendored-lcms2")]
    fn link_lcms2(lcms2: &Path) {
        let mut build = cc::Build::new();
        // Vendored code, not ours to fix; silence its warnings like the rest.
        build.warnings(false).include(lcms2.join("include"));
        for entry in fs::read_dir(lcms2.join("src")).expect("read third_party/lcms2/src") {
            let path = entry.expect("read third_party/lcms2/src entry").path();
            if path.extension().is_some_and(|ext| ext == "c") {
                build.file(path);
            }
        }
        build.compile("lcms2");

        if cfg!(unix) {
            println!("cargo:rustc-link-lib=m");
        }
    }

    #[cfg(all(feature = "system-lcms2", not(feature = "vendored-lcms2")))]
    fn link_lcms2(_lcms2: &Path) {
        // Re-probe with cargo metadata enabled so the link directives are
        // emitted last in the link line.
        pkg_config::Config::new()
            .probe("lcms2")
            .expect("lcms2 not found — install liblcms2-dev (Debian) / lcms2 (Homebrew)");
    }

    /// A `cc::Build` configured for C++11 with libjxl's expected flags.
    fn base_build() -> cc::Build {
        let mut build = cc::Build::new();
        build
            .cpp(true)
            .std("c++11")
            .flag_if_supported("-fno-rtti")
            // libjxl is warning-heavy; it is vendored code, not ours to fix.
            .warnings(false);
        build
    }

    /// Extracts the uncommented `.cc` entries from libjxl's `jxl.cmake` source
    /// list, returning paths relative to `lib/`.
    fn jxl_cmake_sources(cmake: &Path) -> Vec<String> {
        let text =
            fs::read_to_string(cmake).unwrap_or_else(|e| panic!("read {}: {e}", cmake.display()));
        text.lines()
            .map(str::trim)
            .filter(|line| !line.starts_with('#'))
            .filter(|line| line.ends_with(".cc"))
            .map(str::to_owned)
            .collect()
    }

    fn assert_submodule(probe: &Path, name: &str) {
        assert!(
            probe.exists(),
            "third_party/{name} is not checked out (missing {}).\n\
             Run: git submodule update --init --recursive",
            probe.display(),
        );
    }
}
