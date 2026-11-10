# Fonts native zlib patch

Based on crates.io libz-sys 1.1.32. Only src/zlib is replaced with the unmodified official zlib 1.4 release. Rust FFI, static build configuration, zlib-ng fallback, and upstream notices are retained. zlib 1.4 restores z_off_t to long where not otherwise configured; libc::off_t on supported desktop targets matches the selected build. Other targets require separate validation.

Source: https://zlib.net/zlib-1.4.tar.xz
SHA-256: 4f11c512053d7bab8d9e9b89bde3575add3e4c50a383b42ef6cb837035fc5a7c
