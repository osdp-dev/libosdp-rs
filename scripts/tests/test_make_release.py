#
#  Copyright (c) 2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
#
#  SPDX-License-Identifier: Apache-2.0
#
"""Unit tests for the pure logic in make_release.py.

Run with: python3 -m unittest discover -s scripts/tests -v
"""

import contextlib
import io
import os
import subprocess
import sys
import tempfile
import unittest
import unittest.mock
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import make_release as mr  # noqa: E402


class TestSysTargetFromRef(unittest.TestCase):
    """A vendor ref only fixes the crate version if it names one."""

    def test_release_tag_names_its_version(self):
        self.assertEqual(mr.sys_target_from_ref("v3.2.7"), mr.Version(3, 2, 7))

    def test_bare_version_is_accepted(self):
        self.assertEqual(mr.sys_target_from_ref("4.0.0"), mr.Version(4, 0, 0))

    def test_commit_sha_names_no_version(self):
        self.assertIsNone(mr.sys_target_from_ref("91f4c0a"))

    def test_branch_name_names_no_version(self):
        self.assertIsNone(mr.sys_target_from_ref("master"))

    def test_surrounding_whitespace_is_ignored(self):
        self.assertEqual(mr.sys_target_from_ref(" v3.2.7 "), mr.Version(3, 2, 7))


class TestPinUpkeep(unittest.TestCase):
    """libosdp's '=' pin has to survive a sys-side move mid-cycle."""

    @staticmethod
    def _workspace(tmp: str, sys_version: str, libosdp_version: str,
                   pin: str) -> Path:
        root = Path(tmp)
        (root / "libosdp-sys").mkdir()
        (root / "libosdp-sys" / "Cargo.toml").write_text(
            f'[package]\nname = "libosdp-sys"\nversion = "{sys_version}"\n')
        (root / "libosdp").mkdir()
        (root / "libosdp" / "Cargo.toml").write_text(
            f'[package]\nname = "libosdp"\nversion = "{libosdp_version}"\n'
            f'\n[dependencies]\nlibosdp-sys = "{pin}"\n')
        return root

    def test_write_pin_round_trips_a_prerelease(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = self._workspace(tmp, "4.0.0-dev", "1.0.0-dev", "=3.2.7")
            mr.write_pin(root, mr.Version(4, 0, 0, prerelease=True))
            self.assertEqual(mr.read_pin(root), "=4.0.0-dev")

    def test_pin_is_refreshed_while_libosdp_is_mid_cycle(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = self._workspace(tmp, "4.0.0-dev", "1.0.0-dev", "=3.2.7")
            self.assertTrue(mr.refresh_pin_if_mid_cycle(root))
            self.assertEqual(mr.read_pin(root), "=4.0.0-dev")

    def test_pin_is_left_alone_when_libosdp_is_released(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = self._workspace(tmp, "3.2.7", "0.2.2", "=3.2.7")
            self.assertFalse(mr.refresh_pin_if_mid_cycle(root))
            self.assertEqual(mr.read_pin(root), "=3.2.7")


def _git(args: list[str], cwd: Path) -> str:
    return subprocess.run(["git", *args], cwd=cwd, check=True,
                          capture_output=True, text=True).stdout.strip()


def _init(path: Path) -> None:
    path.mkdir(parents=True, exist_ok=True)
    _git(["-c", "init.defaultBranch=master", "init", "-q", "."], path)
    _identify(path)


def _identify(path: Path) -> None:
    _git(["config", "user.email", "test@example.invalid"], path)
    _git(["config", "user.name", "Test"], path)
    _git(["config", "commit.gpgsign", "false"], path)


def _commit(path: Path, text: str) -> str:
    (path / "CMakeLists.txt").write_text(text)
    _git(["add", "CMakeLists.txt"], path)
    _git(["commit", "-q", "-m", text], path)
    return _git(["rev-parse", "HEAD"], path)


class TestCheckoutVendorResolution(unittest.TestCase):
    """checkout_vendor must resolve against origin, never a stale local branch.

    The submodule is cloned while origin/master is still old, then upstream
    moves on -- exactly the shape that silently vendored a nine-month-old tree
    when a bare `git checkout <branch>` picked refs/heads/master.
    """

    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        base = Path(tmp.name)
        # make_release calls git through os.environ, so the developer's own
        # config (signing keys, hooks, default branch) has to be shut out here
        # rather than per invocation.
        isolated = unittest.mock.patch.dict(
            os.environ, {"GIT_CONFIG_GLOBAL": os.devnull,
                         "GIT_CONFIG_SYSTEM": os.devnull})
        isolated.start()
        self.addCleanup(isolated.stop)

        self.upstream = base / "upstream"
        _init(self.upstream)
        self.old = _commit(self.upstream, "project(libosdp VERSION 3.1.0)")

        self.root = base / "work"
        self.vendor = self.root / mr.VENDOR
        self.vendor.parent.mkdir(parents=True)
        _git(["clone", "-q", str(self.upstream), str(self.vendor)], base)
        _identify(self.vendor)

        self.new = _commit(self.upstream, "project(libosdp VERSION 4.0.0)")
        # Same name as a tag on the older commit, to pin down the precedence.
        _git(["branch", "topic", "master"], self.upstream)
        _git(["tag", "topic", self.old], self.upstream)

    def _head(self) -> str:
        return _git(["rev-parse", "HEAD"], self.vendor)

    def test_branch_resolves_to_origin_not_stale_local_ref(self):
        self.assertEqual(_git(["rev-parse", "refs/heads/master"], self.vendor),
                         self.old)
        self.assertEqual(mr.checkout_vendor(self.root, "master"), self.new)
        self.assertEqual(self._head(), self.new)
        # The stale local branch is left behind, not checked out or advanced.
        self.assertEqual(_git(["rev-parse", "refs/heads/master"], self.vendor),
                         self.old)

    def test_checkout_detaches(self):
        mr.checkout_vendor(self.root, "master")
        # A gitlink records a commit, and leaving a branch checked out is how
        # the stale ref arose in the first place.
        attached = subprocess.run(["git", "symbolic-ref", "-q", "HEAD"],
                                  cwd=self.vendor, capture_output=True)
        self.assertNotEqual(attached.returncode, 0)

    def test_repeated_vendor_of_a_branch_is_not_a_no_op(self):
        mr.checkout_vendor(self.root, "master")
        _commit(self.upstream, "project(libosdp VERSION 4.1.0)")
        newest = _git(["rev-parse", "master"], self.upstream)
        self.assertEqual(mr.checkout_vendor(self.root, "master"), newest)

    def test_a_tag_wins_over_a_branch_of_the_same_name(self):
        self.assertEqual(mr.checkout_vendor(self.root, "topic"), self.old)

    def test_a_raw_commit_is_accepted(self):
        self.assertEqual(mr.checkout_vendor(self.root, self.new), self.new)

    def test_an_unknown_ref_is_rejected(self):
        with contextlib.redirect_stderr(io.StringIO()) as err:
            with self.assertRaises(SystemExit):
                mr.checkout_vendor(self.root, "no-such-ref")
        self.assertIn("no-such-ref", err.getvalue())


if __name__ == "__main__":
    unittest.main()
