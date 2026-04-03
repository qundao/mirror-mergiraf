# Docker image used in the CI (as 'ubuntu-with-rust-1.89')
FROM catthehacker/ubuntu:act-24.04

RUN apt update
RUN apt-get install rustup
RUN rustup set auto-self-update disable
ENV PATH="/root/.cargo/bin:${PATH}"
RUN rustup install 1.89.0

# Install 'cargo binstall'
RUN curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash

# Install 'git-cliff'
RUN cargo binstall git-cliff --version 2.10.0 -y

# Install 'shellcheck'
RUN apt-get install shellcheck
