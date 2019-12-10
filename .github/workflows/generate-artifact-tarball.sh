#!/bin/bash
# Copyright (c) 2019 Intel Corporation
#
# SPDX-License-Identifier: Apache-2.0
#

set -o errexit
set -o pipefail


main() {
    artifact_stage=${1:-}
    artifact=$(echo  ${artifact_stage} | sed -n -e 's/^install_//p' | sed -r 's/_/-/g')
    if [ -z "${artifact}" ]; then
        "Scripts needs artifact name to build"
        exit 1
    fi

    tag=$(echo $GITHUB_REF | cut -d/ -f3-)
    export GOPATH=$HOME/go

    PKG_REPO="github.com/egernst/packaging-1"

    go get ${PKG_REPO} || true
    pushd $GOPATH/src/${PKG_REPO}/release >>/dev/null
    git checkout $tag
    pushd ../obs-packaging
    ./gen_versions_txt.sh $tag
    popd

    source ./kata-deploy-binaries.sh
    ${artifact_stage} $tag
    popd

    mv $HOME/go/src/${PKG_REPO}/release/kata-static-${artifact}.tar.gz .
}

main $@
