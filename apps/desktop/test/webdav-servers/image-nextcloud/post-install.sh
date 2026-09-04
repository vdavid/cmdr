#!/bin/bash
# Provisions the two accounts the quota cells read. Runs as `www-data` in
# `/var/www/html`, from the Nextcloud image's post-installation hook.
#
# ❗ Both accounts exist for RFC 4331: Nextcloud answers `quota-available-bytes`
# with the negative sentinel `-3` for an unlimited account and with a real
# number for a limited one, and the backend reads those as different answers
# (`NotSupported` against a free-space figure). One account each is what makes
# both observable.
#
# ❗ A failure here does NOT stop the container: the image's `run_path` reports
# the exit code and carries on to start apache. So the last thing this script
# does is print what it provisioned, and `docker logs` is where a cell that
# fails on a missing account gets explained.
set -e

QUOTA_USER="${QUOTA_USER:-ada}"
UNLIMITED_USER="${UNLIMITED_USER:-grace}"
USER_PASSWORD="${USER_PASSWORD:-openthedoor}"
# ❗ Exactly 5 GiB, and the cell asserts that number: `quota-available-bytes`
# plus `quota-used-bytes` has to add up to the ACCOUNT's quota rather than to
# the container's disk, which is what "does the free-space indicator show the
# right thing" comes down to.
QUOTA="${QUOTA:-5GB}"

# ❗ `user:add` refuses `openthedoor` while this app is on ("present in
# compromised password list"), and the fixture password is public on purpose.
# Nothing under test touches password policy: these cells read sabre/dav's
# answers to GET, PUT, and PROPFIND.
php occ app:disable password_policy

# The admin account the image installed, given a quota it didn't have.
php occ user:setting "$QUOTA_USER" files quota "$QUOTA"

# The second account, explicitly unlimited rather than left at the instance
# default, so the sentinel is a decision this file makes and not one a future
# default quota could take away.
OC_PASS="$USER_PASSWORD" php occ user:add --password-from-env "$UNLIMITED_USER"
php occ user:setting "$UNLIMITED_USER" files quota none

echo "cmdr fixture: provisioned $QUOTA_USER (quota $QUOTA) and $UNLIMITED_USER (unlimited)"
php occ user:list
