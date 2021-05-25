#!/bin/bash

set -o errexit
set -o nounset
set -o pipefail
set -o xtrace

readonly SOURCE_PATH=${PWD}/
readonly TARGET_HOST=twixelbox@twixelbox
readonly TARGET_PORT=22
readonly TARGET_PATH=/home/twixelbox/src/twixelbox-bot

rsync -e "ssh -p ${TARGET_PORT}" -av --exclude="target/" ${SOURCE_PATH} ${TARGET_HOST}:${TARGET_PATH}
