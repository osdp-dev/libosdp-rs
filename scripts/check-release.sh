#!/usr/bin/env bash
#
#  Copyright (c) 2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
#
#  SPDX-License-Identifier: Apache-2.0
#
# Release gate. Verifies that a crate's metadata is internally consistent
# before anything is published to crates.io. Run locally before tagging and
# from the publish workflows before the crate is uploaded.

set -euo pipefail

usage() {
	cat >&2<<---
	LibOSDP release gate

	OPTIONS:
	  -c, --component <name>	Crate to check: libosdp-sys or libosdp
	  -t, --tag <tag>		Assert the crate version matches this git tag
	  -r, --registry		Assert dependencies resolve on crates.io (needs network)
	  -h, --help			Print this help
	--
}

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FAILED=0

function fail() {
	echo "FAIL: $*" >&2
	FAILED=1
}

function pass() {
	echo "ok: $*"
}

function manifest_version() {
	perl -ne 'if (/^version = "(.+)"$/) { print $1; exit }' "$ROOT/$1/Cargo.toml"
}

# The vendored C sources carry their own version in CMakeLists.txt. Comparing
# against that (rather than the submodule's tag) checks the code we actually
# compile, and works in a shallow CI checkout with no tags fetched.
function vendor_version() {
	perl -ne 'if (/^project\(libosdp VERSION (\S+)\)/) { print $1; exit }' \
		"$ROOT/libosdp-sys/vendor/CMakeLists.txt"
}

function dep_requirement() {
	perl -ne 'if (/^\Q'"$2"'\E = "(=?\d+\.\d+\.\d+)"$/) { print $1; exit }' \
		"$ROOT/$1/Cargo.toml"
}

function check_tag() {
	crate=$1
	version=$2
	tag=$3
	[[ -z "$tag" ]] && return 0
	if [[ "$tag" != "$crate-v$version" ]]; then
		fail "tag '$tag' does not match $crate/Cargo.toml version '$version'"
		echo "      a mismatch here publishes '$version' under the name '$tag'" >&2
		return 0
	fi
	pass "tag '$tag' matches $crate/Cargo.toml"
}

function check_on_crates_io() {
	crate=$1
	version=$2
	url="https://crates.io/api/v1/crates/$crate/$version"
	if curl -sfL -o /dev/null -A "libosdp-rs-release-gate" "$url"; then
		pass "$crate $version is published on crates.io"
	else
		fail "$crate $version is not on crates.io yet"
		echo "      publish it first; crates.io rejects an unresolvable '=' pin" >&2
	fi
}

function check_libosdp_sys() {
	tag=$1
	version=$(manifest_version libosdp-sys)
	vendored=$(vendor_version)

	if [[ -z "$vendored" ]]; then
		fail "could not read a version from libosdp-sys/vendor/CMakeLists.txt"
		echo "      is the vendor submodule checked out?" >&2
	elif [[ "$version" != "$vendored" ]]; then
		fail "libosdp-sys $version has libosdp $vendored vendored"
		echo "      the sys crate version must equal the C library version" >&2
	else
		pass "vendored libosdp $vendored matches libosdp-sys/Cargo.toml"
	fi

	check_tag libosdp-sys "$version" "$tag"
}

function check_libosdp() {
	tag=$1
	registry=$2
	version=$(manifest_version libosdp)
	sys_version=$(manifest_version libosdp-sys)
	pin=$(dep_requirement libosdp libosdp-sys)

	if [[ "$pin" != "="* ]]; then
		fail "libosdp depends on libosdp-sys '$pin', which is not an exact pin"
		echo "      libosdp-sys ships breaking C API changes in minor bumps," >&2
		echo "      so a caret range would silently cross an ABI boundary" >&2
	elif [[ "${pin#=}" != "$sys_version" ]]; then
		fail "libosdp pins libosdp-sys '$pin' but the workspace builds $sys_version"
	else
		pass "libosdp pins libosdp-sys $pin, matching the workspace"
	fi

	check_tag libosdp "$version" "$tag"

	if [[ "$registry" == "1" && "$pin" == "="* ]]; then
		check_on_crates_io libosdp-sys "${pin#=}"
	fi
}

CRATE=""
TAG=""
REGISTRY=0
while [ $# -gt 0 ]; do
	case $1 in
	-c|--component)	CRATE=$2; shift;;
	-t|--tag)	TAG=$2; shift;;
	-r|--registry)	REGISTRY=1;;
	-h|--help)	usage; exit 0;;
	*) echo -e "Unknown option $1\n" >&2; usage; exit 1;;
	esac
	shift
done

case $CRATE in
libosdp-sys)	check_libosdp_sys "$TAG" ;;
libosdp)	check_libosdp "$TAG" "$REGISTRY" ;;
*)		echo -e "Must pass -c libosdp-sys or -c libosdp\n" >&2; usage; exit 1 ;;
esac

if [[ "$FAILED" != "0" ]]; then
	echo "release gate: FAILED" >&2
	exit 1
fi
echo "release gate: passed"
