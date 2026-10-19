# Native hinting engines

Fonts is based in part on the work of the FreeType Team and the **ttfautohint** library.

- HarfBuzz **14.6.0**: Old MIT, with per-file copyright/license notices retained. Only the upstream `src` tree and notices are included. The subsetter and FreeType bridge are built as static libraries; the application uses the subsetter to remove CFF/CFF2 hints while retaining all glyph IDs.
- ttfautohint **1.8.4**: **FTL selected**, including the upstream exception permitting unrestricted use of generated font bytecode. BSD/X11 helper files and the Bison generated-parser skeleton exception retain their own notices. The upstream library directory is included; frontends and test executables are not compiled.

Archive URLs and SHA-256 values are recorded in `manifest.json`. Original source files are unchanged except that `ttfautohint.h` is generated from its upstream template with the release version. The Fonts build script supplies a minimal portable `config.h`; the wrapper lives in `crates/font-hinting-native`. The existing vendored FreeType is reused, and no system HarfBuzz/FreeType, Python, FontForge, or command-line hinting tool is needed at runtime.

`collect_licenses.py` includes both engines' license texts and per-file leading copyright/license blocks in the application's bundled third-party notices. Original whitespace and complete license files remain in this source tree.
