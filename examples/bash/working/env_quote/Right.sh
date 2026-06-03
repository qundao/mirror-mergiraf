PKG="test"
RUSTFLAGS="-c target-cpu=native" cargo build --package ${PKG}
