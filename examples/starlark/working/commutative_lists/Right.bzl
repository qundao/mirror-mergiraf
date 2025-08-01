load("@rules_rust//rust:defs.bzl", "rust_library")

exports_files([
    "README.md",
    "Cargo.toml",
])

RUST_SOURCES = glob([
    "src/*.rs",
    "src/**/*.rs",
])

rust_library(
    name = "lib",
    srcs = [
        "src/lib.rs",
        "src/config.rs",
    ],
    data = [
        "README.md",
        "config.toml",
    ],
    deps = [
        "@crates//:log",
        "@crates//:serde",
    ],
)
