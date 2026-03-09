use std::path::PathBuf;

fn main() {
    let vendor = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent() // crates/
        .unwrap()
        .parent() // moneypenny/
        .unwrap()
        .join("vendor");

    if !vendor.exists() {
        panic!(
            "vendor/ directory not found at {}. \
             Run `git submodule update --init --recursive` to fetch extension sources.",
            vendor.display()
        );
    }

    // rusqlite's libsqlite3-sys exports this via `cargo:include=...`.
    // It points to the bundled SQLite 3.49.1 headers — all extensions MUST
    // compile against these instead of any system or vendored copy so that
    // the header declarations match the symbols in the linked library.
    let sqlite_include = std::env::var("DEP_SQLITE3_INCLUDE")
        .expect("DEP_SQLITE3_INCLUDE not set. rusqlite with the 'bundled' feature is required.");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    build_sqlite_vector(&vendor, &sqlite_include);
    build_sqlite_js(&vendor, &sqlite_include);
    build_sqlite_sync(&vendor, &target_os, &sqlite_include);
    build_sqlite_memory(&vendor, &sqlite_include);
    build_sqlite_mcp(&vendor, &sqlite_include);
    build_sqlite_ai(&vendor, &target_os, &sqlite_include);
}

fn build_sqlite_vector(vendor: &PathBuf, sqlite_include: &str) {
    let ext = vendor.join("sqlite-vector");
    let src = ext.join("src");
    let libs = ext.join("libs");

    cc::Build::new()
        .files([
            src.join("sqlite-vector.c"),
            src.join("distance-cpu.c"),
            src.join("distance-neon.c"),
            src.join("distance-sse2.c"),
            src.join("distance-avx2.c"),
            src.join("distance-avx512.c"),
            src.join("distance-rvv.c"),
        ])
        .include(sqlite_include)
        .include(&src)
        .include(&libs)
        .include(libs.join("fp16"))
        .define("SQLITE_CORE", None)
        .opt_level(3)
        .warnings(false)
        .compile("sqlite_vector");
}

fn build_sqlite_js(vendor: &PathBuf, sqlite_include: &str) {
    let ext = vendor.join("sqlite-js");
    let src = ext.join("src");
    let libs = ext.join("libs");

    cc::Build::new()
        .file(libs.join("quickjs.c"))
        .file(src.join("sqlitejs.c"))
        .include(sqlite_include)
        .include(&src)
        .include(&libs)
        .define("SQLITE_CORE", None)
        .define("QJS_BUILD_LIBC", None)
        .opt_level(2)
        .warnings(false)
        .compile("sqlite_js");
}

fn build_sqlite_sync(vendor: &PathBuf, target_os: &str, sqlite_include: &str) {
    let ext = vendor.join("sqlite-sync");
    let src = ext.join("src");

    let mut build = cc::Build::new();
    build
        .file(src.join("cloudsync.c"))
        .file(src.join("dbutils.c"))
        .file(src.join("lz4.c"))
        .file(src.join("pk.c"))
        .file(src.join("vtab.c"))
        .file(src.join("utils.c"))
        .include(sqlite_include)
        .include(&src)
        .define("SQLITE_CORE", None)
        .opt_level(2)
        .warnings(false);

    if target_os == "macos" || target_os == "ios" {
        // Use native NSURLSession networking — no libcurl dependency
        build.file(src.join("network.c"));
        build.file(src.join("network.m"));
        build.define("CLOUDSYNC_OMIT_CURL", None);
        println!("cargo:rustc-link-lib=framework=Foundation");
        println!("cargo:rustc-link-lib=framework=Security");
    } else {
        // On other platforms, omit network for now (CRDT merge still works locally)
        build.define("CLOUDSYNC_OMIT_NETWORK", None);
    }

    build.compile("sqlite_sync");
}

fn build_sqlite_memory(vendor: &PathBuf, sqlite_include: &str) {
    let ext = vendor.join("sqlite-memory");
    let src = ext.join("src");

    cc::Build::new()
        .file(src.join("sqlite-memory.c"))
        .file(src.join("dbmem-utils.c"))
        .file(src.join("dbmem-parser.c"))
        .file(src.join("dbmem-search.c"))
        .file(src.join("md4c.c"))
        .include(sqlite_include)
        .include(&src)
        .define("SQLITE_CORE", None)
        .define("DBMEM_OMIT_LOCAL_ENGINE", None)
        .define("DBMEM_OMIT_REMOTE_ENGINE", None)
        .opt_level(2)
        .warnings(false)
        .compile("sqlite_memory");
}

fn build_sqlite_mcp(vendor: &PathBuf, sqlite_include: &str) {
    let ext = vendor.join("sqlite-mcp");
    let src = ext.join("src");
    let libs = ext.join("libs");

    cc::Build::new()
        .file(src.join("sqlite-mcp.c"))
        .include(sqlite_include)
        .include(&src)
        .include(&libs)
        .define("SQLITE_CORE", None)
        .flag("-include")
        .flag(libs.join("sqlite3ext.h").to_str().unwrap())
        .opt_level(2)
        .warnings(false)
        .compile("sqlite_mcp");
}

fn build_sqlite_ai(vendor: &PathBuf, target_os: &str, sqlite_include: &str) {
    let ext = vendor.join("sqlite-ai");
    let src = ext.join("src");
    let llama_dir = ext.join("modules/llama.cpp");
    let whisper_dir = ext.join("modules/whisper.cpp");
    let miniaudio_dir = ext.join("modules/miniaudio");
    let fp16_dir = vendor.join("sqlite-vector/libs/fp16");

    // 1. Build llama.cpp via CMake
    let llama_build = cmake::Config::new(&llama_dir)
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("LLAMA_BUILD_COMMON", "ON")
        .define("LLAMA_BUILD_EXAMPLES", "OFF")
        .define("LLAMA_BUILD_TESTS", "OFF")
        .define("LLAMA_BUILD_TOOLS", "ON")
        .define("LLAMA_TOOLS_INSTALL", "OFF")
        .define("LLAMA_BUILD_SERVER", "OFF")
        .define("LLAMA_OPENSSL", "OFF")
        .define("GGML_RPC", "OFF")
        .define("GGML_LTO", "ON")
        .define("GGML_OPENMP", "OFF")
        .build();

    // 2. Build whisper.cpp via CMake (uses system ggml from llama build)
    let whisper_build = cmake::Config::new(&whisper_dir)
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("WHISPER_BUILD_EXAMPLES", "OFF")
        .define("WHISPER_BUILD_TESTS", "OFF")
        .define("WHISPER_BUILD_SERVER", "OFF")
        .define("WHISPER_RPC", "OFF")
        .define("WHISPER_USE_SYSTEM_GGML", "ON")
        .define(
            "CMAKE_PREFIX_PATH",
            llama_build.join("lib/cmake").display().to_string(),
        )
        .build();

    // 3. Build miniaudio via CMake
    let miniaudio_build = cmake::Config::new(&miniaudio_dir)
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("MINIAUDIO_BUILD_EXAMPLES", "OFF")
        .define("MINIAUDIO_BUILD_TESTS", "OFF")
        .build();

    // Link static libraries from CMake builds
    println!(
        "cargo:rustc-link-search=native={}",
        llama_build.join("lib").display()
    );
    println!(
        "cargo:rustc-link-search=native={}",
        llama_build.join("lib64").display()
    );
    // mtmd lives under tools/mtmd in the build
    for subdir in &["lib", "lib64"] {
        let mtmd = llama_build.join(subdir);
        if mtmd.exists() {
            println!("cargo:rustc-link-search=native={}", mtmd.display());
        }
    }

    println!(
        "cargo:rustc-link-search=native={}",
        whisper_build.join("lib").display()
    );
    println!(
        "cargo:rustc-link-search=native={}",
        whisper_build.join("lib64").display()
    );
    println!(
        "cargo:rustc-link-search=native={}",
        miniaudio_build.join("lib").display()
    );
    println!(
        "cargo:rustc-link-search=native={}",
        miniaudio_build.join("lib64").display()
    );

    println!("cargo:rustc-link-lib=static=llama");
    println!("cargo:rustc-link-lib=static=ggml");
    println!("cargo:rustc-link-lib=static=ggml-base");
    println!("cargo:rustc-link-lib=static=ggml-cpu");
    println!("cargo:rustc-link-lib=static=mtmd");
    println!("cargo:rustc-link-lib=static=whisper");
    println!("cargo:rustc-link-lib=static=miniaudio");

    // Platform-specific accelerator frameworks
    if target_os == "macos" || target_os == "ios" {
        println!("cargo:rustc-link-lib=static=ggml-metal");
        println!("cargo:rustc-link-lib=static=ggml-blas");
        println!("cargo:rustc-link-lib=framework=Accelerate");
        println!("cargo:rustc-link-lib=framework=Metal");
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
        println!("cargo:rustc-link-lib=framework=QuartzCore");
    }
    println!("cargo:rustc-link-lib=c++");

    // 4. Compile sqlite-ai.c against the built headers
    cc::Build::new()
        .file(src.join("sqlite-ai.c"))
        .file(src.join("utils.c"))
        .include(sqlite_include)
        .include(&src)
        .include(llama_build.join("include"))
        .include(whisper_dir.join("include"))
        .include(&miniaudio_dir)
        .include(llama_dir.join("tools/mtmd"))
        .include(&fp16_dir)
        .define("SQLITE_CORE", None)
        .opt_level(2)
        .warnings(false)
        .compile("sqlite_ai");
}

