use std::{env, fs, path::PathBuf};
fn main() {
    let root = PathBuf::from("../../vendor/font-hinting");
    let hb = root.join("harfbuzz/src");
    let ta = root.join("ttfautohint/lib");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    fs::write(out.join("config.h"), "#define VERSION \"1.8.4\"\n").unwrap();
    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .warnings(false)
        .include(&hb)
        .define("HAVE_FREETYPE", None)
        .include("../../vendor/freetype-sys/freetype2/include")
        .file(hb.join("hb-ft.cc"))
        .file(hb.join("harfbuzz-subset.cc"))
        .compile("fonts_harfbuzz");
    let mut build = cc::Build::new();
    build
        .warnings(false)
        .include(&out)
        .include(&ta)
        .include(&hb)
        .include("../../vendor/freetype-sys/freetype2/include");
    // Exact library sources from upstream lib/local.mk; frontends/tests are excluded.
    for entry in fs::read_dir(&ta).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|x| x == "c") {
            let name = path.file_name().unwrap().to_str().unwrap();
            if name != "sds.c" && name != "numberset-test.c" {
                build.file(path);
            }
        }
    }
    build.file("wrapper.c").compile("fonts_ttfautohint");
    println!("cargo:rerun-if-changed=wrapper.c");
    println!("cargo:rerun-if-changed={}", root.display());
}
