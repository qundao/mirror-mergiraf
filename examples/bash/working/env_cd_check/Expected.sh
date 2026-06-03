cd src || exit 1
RUSTFLAGS="-c target-cpu=native" cargo build --release
