load("@rules_rust//rust:defs.bzl", "rust_library")

exports_files([
    "README.md",
    "LICENSE",
    "Cargo.toml",
])

RUST_SOURCES = glob([
    "src/*.rs",
    "src/**/*.rs",
])

rust_library(
    name = "lib",
    visibility = [
        "//some/package:a",
        "//some/package:b",
        "//some/package:c",
    ],
    srcs = [
        "src/lib.rs",
        "src/utils.rs",
        "src/config.rs",
    ],
    data = [
        "README.md",
        "test_data.json",
        "config.toml",
    ],
    deps = [
        "@crates//:log",
        "@crates//:regex",
        "@crates//:serde",
    ],
)
