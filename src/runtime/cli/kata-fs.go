// Copyright (c) 2021 Apple Inc.
//
// SPDX-License-Identifier: Apache-2.0
//

package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io/ioutil"
	"net/http"
	"path/filepath"

	ctrshim "github.com/kata-containers/kata-containers/src/runtime/containerd-shim-v2"
	kataMonitor "github.com/kata-containers/kata-containers/src/runtime/pkg/kata-monitor"
	"github.com/kata-containers/kata-containers/src/runtime/pkg/katautils"
	"github.com/urfave/cli"
)

const (
	podID          = "sandbox-id"
	hostBlkDevName = "host-block-device-name"
	volumeSize     = "volume-size"
	paramNamespace = "runtime-namespace"
)

var fsSubCmds = []cli.Command{
	getStatsCommand,
	resizeCommand,
}

var kataFsCLICommand = cli.Command{
	Name:        "fs",
	Usage:       "file system management",
	Subcommands: fsSubCmds,
	Action: func(context *cli.Context) {
		cli.ShowSubcommandHelp(context)
	},
}

var podIDFlag = cli.StringFlag{
	Name:  podID,
	Usage: "Pod ID associated with the given volume",
}

var blkDevFlag = cli.StringFlag{
	Name:  hostBlkDevName,
	Usage: "Block device on host",
}

var volSizeFlag = cli.StringFlag{
	Name:  volumeSize,
	Usage: "Requested size of volume in bytes",
}

var namespaceFlag = cli.StringFlag{
	Name:  paramNamespace,
	Usage: "Namespace that containerd or CRI-O are using for containers. (Default: k8s.io, only works for containerd)",
}

var getStatsCommand = cli.Command{
	Name:  "get-stats",
	Usage: "Get filesystem stats for a given volume mount",
	Flags: []cli.Flag{
		podIDFlag,
		blkDevFlag,
		namespaceFlag,
	},
	Action: func(c *cli.Context) error {

		namespace := c.String(paramNamespace)
		if namespace == "" {
			namespace = defaultRuntimeNamespace
		}

		sandboxID := c.String(podID)
		if err := katautils.VerifyContainerID(sandboxID); err != nil {
			return err
		}

		// get connection to the appropriate containerd shim
		socketAddr := filepath.Join(string(filepath.Separator), "containerd-shim", namespace, sandboxID, "shim-monitor.sock")
		client, err := kataMonitor.BuildUnixSocketClient(socketAddr, defaultTimeout)
		if err != nil {
			return err
		}

		req, err := json.Marshal(
			ctrshim.FsStatsRequest{
				BlkDevice: c.String(hostBlkDevName),
			},
		)

		resp, err := client.Post("http://shim/fs-stats", "application/json", bytes.NewReader(req))
		if err != nil {
			return err
		}
		defer resp.Body.Close()

		if resp.StatusCode != http.StatusOK {
			return fmt.Errorf("Failed to get %s: %d", socketAddr, resp.StatusCode)
		}
		data, err := ioutil.ReadAll(resp.Body)
		fmt.Printf("%s", data)

		return nil
	},
}

var resizeCommand = cli.Command{
	Name:  "resize",
	Usage: "Resize filesystem for a given volume mount",
	Flags: []cli.Flag{
		podIDFlag,
		blkDevFlag,
		volSizeFlag,
		namespaceFlag,
	},
	Action: func(c *cli.Context) error {

		namespace := c.String(paramNamespace)
		if namespace == "" {
			namespace = defaultRuntimeNamespace
		}

		sandboxID := c.String(podID)
		if err := katautils.VerifyContainerID(sandboxID); err != nil {
			return err
		}

		fmt.Println("we made it")
		return nil
	},
}
