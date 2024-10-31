#!/bin/bash

ext=`ls $1 | grep Base. | sed -e 's/Base.//'`
vimdiff $1/Base.$ext $1/Right.$ext
