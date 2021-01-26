// Copyright (c) 2021 Apple Inc.
//
// SPDX-License-Identifier: Apache-2.0
//

package virtcontainers

import (
	"encoding/json"
	"io/ioutil"
	"os"
)

const DirectAssignedVolumeJson = "csiPlugin.json"

type DiskMountInfo struct {
	// Device: source device for this volume
	Device string `json:"device,omitempty"`

	// VolumeType: type associated with this volume (ie, block)
	VolumeType string `json:"volumeType,omitempty"`

	// TargetPath: path which this device should be mounted within the guest
	TargetPath string `json:"targetPath,omitempty"`

	// FsType: filesystem that needs to be used to mount the storage inside the VM
	FsType string `json:"fsType,omitempty"`

	// Options: additional options that might be needed to mount the storage filesystem
	Options string `json:options,omitempty"`
}

// getDirectAssignedDiskInfo reads the `file` and unmarshalls it to DiskMountInfo
func getDirectAssignedDiskMountInfo(file string) (DiskMountInfo, error) {

	jsonFile, err := os.Open(file)
	if err != nil {
		return DiskMountInfo{}, err
	}
	defer jsonFile.Close()

	//read the json file:
	byteValue, err := ioutil.ReadAll(jsonFile)
	if err != nil {
		return DiskMountInfo{}, err
	}

	mountInfo := DiskMountInfo{}
	if err := json.Unmarshal(byteValue, &mountInfo); err != nil {
		return DiskMountInfo{}, err
	}

	return mountInfo, nil
}
