// Copyright (c) 2021 Apple Inc.
//
// SPDX-License-Identifier: Apache-2.0
//

package main

import (
	"fmt"

	kataMonitor "github.com/kata-containers/kata-containers/src/runtime/pkg/kata-monitor"
	"github.com/kata-containers/kata-containers/src/runtime/pkg/katautils"
	"github.com/urfave/cli"
)

const (
	podID          = "sandbox-id"
	paramNamespace = "runtime-namespace"
)

var podIDFlag = cli.StringFlag{
	Name:  podID,
	Usage: "Pod ID associated with the given volume",
}

var namespaceFlag = cli.StringFlag{
	Name:  paramNamespace,
	Usage: "Namespace that containerd or CRI-O are using for containers. (Default: k8s.io, only works for containerd)",
}

var kataMetricsCLICommand = cli.Command{
	Name:  "metrics",
	Usage: "metrics for infra used to run a sandbox",
	Flags: []cli.Flag{
		podIDFlag,
	},
	Action: func(context *cli.Context) error {

		sandboxID := context.String(podID)
		if err := katautils.VerifyContainerID(sandboxID); err != nil {
			return err
		}

		// Get the metrics!
		metrics, err := kataMonitor.GetSandboxMetrics(sandboxID)
		if err != nil {
			return err
		}

		fmt.Printf("%s\n", metrics)

		return nil
	},
}
