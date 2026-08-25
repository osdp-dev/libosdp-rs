#!/usr/bin/env bash
#
#  Copyright (c) 2024 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
#
#  SPDX-License-Identifier: Apache-2.0
#

usage() {
	cat >&2<<----
	LibOSDP release helper

	OPTIONS:
	  -c, --component	Component to release (one of libosdp, libosdp-sys, osdpctl)
	  -v, --version		libosdp-sys only: vendor this libosdp tag (default: newest
				                        on the branch the vendor submodule tracks)
	  --patch		Release version bump type: patch (default)
	  --major		Release version bump type: major
	  --minor		Release version bump type: minor
	  -h, --help		Print this help
	---
}

function cargo_set_version() {
	dir=$1
	ver=$2
	perl -pi -se '
	if (/^version = "\d+\.\d+\.\d+"$/) {
		$_="version = \"$ver\"\n"
	}' -- -ver=$ver $dir/Cargo.toml
}

function cargo_get_version() {
	dir=$1
	perl -ne 'print $1 if (/^version = "(.+)"$/)' $dir/Cargo.toml
}

# Pin a dependency to an exact version (=x.y.z) in a crate's Cargo.toml.
# libosdp and libosdp-sys are released in lockstep and libosdp-sys ships
# breaking changes in minor bumps, so libosdp must never float across them.
function cargo_pin_dep() {
	dir=$1
	dep=$2
	ver=$3
	perl -pi -se '
	if (/^\Q$dep\E = "=?\d+\.\d+\.\d+"$/) {
		$_="$dep = \"=$ver\"\n"
	}' -- -dep="$dep" -ver="$ver" $dir/Cargo.toml
}

function cargo_inc_version() {
	dir=$1
	inc=$2
	perl -pi -se '
	if (/^version = "(\d+)\.(\d+)\.(\d+)"$/) {
		$maj=$1; $min=$2; $pat=$3;
		if ($major) { $maj+=1; $min=0; $pat=0; }
		if ($minor) { $min+=1; $pat=0; }
		$pat+=1 if $patch;
		$_="version = \"$maj.$min.$pat\"\n"
	}' -- -$inc $dir/Cargo.toml
}

function commit_release() {
	crate=$1
	version=$(cargo_get_version $crate)
	# Refuse to tag anything the release gate would reject in CI. Catching it
	# here costs a rerun; catching it after publish costs a burnt version,
	# because crates.io has no unpublish.
	if [[ "$crate" != "osdpctl" ]]; then
		bash $(dirname ${BASH_SOURCE[0]})/check-release.sh \
			-c $crate -t "$crate-v$version" || return 1
	fi
	git add $crate/Cargo.toml Cargo.lock &&
	git commit -s -m "$crate: Release v$version" &&
	git tag "$crate-v$version" -s -a -m "Release $version"
}

function do_cargo_release() {
	crate=$1
	inc=$2
	cargo_inc_version $crate $inc
	commit_release $crate
}

# The branch the vendor submodule follows, so that a release branch steps
# along its own line instead of picking up tags from master.
function vendor_branch() {
	git config -f .gitmodules submodule.vendor.branch || echo master
}

function vendor_newest_tag() {
	branch=$(vendor_branch)
	git -C libosdp-sys/vendor fetch -q --tags origin $branch
	git -C libosdp-sys/vendor tag --list 'v*' --sort=-v:refname \
		--merged FETCH_HEAD | head -1
}

function do_libosdp_sys_bump() {
	target=$1
	[[ -z "$target" ]] && target=$(vendor_newest_tag)
	version=${target#"v"}
	current=$(cargo_get_version libosdp-sys)
	if [[ "$current" == "$version" ]]; then
		echo "libosdp-sys is already at $version, nothing to be done"
		return
	fi
	# Releases on one line are not reachable from another, so resolving the
	# newest tag can land behind the current version if the vendor submodule
	# tracks the wrong branch. Never walk a published crate backwards.
	if [[ "$(printf '%s\n%s\n' "$current" "$version" | sort -V | tail -1)" != "$version" ]]; then
		echo "Refusing to move libosdp-sys $current back to $version" >&2
		echo "Check 'submodule.vendor.branch' in .gitmodules" >&2
		return 1
	fi
	git -C libosdp-sys/vendor fetch -q --tags origin
	git -C libosdp-sys/vendor checkout -q $target || return 1
	git -C libosdp-sys/vendor submodule update --init --recursive
	cargo clean -p libosdp-sys
	CCACHE_DISABLE=1 LIBOSDP_SYS_REGENERATE_BINDINGS=1 cargo build -p libosdp-sys
	cargo_set_version libosdp-sys $version
	git add libosdp-sys/vendor libosdp-sys/src/bindings.rs
	commit_release libosdp-sys
}

function do_libosdp_release() {
	inc=$1
	cargo_pin_dep libosdp libosdp-sys "$(cargo_get_version libosdp-sys)"
	do_cargo_release "libosdp" $inc
}

function do_release() {
	case $1 in
	libosdp-sys) do_libosdp_sys_bump "$3" ;;
	libosdp) do_libosdp_release $2 ;;
	osdpctl) do_cargo_release "osdpctl" $2 ;;
	*) echo -e "Must pass -c with a known component\n"; usage; exit 1;;
	esac
}

INC="patch"
CRATE=""
VERSION=""
while [ $# -gt 0 ]; do
	case $1 in
	-c|--component)		CRATE=$2; shift;;
	-v|--version)		VERSION=$2; shift;;
	--patch)		INC="patch";;
	--major)		INC="major";;
	--minor)		INC="minor";;
	-h|--help)             usage; exit 0;;
	*) echo -e "Unknown option $1\n"; usage; exit 1;;
	esac
	shift
done

do_release "$CRATE" "$INC" "$VERSION"
