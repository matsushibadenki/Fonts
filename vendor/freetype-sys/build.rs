use std::env;

fn add_sources(build: &mut cc::Build, root: &str, files: &[&str]) {
    let root = std::path::Path::new(root);
    build.files(files.iter().map(|src| {
        let mut p = root.join(src);
        p.set_extension("c");
        p
    }));

    build.include(root);
}

fn main() {
    if !cfg!(feature = "bundled") {
        let target = env::var("TARGET").unwrap();
        if !target.contains("android") && !target.contains("ohos") {
            pkg_config::Config::new()
                .atleast_version("24.3.18")
                .probe("freetype2")
                .unwrap();
        }
        return;
    }

    let mut build = cc::Build::new();
    build.include(env::var_os("OUT_DIR").unwrap());
    if let Some(paths) = env::var_os("DEP_Z_INCLUDE") {
        for path in env::split_paths(&paths) {
            build.include(path);
        }
    }

    build
        .warnings(false)
        .include(".")
        .include("freetype2/include")
        .define("FT2_BUILD_LIBRARY", None)
        // Use libz-sys's bundled stable zlib instead of FreeType's older private copy.
        .define("FT_CONFIG_OPTION_SYSTEM_ZLIB", None);

    add_sources(
        &mut build,
        "freetype2/src",
        &[
            "autofit/autofit",
            "base/ftbase",
            "base/ftbbox",
            "base/ftbdf",
            "base/ftbitmap",
            "base/ftcid",
            "base/ftdebug",
            "base/ftfstype",
            "base/ftgasp",
            "base/ftglyph",
            "base/ftgxval",
            "base/ftinit",
            "base/ftmm",
            "base/ftotval",
            "base/ftpatent",
            "base/ftpfr",
            "base/ftstroke",
            "base/ftsynth",
            "base/ftsystem",
            "base/fttype1",
            "base/ftwinfnt",
            "bdf/bdf",
            "bzip2/ftbzip2",
            "cache/ftcache",
            "cff/cff",
            "cid/type1cid",
            "gzip/ftgzip",
            "lzw/ftlzw",
            "pcf/pcf",
            "pfr/pfr",
            "psaux/psaux",
            "pshinter/pshinter",
            "psnames/psnames",
            "raster/raster",
            "sdf/sdf",
            "svg/svg",
            "sfnt/sfnt",
            "smooth/smooth",
            "truetype/truetype",
            "type1/type1",
            "type42/type42",
            "winfonts/winfnt",
        ],
    );

    build.compile("freetype2");

    println!("cargo:rustc-link-lib=z");

    // Fonts uses vector outlines only; do not build or enable the optional PNG decoder.
}
