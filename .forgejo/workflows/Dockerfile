# Docker image used in the CI (as 'ubuntu-with-rust-1.91.1')
FROM catthehacker/ubuntu:act-24.04

RUN apt update
RUN apt-get install rustup
RUN rustup set auto-self-update disable
ENV PATH="/root/.cargo/bin:${PATH}"
RUN rustup install 1.91.1

# Install 'cargo binstall'
RUN curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash

# Install 'git-cliff'
RUN cargo binstall git-cliff --version 2.10.0 -y

# Install `jj`
RUN cargo binstall jj-cli --version 0.41.0 -y --strategies crate-meta-data 

# Install 'shellcheck'
RUN apt-get install shellcheck

# Install a newer nodejs
# TODO: this should be dropped once we migrate to act-26 (it's not out yet as of 2026-09-24)
RUN curl -fsSL https://deb.nodesource.com/setup_24.x | sudo -E bash - && sudo apt-get install -y nodejs
