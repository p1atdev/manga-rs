use std::io::Result;

/// Compiles the libjpeg coefficient shim against the headers bundled with `mozjpeg-sys`.
///
/// `mozjpeg-sys` exports its include directories (vendored sources plus the generated `jconfig.h`)
/// as a platform path list in `DEP_JPEG_INCLUDE`, so it must be split with `split_paths`.
fn build_direct_sort() {
    use std::env;

    println!("cargo:rerun-if-changed=csrc/direct_sort.c");
    println!("cargo:rerun-if-changed=csrc/direct_sort.h");

    let include = env::var_os("DEP_JPEG_INCLUDE")
        .or_else(|| env::var_os("DEP_MOZJPEG_INCLUDE"))
        .expect("mozjpeg-sys did not export DEP_JPEG_INCLUDE");

    let mut build = cc::Build::new();
    build
        .file("csrc/direct_sort.c")
        .include("csrc")
        .warnings(true)
        .extra_warnings(true)
        .warnings_into_errors(true);
    for dir in env::split_paths(&include).filter(|p| !p.as_os_str().is_empty()) {
        build.include(dir);
    }
    build.compile("manga_direct_sort");
}

fn main() -> Result<()> {
    build_direct_sort();
    Ok(())
}
