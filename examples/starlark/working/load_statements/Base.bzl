load("@rules_cc//cc:defs.bzl", "cc_library")

cc_library(
    name = "common_lib",
    srcs = ["common.cc"],
    hdrs = ["common.h"],
)
