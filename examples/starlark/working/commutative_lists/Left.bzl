load("@rules_rust//rust:defs.bzl", "rust_library")

exports_files([
    "README.md",
    "LICENSE",
])

RUST_SOURCES = glob(["src/*.rs"])

rust_library(
    name = "lib",
    srcs = [
        "src/lib.rs",
        "src/utils.rs",
    ],
    data = [
        "README.md",
        "test_data.json",
    ],
    deps = [
        "@crates//:log",
        "@crates//:regex",
    ],
)
