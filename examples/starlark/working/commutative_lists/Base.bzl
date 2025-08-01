load("@rules_rust//rust:defs.bzl", "rust_library")

exports_files([
    "README.md",
])

RUST_SOURCES = glob(["src/*.rs"])

rust_library(
    name = "lib",
    srcs = [
        "src/lib.rs",
    ],
    data = [
        "README.md",
    ],
    deps = [
        "@crates//:log",
    ],
)
