#!/bin/bash
set -e

script_path="$(realpath "${BASH_SOURCE[0]}")"
script_dir="$(dirname "${script_path}")"

cd ${script_dir}/../debug/
echo "Generating debug/base_left.svg"
dot -Tsvg base_left.dot > base_left.svg
echo "Generating debug/base_right.svg"
dot -Tsvg base_right.dot > base_right.svg
echo "Generating debug/left_right.svg"
dot -Tsvg left_right.dot > left_right.svg
