# Installation

You can [download Mergiraf from Codeberg](https://codeberg.org/mergiraf/mergiraf/releases).
Choose the archive that matches your device's architecture and place the extracted binary in your `PATH`.

Once Mergiraf is installed, you can then [configure Git to use Mergiraf](./usage.md#registration-as-a-git-merge-driver).

## Via a package manager

Mergiraf is also available from the following package repositories:

[![Packaging status](https://repology.org/badge/vertical-allrepos/mergiraf.svg)](https://repology.org/project/mergiraf/versions)

## From source

If you have Rust installed, you can also install Mergiraf from source by cloning [its repository](https://codeberg.org/mergiraf/mergiraf) and running `cargo install --path .` in it. Or download and build [Mergiraf on crates.io](https://crates.io/crates/mergiraf) with Cargo by running: `cargo install --locked mergiraf`.

## Pssst! Why don't you install Difftastic too?

[Difftastic](https://difftastic.wilfred.me.uk/) is a fantastic structural diff tool which works great in combination with Mergiraf.
