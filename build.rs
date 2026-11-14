//! Builds parajpeg's CPU decoder (its C++20 sources under `src/core` and `src/cpu`, not the CUDA backend) and the C shim in
//! `csrc/shim.cpp` into one static library that the crate links.
//!
//! On Windows the C++ is compiled with clang-cl, not MSVC. The AVX2 kernels use GCC/Clang function attributes, and the
//! runtime CPU check is `__builtin_cpu_supports`, which needs clang's builtins runtime at link time. clang-cl is the
//! executable named by `CLANG_CL`, else `C:\Program Files\LLVM\bin\clang-cl.exe`.

use std::path::{Path, PathBuf};

/// parajpeg's sources for the CPU backend, as its CMakeLists.txt lists them (minus the CUDA one).
const SOURCES: &[&str] = &[
    "src/core/parser.cpp",
    "src/core/entropy.cpp",
    "src/core/idct.cpp",
    "src/core/color.cpp",
    "src/core/decoder.cpp",
    "src/core/smoothing.cpp",
    "src/cpu/simd_neon.cpp",
    "src/cpu/simd_avx2.cpp",
    "src/cpu/simd_dispatch.cpp",
];

const DEFAULT_CLANG_CL: &str = r"C:\Program Files\LLVM\bin\clang-cl.exe";

fn main() {
    for path in ["build.rs", "csrc", "include", "src/core", "src/cpu"] {
        println!("cargo:rerun-if-changed={path}");
    }
    println!("cargo:rerun-if-env-changed=CLANG_CL");

    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default());
    let msvc = std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|env| env == "msvc");

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++20")
        .opt_level(2)
        .warnings(false)
        .include(manifest.join("include"))
        .include(manifest.join("src"));
    if msvc {
        build.compiler(clang_cl()).flag("/EHsc");
    }
    for source in SOURCES {
        build.file(manifest.join(source));
    }
    build.file(manifest.join("csrc/shim.cpp"));
    build.compile("parajpeg_cpu");

    if msvc {
        let dir = clang_builtins_dir(&clang_cl());
        println!("cargo:rustc-link-search=native={}", dir.display());
        println!("cargo:rustc-link-lib=static=clang_rt.builtins-x86_64");
    }
}

fn clang_cl() -> PathBuf {
    let path = std::env::var_os("CLANG_CL").map_or_else(|| PathBuf::from(DEFAULT_CLANG_CL), PathBuf::from);
    assert!(
        path.is_file(),
        "clang-cl not found at {}: install LLVM for Windows, or set CLANG_CL to its clang-cl.exe",
        path.display()
    );
    path
}

/// `lib\clang\<version>\lib\windows` of the LLVM that `clang_cl` belongs to: the folder with `clang_rt.builtins-x86_64.lib`.
/// The newest version wins if there are several.
fn clang_builtins_dir(clang_cl: &Path) -> PathBuf {
    let llvm = clang_cl.parent().and_then(Path::parent).unwrap_or(Path::new(r"C:\Program Files\LLVM"));
    let versions = llvm.join("lib").join("clang");
    let mut found: Vec<PathBuf> = std::fs::read_dir(&versions)
        .map(|entries| entries.filter_map(Result::ok).map(|e| e.path().join("lib").join("windows")).collect())
        .unwrap_or_default();
    found.retain(|dir| dir.join("clang_rt.builtins-x86_64.lib").is_file());
    found.sort();
    found.pop().unwrap_or_else(|| {
        panic!("clang_rt.builtins-x86_64.lib not found under {}: the LLVM install is incomplete", versions.display())
    })
}
