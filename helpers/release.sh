#!/usr/bin/env bash

set -e

if [ "$#" -ne 1 ]; then
    echo "usage: ./helpers/release.sh <new_version>"
    echo "where <new_version> is the version number for the release, such as 1.2.3"
    exit 1
fi

current_branch=$(git branch --show-current)
if [ $current_branch != "main" ]; then
    echo "You need to be on the 'main' branch to publish a release."
    exit 1
fi

version=$1

if cargo set-version -p mergiraf ${version}; then
    git add Cargo.lock Cargo.toml
    git commit -m "Set version to ${version}"
    git tag -a v${version} -m "Version ${version}"
    git push --atomic origin main:main v${version}
else
    echo ""
    echo "Failed to run 'cargo set-version'."
    echo "If it is not installed, install it with: cargo binstall cargo-edit."
    echo "NOTE: do not install cargo-set-version, that's a different tool, which isn't compatible with this script."
    exit 1
fi

