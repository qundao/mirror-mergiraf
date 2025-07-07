#!/usr/bin/env bash
#set -e

script_path="$(realpath "${BASH_SOURCE[0]}")"
script_dir="$(dirname "${script_path}")"

ext=`ls $1 | grep Base | sed -e 's/Base//'`

mkdir -p debug
extra_args="-v -d debug/"
if [ "$NO_DEBUG" == "true" ]; then
    extra_args=""
fi

if [ -e $1/language ]; then
    language=`cat $1/language`
    extra_args="$extra_args --language $language"
fi

${script_dir}/../target/debug/mergiraf merge $1/Base$ext $1/Left$ext $1/Right$ext -s BASE -x LEFT -y RIGHT $extra_args
