#!/bin/bash
# Copyright (c) 2019 Intel Corporation
#
# SPDX-License-Identifier: Apache-2.0
#

set -o errexit
set -o pipefail

export GOPATH=$HOME/go
go get github.com/kata-containers/packaging || true
pushd $GOPATH/src/github.com/kata-containers/packaging/release >>/dev/null
# Get versions information
tag=`echo $GITHUB_REF | cut -d/ -f3-`
popd >>/dev/null

pushd kata-artifacts >>/dev/null
for c in ./*.tar.gz
do
    echo "untarring tarball $c"
    tar -xvf $c
done

tar cfJ ../kata-static.tar.xz ./opt
popd >>/dev/null
