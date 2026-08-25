#!/usr/bin/env python3
#
#  Copyright (c) 2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
#
#  SPDX-License-Identifier: Apache-2.0
#
"""libosdp-rs release helper — a human-in-the-loop release flow in two shapes.

Every command takes -c/--crate, because the three crates in this workspace
version independently: libosdp-sys tracks the vendored C library exactly,
while libosdp and osdpctl carry their own Rust-side semver.

QUICK (the default) — one commit, for patch releases and anything else that
does not need a long cycle:

    1) prepare  — bump the crate to its final released state and scaffold
                  changelog/<crate>-v<next>.md with commit hints. Staged, NOT
                  committed. No pre-release marker.

    2) (edit the changelog: fold the ## Changes hints in and delete them)

    3) publish  — validate the changelog, then fold the version bump and the
                  changelog into a single "Release <tag>" commit and lay a
                  GPG-signed tag.

CYCLE (prepare --cycle) — two commits, for a release prepared long before it
ships. Identical, except that prepare also sets the pre-release marker and you
commit "Prepare <tag>"; the crate then reports e.g. 4.0.0-dev for the whole
cycle, and publish clears the marker in the "Release <tag>" commit.

Either way the tag's signature must match https://github.com/sidcha.gpg, and
nothing is ever pushed — the push command is printed for you to run.

publish infers which shape it is finishing by comparing the worktree's version
against HEAD's: differing => the prepare was never committed => quick. So a
--cycle prepare you decide not to commit simply publishes as a quick release.

libosdp-sys is not bumped, it is vendored: its crate version IS the C library
version, so prepare takes --vendor <tag> (default: the newest libosdp release
on the branch .gitmodules tracks), checks out that tag, regenerates bindings
and sets the crate version to match.

Examples:
    scripts/make_release.py prepare -c libosdp-sys --vendor v3.2.3
    scripts/make_release.py publish -c libosdp-sys

    scripts/make_release.py prepare -c libosdp --patch
    scripts/make_release.py publish -c libosdp

    scripts/make_release.py prepare -c libosdp --cycle --set 1.0.0
    scripts/make_release.py publish -c libosdp
"""

import argparse
import dataclasses
import os
import re
import subprocess
import sys
import tempfile
import urllib.error
import urllib.request
from datetime import UTC, datetime
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import changelog_tool  # noqa: E402  (sibling script, reused for changelog logic)

DEFAULT_KEY_URL = "https://github.com/sidcha.gpg"

# Primary key of the release identity. Signatures are accepted from any valid
# subkey it certifies, so a routine subkey rotation needs no change here.
RELEASE_KEY = "D8861B9C6B9C4284D980D6016804DFEE281234B2"

MARKER = "dev"  # the only pre-release marker value

CRATES = changelog_tool.CRATES

VENDOR = "libosdp-sys/vendor"
BINDINGS = "libosdp-sys/src/bindings.rs"


def die(message: str) -> None:
    print(f"error: {message}", file=sys.stderr)
    raise SystemExit(1)


@dataclasses.dataclass(frozen=True)
class Version:
    major: int
    minor: int
    patch: int
    prerelease: bool = False

    def __str__(self) -> str:
        base = f"{self.major}.{self.minor}.{self.patch}"
        return f"{base}-{MARKER}" if self.prerelease else base

    @property
    def core(self) -> tuple[int, int, int]:
        return (self.major, self.minor, self.patch)

    def bumped(self, kind: str) -> "Version":
        if kind == "major":
            return Version(self.major + 1, 0, 0)
        if kind == "minor":
            return Version(self.major, self.minor + 1, 0)
        return Version(self.major, self.minor, self.patch + 1)

    def released(self) -> "Version":
        return dataclasses.replace(self, prerelease=False)


VERSION_RE = re.compile(r"^(\d+)\.(\d+)\.(\d+)(?:-(dev))?$")


def parse_version(raw: str) -> Version:
    match = VERSION_RE.fullmatch(raw.strip())
    if not match:
        die(f"Not an X.Y.Z[-dev] version: {raw}")
    return Version(
        int(match.group(1)), int(match.group(2)), int(match.group(3)),
        prerelease=bool(match.group(4)),
    )


# ---------------------------------------------------------------------------
# git plumbing
# ---------------------------------------------------------------------------


def git(args: list[str], root: Path, check: bool = True,
        strip: bool = True) -> str:
    result = subprocess.run(
        ["git", *args], cwd=root, capture_output=True, text=True
    )
    if check and result.returncode != 0:
        die(f"git {' '.join(args)} failed: {result.stderr.strip()}")
    # strip=False matters for --porcelain, whose leading status column is
    # significant: stripping turns " M path" into "M path" and shifts the name.
    return result.stdout.strip() if strip else result.stdout


def repo_root() -> Path:
    result = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"], capture_output=True, text=True
    )
    if result.returncode != 0:
        die("not inside a git work tree")
    return Path(result.stdout.strip())


def tag_exists(root: Path, tag: str) -> bool:
    return bool(git(["tag", "-l", tag], root))


def branch(root: Path) -> str:
    return git(["symbolic-ref", "--short", "-q", "HEAD"], root, check=False) or "HEAD"


# ---------------------------------------------------------------------------
# crate version state
# ---------------------------------------------------------------------------


def manifest(crate: str) -> str:
    return f"{crate}/Cargo.toml"


def version_files(crate: str) -> list[str]:
    """Tracked files a release of `crate` is allowed to touch, changelog aside.

    libosdp-sys also moves the vendor gitlink and the checked-in bindings,
    because its version is defined by which C release it vendors.
    """
    files = [manifest(crate)]
    if crate == "libosdp-sys":
        files += [VENDOR, BINDINGS]
    return files


MANIFEST_VERSION_RE = re.compile(r'^version = "([^"]+)"$', re.M)
SYS_DEP_RE = re.compile(r'^libosdp-sys = "(=?[^"]+)"$', re.M)


def parse_manifest_version(name: str, text: str) -> Version:
    match = MANIFEST_VERSION_RE.search(text)
    if not match:
        die(f"{name}: could not parse a version")
    return parse_version(match.group(1))


def read_state(root: Path, crate: str) -> Version:
    return parse_manifest_version(manifest(crate),
                                  (root / manifest(crate)).read_text())


def head_state(root: Path, crate: str) -> Version:
    """The version recorded in the last commit.

    Compared against the worktree, this is what distinguishes a quick release
    (bump still uncommitted) from a cycle whose Prepare commit already landed.
    """
    text = git(["show", f"HEAD:{manifest(crate)}"], root, check=False)
    if not text:
        die(f"cannot read {manifest(crate)} at HEAD — not a libosdp-rs "
            "checkout, or the branch has no commits yet")
    return parse_manifest_version(manifest(crate), text)


def _sub_once(name: str, pattern: re.Pattern[str], repl: str, text: str) -> str:
    new, count = pattern.subn(repl, text, count=1)
    if count != 1:
        die(f"{name}: expected exactly one match for /{pattern.pattern}/, "
            f"found {count}")
    return new


def write_state(root: Path, crate: str, version: Version) -> None:
    path = root / manifest(crate)
    text = _sub_once(manifest(crate), MANIFEST_VERSION_RE,
                     f'version = "{version}"', path.read_text())
    path.write_text(text)


def read_pin(root: Path) -> str:
    """libosdp's declared requirement on libosdp-sys, e.g. '=3.2.2'."""
    match = SYS_DEP_RE.search((root / manifest("libosdp")).read_text())
    if not match:
        die("libosdp/Cargo.toml: could not parse the libosdp-sys dependency")
    return match.group(1)


def write_pin(root: Path, version: Version) -> None:
    """Pin libosdp to an exact libosdp-sys version.

    The pin is exact because libosdp-sys ships breaking C API changes in minor
    bumps; a caret range would silently carry libosdp across an ABI boundary.
    """
    path = root / manifest("libosdp")
    text = _sub_once("libosdp/Cargo.toml", SYS_DEP_RE,
                     f'libosdp-sys = "={version}"', path.read_text())
    path.write_text(text)


# ---------------------------------------------------------------------------
# vendored C library (libosdp-sys only)
# ---------------------------------------------------------------------------


CMAKE_VERSION_RE = re.compile(r"^project\(libosdp VERSION (\d+\.\d+\.\d+)\)$", re.M)


def vendor_version(root: Path) -> Version:
    """The version the vendored C sources declare for themselves.

    Read from CMakeLists.txt rather than derived from the submodule's tag: this
    is the code that actually gets compiled, and it resolves in a shallow CI
    checkout where no tags were fetched.
    """
    path = root / VENDOR / "CMakeLists.txt"
    if not path.exists():
        die(f"{VENDOR}/CMakeLists.txt is missing — is the submodule checked out?")
    match = CMAKE_VERSION_RE.search(path.read_text())
    if not match:
        die(f"{VENDOR}/CMakeLists.txt: could not parse project(libosdp VERSION ...)")
    return parse_version(match.group(1))


def vendor_branch(root: Path) -> str:
    """The libosdp branch the submodule follows.

    Load-bearing, not decorative: releases on one line are not reachable from
    another, so this is what makes a release branch step along its own line.
    """
    name = git(["config", "-f", ".gitmodules", "submodule.vendor.branch"],
               root, check=False)
    return name or "master"


def vendor_newest_tag(root: Path) -> str:
    tracked = vendor_branch(root)
    vendor_root = root / VENDOR
    git(["fetch", "-q", "--tags", "origin", tracked], vendor_root)
    tags = git(["tag", "--list", "v*", "--sort=-version:refname",
                "--merged", "FETCH_HEAD"], vendor_root).splitlines()
    if not tags:
        die(f"no libosdp release tags reachable from origin/{tracked}")
    return tags[0].strip()


def checkout_vendor(root: Path, tag: str) -> None:
    vendor_root = root / VENDOR
    git(["fetch", "-q", "--tags", "origin"], vendor_root)
    git(["checkout", "-q", tag], vendor_root)
    git(["submodule", "update", "--init", "--recursive"], vendor_root)


def regenerate_bindings(root: Path) -> None:
    """Rebuild the checked-in bindings against the newly vendored headers.

    CI re-runs this and fails on any diff, so getting it wrong here only delays
    the failure. Bindings are expected to be unchanged across a patch release.
    """
    env = {
        **os.environ,
        "CCACHE_DISABLE": "1",
        "LIBOSDP_SYS_REGENERATE_BINDINGS": "1",
    }
    subprocess.run(["cargo", "clean", "-p", "libosdp-sys"], cwd=root,
                   capture_output=True)
    result = subprocess.run(["cargo", "build", "-p", "libosdp-sys"], cwd=root,
                            env=env, capture_output=True, text=True)
    if result.returncode != 0:
        die(f"failed to build libosdp-sys against the new vendor:\n"
            f"{result.stderr.strip()}")


def refresh_lockfile(root: Path) -> None:
    """Re-resolve Cargo.lock so local builds see the new versions.

    Not part of the release: this workspace publishes libraries and gitignores
    the lock file, so it is a local convenience only.
    """
    result = subprocess.run(["cargo", "metadata", "--format-version", "1"],
                            cwd=root, capture_output=True, text=True)
    if result.returncode != 0:
        die(f"cargo could not re-resolve Cargo.lock:\n{result.stderr.strip()}")


# ---------------------------------------------------------------------------
# changelog
# ---------------------------------------------------------------------------


CHANGELOG_DIR = "changelog"


def release_file(root: Path, tag: str) -> Path:
    return root / CHANGELOG_DIR / f"{tag}.md"


def last_release_tag(root: Path, crate: str) -> str | None:
    tags = git(["tag", "-l", f"{crate}-v*", "--sort=-version:refname"],
               root).splitlines()
    # libosdp-sys-v* also matches nothing else, but libosdp-v* would match
    # libosdp-sys-v* without this filter.
    for tag in tags:
        if changelog_tool.split_tag(tag.strip())[0] == crate:
            return tag.strip()
    return None


RELEASE_SUBJECT_RE = re.compile(r"^\w+ (Release|Prepare|Re-release) ")


def rust_commit_hints(root: Path, crate: str) -> list[str]:
    """Commits in this repo that touched the crate since its last release."""
    base = last_release_tag(root, crate)
    rng = f"{base}..HEAD" if base else "HEAD"
    log = git(["log", rng, "--no-merges", "--format=%h %s", "--", crate],
              root, check=False)
    return [f"- {line}" for line in log.splitlines()
            if line.strip() and not RELEASE_SUBJECT_RE.match(line)]


def vendor_commit_hints(root: Path, old: Version, new_tag: str) -> list[str]:
    """Commits in the C library between the previously vendored tag and new_tag.

    A libosdp-sys release carries no Rust-side change of its own; everything
    worth announcing happened in libosdp, so that is where the hints come from.
    """
    vendor_root = root / VENDOR
    log = git(["log", f"v{old}..{new_tag}", "--no-merges", "--format=%h %s"],
              vendor_root, check=False)
    return [f"- {line}" for line in log.splitlines()
            if line.strip() and not RELEASE_SUBJECT_RE.match(line)]


def scaffold_changelog(root: Path, tag: str, hints: list[str]) -> Path:
    path = release_file(root, tag)
    if path.exists():
        die(f"Release file already exists: {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    crate, version = changelog_tool.split_tag(tag)
    body = changelog_tool.scaffold(crate, version)
    changes = "\n## Changes\n\n" + ("\n".join(hints) if hints else "-") + "\n"
    path.write_text(body + changes)
    return path


def stamp_release_date(path: Path) -> None:
    today = datetime.now(UTC).strftime("%Y-%m-%d")
    text = path.read_text()
    text = _sub_once(path.name, re.compile(r"^release_date: .*$", re.M),
                     f"release_date: {today}", text)
    path.write_text(text)


def validate_changelog(root: Path, path: Path, tag: str) -> None:
    if not path.exists():
        die(f"Missing release file: {path}")
    if "## Changes" in path.read_text():
        die(f"{path.name} still has a '## Changes' section — fold the hints "
            "into Enhancements/Fixes and delete it before publishing.")
    result = subprocess.run(
        [sys.executable, str(root / "scripts/changelog_tool.py"), "validate",
         "--file", str(path), "--expected-tag", tag, "--quiet"], cwd=root)
    if result.returncode != 0:
        die(f"{path.name} failed changelog validation")


# ---------------------------------------------------------------------------
# signature gate
# ---------------------------------------------------------------------------


def _fetch_key(key_url: str) -> bytes:
    try:
        data = urllib.request.urlopen(key_url, timeout=20).read()
    except Exception as exc:  # noqa: BLE001 - any fetch failure blocks the release
        die(f"could not fetch signing key from {key_url}: {exc}")
    if not data.strip():
        die(f"signing key at {key_url} is empty")
    return data


def _fingerprints(colons_output: str) -> set[str]:
    return {line.split(":")[9] for line in colons_output.splitlines()
            if line.startswith("fpr:")}


def check_signing_key_available(key_url: str) -> None:
    """Fail BEFORE any mutation if no local secret key matches the published key,
    so a mis-configured signing key never leaves a half-made release behind."""
    with tempfile.TemporaryDirectory() as home:
        os.chmod(home, 0o700)
        env = {**os.environ, "GNUPGHOME": home}
        imported = subprocess.run(["gpg", "--batch", "--import"],
                                  input=_fetch_key(key_url), env=env,
                                  capture_output=True)
        if imported.returncode != 0:
            die(f"failed to import signing key: {imported.stderr.decode().strip()}")
        published = _fingerprints(subprocess.run(
            ["gpg", "--with-colons", "--list-keys"], env=env,
            capture_output=True, text=True).stdout)
    local = _fingerprints(subprocess.run(
        ["gpg", "--with-colons", "--list-secret-keys"],
        capture_output=True, text=True).stdout)
    if not (published & local):
        die(f"no local secret key matches {key_url}; point git's user.signingkey "
            "at your published key before publishing")


def verify_tag_signature(root: Path, tag: str, key_url: str) -> None:
    """Verify `tag` was signed by a subkey of RELEASE_KEY.

    A throwaway keyring is not sufficient on its own: key_url serves *every* key
    the account publishes. gpg names the certifying primary key in the last
    field of its VALIDSIG status line, so that is asserted instead of a specific
    subkey — a rotated subkey keeps working. GOODSIG is also required, since gpg
    withholds it (emitting EXPKEYSIG or REVKEYSIG) for an expired or revoked
    key. Raises SystemExit (via die) on any mismatch — callers roll back their
    mutations."""
    with tempfile.TemporaryDirectory() as home:
        os.chmod(home, 0o700)
        env = {**os.environ, "GNUPGHOME": home}
        imported = subprocess.run(["gpg", "--batch", "--import"],
                                  input=_fetch_key(key_url), env=env,
                                  capture_output=True)
        if imported.returncode != 0:
            die(f"failed to import signing key: {imported.stderr.decode().strip()}")
        verified = subprocess.run(
            ["git", "-c", "gpg.program=gpg", "verify-tag", "--raw", tag],
            cwd=root, env=env, capture_output=True, text=True,
        )
        status = f"{verified.stdout}{verified.stderr}"
        primary = ""
        for line in status.splitlines():
            if line.startswith("[GNUPG:] VALIDSIG "):
                primary = line.split()[-1]
        if "[GNUPG:] GOODSIG " not in status or primary != RELEASE_KEY:
            die(f"tag {tag} is not signed by the release key:\n{status.strip()}")


def published_on_crates_io(crate: str, version: Version) -> bool:
    url = f"https://crates.io/api/v1/crates/{crate}/{version}"
    request = urllib.request.Request(
        url, headers={"User-Agent": "libosdp-rs-release-gate"})
    try:
        with urllib.request.urlopen(request, timeout=20) as response:
            return response.status == 200
    except urllib.error.HTTPError as exc:
        if exc.code == 404:
            return False
        die(f"crates.io lookup for {crate} {version} failed: {exc}")
    except Exception as exc:  # noqa: BLE001 - an unreachable registry is not a pass
        die(f"crates.io lookup for {crate} {version} failed: {exc}")


# ---------------------------------------------------------------------------
# working tree guards
# ---------------------------------------------------------------------------


def _porcelain_names(root: Path) -> list[str]:
    names = []
    for line in git(["status", "--porcelain", "--untracked-files=no"],
                    root, strip=False).splitlines():
        name = line[3:]
        if " -> " in name:  # rename entries read "R  old -> new"
            name = name.split(" -> ", 1)[1]
        names.append(name.strip('"'))
    return names


def require_clean_tree(root: Path) -> None:
    # Untracked files are ignored: they can't be committed by us and survive the
    # rollback's `reset --hard`. Only tracked modifications would make the commit
    # ambiguous or be lost on rollback.
    if _porcelain_names(root):
        die("working tree has uncommitted changes to tracked files; "
            "commit or stash them before publishing")


def require_release_only_dirt(root: Path, crate: str, path: Path) -> None:
    """Quick-release counterpart of require_clean_tree: the release's own files
    are expected to be uncommitted (that is the whole point), but nothing else
    may be, or the single Release commit would absorb unrelated work."""
    allowed = set(version_files(crate)) | {str(path.relative_to(root))}
    if crate == "libosdp":
        # A libosdp release re-pins libosdp-sys, so its manifest moves too.
        allowed.add(manifest("libosdp"))
    stray = sorted(set(_porcelain_names(root)) - allowed)
    if stray:
        die("working tree has uncommitted changes outside the release: "
            + ", ".join(stray)
            + "\ncommit or stash them before publishing")


# ---------------------------------------------------------------------------
# release invariants shared by publish and the CI gate
# ---------------------------------------------------------------------------


def release_invariants(root: Path, crate: str, version: Version) -> list[str]:
    """Problems that must block a release of `crate` at `version`."""
    problems = []
    if version.prerelease:
        problems.append(
            f"{version} is a pre-release; a release tag must point at a commit "
            "with the marker cleared")
    if crate == "libosdp-sys":
        vendored = vendor_version(root)
        if vendored.core != version.core:
            problems.append(
                f"libosdp-sys {version} has libosdp {vendored} vendored; the "
                "sys crate version must equal the C library version")
    if crate == "libosdp":
        pin = read_pin(root)
        sys_version = read_state(root, "libosdp-sys")
        if not pin.startswith("="):
            problems.append(
                f"libosdp requires libosdp-sys '{pin}', which is not an exact "
                "pin; libosdp-sys ships breaking C API changes in minor bumps, "
                "so a caret range would silently cross an ABI boundary")
        elif pin[1:] != str(sys_version):
            problems.append(
                f"libosdp pins libosdp-sys '{pin}' but the workspace builds "
                f"{sys_version}")
    return problems


def registry_invariants(root: Path, crate: str) -> list[str]:
    """Problems that need the registry to detect. Network-bound, so separate."""
    if crate != "libosdp":
        return []
    pin = read_pin(root)
    if not pin.startswith("="):
        return []
    version = parse_version(pin[1:])
    if published_on_crates_io("libosdp-sys", version):
        return []
    return [f"libosdp-sys {version} is not on crates.io yet; publish it first, "
            "because crates.io rejects an unresolvable '=' pin"]


# ---------------------------------------------------------------------------
# commands
# ---------------------------------------------------------------------------


def cmd_prepare(root: Path, args: argparse.Namespace) -> None:
    crate = args.crate
    state = read_state(root, crate)
    if state.prerelease:
        die(f"{crate} is already in a prepared cycle for v{state.released()}; "
            "run 'publish', or revert the Prepare commit to abandon it")
    head = head_state(root, crate)
    if state != head:
        die(f"a release of {crate} v{state} is already prepared and staged; run "
            f"'publish', or abandon it with:\n"
            f"  git restore --staged --worktree {' '.join(version_files(crate))}")

    if crate == "libosdp-sys":
        target_tag = args.vendor or vendor_newest_tag(root)
        target = parse_version(target_tag.lstrip("v"))
        if target.core <= state.core:
            die(f"refusing to move libosdp-sys {state} back to {target}; "
                f"check 'submodule.vendor.branch' in .gitmodules "
                f"(currently {vendor_branch(root)})")
        checkout_vendor(root, target_tag)
        regenerate_bindings(root)
        vendored = vendor_version(root)
        if vendored.core != target.core:
            die(f"{target_tag} vendors sources that declare v{vendored}")
        hints = (vendor_commit_hints(root, state, target_tag)
                 + rust_commit_hints(root, crate))
    else:
        if args.set:
            target = parse_version(args.set)
            if target.core <= state.core:
                die(f"--set {target} must be greater than the current v{state}")
        else:
            target = state.bumped(args.bump)
        hints = rust_commit_hints(root, crate)

    nxt = dataclasses.replace(target, prerelease=args.cycle)
    tag = f"{crate}-v{nxt.released()}"
    path = release_file(root, tag)
    if path.exists():
        die(f"Release file already exists: {path}")
    if tag_exists(root, tag):
        die(f"tag {tag} already exists")

    write_state(root, crate, nxt)
    if crate == "libosdp":
        write_pin(root, read_state(root, "libosdp-sys"))
    refresh_lockfile(root)
    scaffold_changelog(root, tag, hints)

    staged = version_files(crate) + [str(path.relative_to(root))]
    if crate == "libosdp":
        staged.append(manifest("libosdp"))
    git(["add", *dict.fromkeys(staged)], root)

    rel = path.relative_to(root)
    if args.cycle:
        print(f"Prepared {tag} for an extended cycle "
              f"(marker set: {crate} now reports {nxt}).")
    else:
        print(f"Prepared {tag} (staged, not committed — publish makes the "
              "one and only commit).")
    print(f"  edited + staged: {', '.join(dict.fromkeys(staged))}")
    print(f"  scaffolded:      {rel}")
    if args.cycle:
        print("\nReview, then commit:")
        print(f'  git commit -s -m "Prepare {tag}"')
        print(f"\nEdit {rel} over the cycle; before publishing, fold the "
              "## Changes hints\ninto Enhancements/Fixes and delete that section.")
    else:
        print(f"\nEdit {rel} — fold the ## Changes hints into Enhancements/Fixes\n"
              "and delete that section — then:")
        print(f"  scripts/make_release.py publish -c {crate}")


def cmd_publish(root: Path, args: argparse.Namespace) -> None:
    crate = args.crate
    state = read_state(root, crate)
    head = head_state(root, crate)
    # The bump is uncommitted => this is a quick release and the Release commit
    # carries it; equal versions => a committed Prepare, i.e. a cycle.
    quick = state.core != head.core
    if not quick and not state.prerelease:
        die(f"nothing prepared to publish for {crate}; run 'prepare' first")
    if quick and head.prerelease:
        die(f"HEAD is in a prepared cycle for {crate} v{head.released()} but the "
            f"worktree says v{state}; reconcile the manifests before publishing")

    version = state.released()
    tag = f"{crate}-v{version}"
    path = release_file(root, tag)
    if quick:
        require_release_only_dirt(root, crate, path)
    else:
        require_clean_tree(root)

    if tag_exists(root, tag):
        die(f"tag {tag} already exists")

    # Validate everything and fail fast on a wrong signing key BEFORE touching
    # anything, so a rejected release leaves the tree clean and re-runnable.
    validate_changelog(root, path, tag)
    problems = release_invariants(root, crate, version)
    if not args.no_registry:
        problems += registry_invariants(root, crate)
    if problems:
        die("release invariants failed:\n  - " + "\n  - ".join(problems))
    check_signing_key_available(args.key_url)

    saved = git(["rev-parse", "HEAD"], root)
    stamp_release_date(path)
    if state.prerelease:
        write_state(root, crate, version)
        refresh_lockfile(root)
    staged = version_files(crate) + [str(path.relative_to(root))]
    if crate == "libosdp":
        staged.append(manifest("libosdp"))
    git(["add", *dict.fromkeys(staged)], root)
    git(["commit", "-s", "-m", f"Release {tag}"], root)

    def rollback(reason: str) -> None:
        git(["tag", "-d", tag], root, check=False)
        # A quick release's changelog prose exists only in the index and
        # worktree, so --hard would destroy it. --soft restores exactly the
        # pre-publish state, and publish stays re-runnable: the bump is still
        # uncommitted.
        git(["reset", "--soft" if quick else "--hard", saved], root, check=False)
        kept = ("the release files are left staged" if quick
                else "Release commit undone")
        die(f"{reason} — rolled back (tag {tag} deleted, {kept}). "
            "Fix your signing key and re-run publish.")

    # Signing can fail for reasons no pre-flight check can see -- a locked
    # agent, a pinentry with no terminal to prompt on, an expired subkey. That
    # must roll the Release commit back too, or the branch keeps a release
    # commit that no tag points at.
    try:
        git(["tag", "-s", "-a", tag, "-m", f"Release {tag}"], root)
    except SystemExit:
        rollback("could not sign the tag")

    try:
        verify_tag_signature(root, tag, args.key_url)
    except SystemExit:
        rollback("signature check failed")

    print(f"Released {tag} ({crate} now reports {version}).")
    print(f"Signed and verified against {args.key_url}. Push with:")
    print(f"  git push origin {branch(root)} {tag}")


def cmd_check_release(root: Path, args: argparse.Namespace) -> None:
    """CI gate run against a pushed tag, before anything reaches crates.io."""
    crate, version_str = changelog_tool.split_tag(args.tag)
    if args.crate and args.crate != crate:
        die(f"tag {args.tag} is for {crate}, not {args.crate}")
    tagged = parse_version(version_str.lstrip("v"))
    declared = read_state(root, crate)

    problems = []
    if declared != tagged:
        problems.append(
            f"tag '{args.tag}' does not match {manifest(crate)} version "
            f"'{declared}'; a mismatch publishes '{declared}' under the name "
            f"'{args.tag}'")
    problems += release_invariants(root, crate, declared)
    if args.registry:
        problems += registry_invariants(root, crate)

    path = release_file(root, args.tag)
    if not path.exists():
        problems.append(f"changelog/{args.tag}.md is missing")
    else:
        result = subprocess.run(
            [sys.executable, str(root / "scripts/changelog_tool.py"), "validate",
             "--file", str(path), "--expected-tag", args.tag, "--quiet"],
            cwd=root, capture_output=True, text=True)
        if result.returncode != 0:
            problems.append(f"changelog/{args.tag}.md: "
                            f"{result.stderr.strip() or 'failed validation'}")

    if problems:
        print("release gate: FAILED", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        raise SystemExit(1)
    print(f"release gate: {args.tag} passed")


def cmd_check_staged(root: Path) -> None:
    """Pre-commit gate. Reads the index (== the future commit) so a manual commit
    can't drift a crate version or ship a broken release changelog. Silent + 0
    when the commit touches neither."""
    staged = set(git(["diff", "--cached", "--name-only"], root).splitlines())
    manifests = {c: manifest(c) for c in CRATES}
    touched = {c: m for c, m in manifests.items() if m in staged}
    release_files = sorted(
        f for f in staged
        if f.startswith(f"{CHANGELOG_DIR}/")
        and changelog_tool.RELEASE_FILE_RE.fullmatch(Path(f).name)
    )
    if not touched and not release_files:
        return

    def index_text(path: str) -> str:
        return git(["show", f":{path}"], root)

    for crate, path in touched.items():
        parse_manifest_version(path, index_text(path))
    if manifests["libosdp"] in staged:
        pin = SYS_DEP_RE.search(index_text(manifests["libosdp"]))
        if pin and not pin.group(1).startswith("="):
            die(f"pre-commit: libosdp requires libosdp-sys '{pin.group(1)}', "
                "which is not an exact pin")

    for rel in release_files:
        name = Path(rel).name
        file_crate, file_version = changelog_tool.split_tag(name[:-len(".md")])
        content = index_text(rel)
        # The in-flight file of a prepared cycle is a WIP stub by design; once
        # the marker is cleared (publish) it must be finalized and valid.
        current = read_state(root, file_crate)
        if current.prerelease and f"v{current.released()}" == file_version:
            continue
        if "## Changes" in content:
            die(f"pre-commit: {rel} still has a ## Changes section — fold it "
                "into Enhancements/Fixes and delete it before publishing")
        result = subprocess.run(
            [sys.executable, str(root / "scripts/changelog_tool.py"), "validate",
             "--stdin", "--expected-tag", f"{file_crate}-{file_version}",
             "--quiet"], input=content, text=True, cwd=root)
        if result.returncode != 0:
            die(f"pre-commit: {rel} failed changelog validation")


def cmd_check_changelog(root: Path) -> None:
    """CI gate over the whole changelog directory. Only *published* releases are
    held to the finalized format; the in-flight file of a prepared cycle is a WIP
    stub by design until publish clears the marker, so it is skipped."""
    files = changelog_tool.release_files_in_dir(root / CHANGELOG_DIR)
    in_flight = set()
    for crate in CRATES:
        state = read_state(root, crate)
        if state.prerelease:
            in_flight.add(f"{crate}-v{state.released()}")

    checked, skipped = 0, []
    for path in files:
        tag = path.name[:-len(".md")]
        if tag in in_flight:
            skipped.append(tag)
            continue
        validate_changelog(root, path, tag)
        checked += 1

    if skipped:
        print(f"{checked} published release file(s) valid; skipped "
              f"{', '.join(sorted(skipped))} (prepared, not published)")
    else:
        print(f"{checked} published release file(s) valid")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="libosdp-rs two-phase release helper",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    sub = parser.add_subparsers(dest="command", required=True)

    def add_crate(target: argparse.ArgumentParser, required: bool = True) -> None:
        target.add_argument("-c", "--crate", choices=CRATES, required=required,
                            help="crate to act on")

    prepare = sub.add_parser("prepare", help="stage the next release")
    add_crate(prepare)
    bump = prepare.add_mutually_exclusive_group()
    bump.add_argument("--major", dest="bump", action="store_const", const="major")
    bump.add_argument("--minor", dest="bump", action="store_const", const="minor")
    bump.add_argument("--patch", dest="bump", action="store_const", const="patch")
    prepare.add_argument("--set", help="set an explicit X.Y.Z instead of bumping")
    prepare.add_argument("--vendor", metavar="vX.Y.Z",
                         help="libosdp-sys only: vendor this libosdp tag "
                              "(default: newest on the branch .gitmodules tracks)")
    prepare.add_argument("--cycle", action="store_true",
                         help="extended release cycle: set the pre-release "
                              "marker and expect a 'Prepare <tag>' commit "
                              "(default: a single-commit release)")
    prepare.set_defaults(bump="patch")

    publish = sub.add_parser("publish", help="finalize + sign the prepared release")
    add_crate(publish)
    publish.add_argument("--no-registry", action="store_true",
                         help="skip the crates.io reachability check")
    publish.add_argument("--key-url", default=DEFAULT_KEY_URL,
                         help=f"public key the tag signature must match "
                              f"(default {DEFAULT_KEY_URL})")

    check = sub.add_parser("check-release",
                           help="CI gate: verify a tag against the manifests "
                                "and its changelog")
    add_crate(check, required=False)
    check.add_argument("--tag", required=True, metavar="CRATE-vX.Y.Z")
    check.add_argument("--registry", action="store_true",
                       help="also verify pinned dependencies resolve on crates.io")

    sub.add_parser("check-staged",
                   help="pre-commit gate: verify staged manifests and a staged "
                        "release changelog are consistent")

    sub.add_parser("check-changelog",
                   help="CI gate: validate every published release file, "
                        "skipping the in-flight file of a prepared cycle")

    args = parser.parse_args()
    if args.command == "prepare":
        if args.set and args.bump != "patch":
            parser.error("--set cannot be combined with --major/--minor/--patch")
        if args.crate == "libosdp-sys" and (args.set or args.bump != "patch"):
            parser.error("libosdp-sys is vendored, not bumped; use --vendor")
        if args.crate != "libosdp-sys" and args.vendor:
            parser.error("--vendor applies only to libosdp-sys")
    return args


def main() -> None:
    args = parse_args()
    root = repo_root()
    if args.command == "prepare":
        cmd_prepare(root, args)
    elif args.command == "publish":
        cmd_publish(root, args)
    elif args.command == "check-release":
        cmd_check_release(root, args)
    elif args.command == "check-staged":
        cmd_check_staged(root)
    elif args.command == "check-changelog":
        cmd_check_changelog(root)


if __name__ == "__main__":
    main()
