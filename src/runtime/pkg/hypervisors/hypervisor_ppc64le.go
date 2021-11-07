// Copyright (c) 2021 IBM
//
// SPDX-License-Identifier: Apache-2.0

package hypervisors

import "os"

//Returns pefProtection if the firmware directory exists
func availableGuestProtection() (guestProtection, error) {

	if d, err := os.Stat(pefSysFirmwareDir); err == nil && d.IsDir() {
		return pefProtection, err
	}

	return noneProtection, nil
}
