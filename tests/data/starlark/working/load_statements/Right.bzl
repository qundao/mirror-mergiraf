load("@rules_cc//cc:defs.bzl", "cc_library")
load("@rules_rust//rust:defs.bzl", "rust_library")

cc_library(
    name = "common_lib",
    srcs = ["common.cc"],
    hdrs = ["common.h"],
)

rust_library(
    name = "rust_lib",
    srcs = ["src/lib.rs"],
    deps = ["@crates//:log"],
)
