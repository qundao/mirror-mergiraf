#!/bin/bash
#set -e

ext=`ls $1 | grep Base. | sed -e 's/Base.//'`

mkdir -p debug
extra_args="-v -d debug/"
if [ "$NO_DEBUG" == "true" ]; then
    extra_args=""
fi

cargo run --bin mergiraf merge $1/Base.$ext $1/Left.$ext $1/Right.$ext -s BASE -x LEFT -y RIGHT $extra_args
