#!/usr/bin/env python3
#
#  Copyright (c) 2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
#
#  SPDX-License-Identifier: Apache-2.0
#
"""Changelog tooling for the libosdp-rs workspace.

One file per release under changelog/, named exactly after its git tag:

    changelog/libosdp-sys-v3.2.3.md
    changelog/libosdp-v0.2.2.md

Each carries front matter naming the crate, date and version, followed by a
free-form subject block and one or more bulleted sections:

    ---
    release_crate: libosdp-sys
    release_date: 2026-07-09
    release_version: v3.2.3
    ---

    Vendors libosdp v3.2.3.

    ## Fixes

    - python: Clear stale AttributeError from optional channel "id" lookup
"""

import argparse
import dataclasses
import json
import pathlib
import re
import sys
from datetime import UTC, datetime

# Longest first: libosdp-sys must win over libosdp, or "libosdp-sys-v3.2.3.md"
# parses as crate "libosdp" at version "sys-v3.2.3".
CRATES = ["libosdp-sys", "libosdp", "osdpctl"]

VERSION_RE = re.compile(r"^[0-9A-Za-z][0-9A-Za-z.+-]*$")
RELEASE_FILE_RE = re.compile(
    r"^(" + "|".join(CRATES) + r")-(v[0-9A-Za-z][0-9A-Za-z.+-]*)\.md$"
)
MARKDOWN_HEADING_RE = re.compile(r"^## ([A-Z][A-Za-z ]+)$")

FRONT_MATTER_KEYS = {"release_crate", "release_date", "release_version"}


@dataclasses.dataclass
class ReleaseEntry:
    crate: str
    version: str
    date: str
    subject: str
    sections: list[tuple[str, list[str]]]

    @property
    def tag(self) -> str:
        return f"{self.crate}-{self.version}"

    @property
    def path_name(self) -> str:
        return f"{self.tag}.md"


def die(message: str) -> None:
    print(message, file=sys.stderr)
    raise SystemExit(1)


def normalize_version(raw: str) -> str:
    version = raw.strip()
    if version.startswith("v"):
        version = version[1:]
    version = re.sub(r"\s*-\s*", "-", version)
    version = re.sub(r"\s+", "-", version)
    version = re.sub(r"-{2,}", "-", version)
    version = version.lower()
    if not VERSION_RE.fullmatch(version):
        die(f"Invalid release version: {raw}")
    return f"v{version}"


def split_tag(tag: str) -> tuple[str, str]:
    """Split a release tag into (crate, vX.Y.Z)."""
    for crate in CRATES:
        prefix = f"{crate}-v"
        if tag.startswith(prefix):
            return crate, normalize_version(tag[len(crate) + 1:])
    die(f"Not a release tag for a known crate: {tag}")


def human_date_to_iso(value: str) -> str:
    value = value.strip()
    if not value:
        die("Missing release date")
    if re.fullmatch(r"\d{4}-\d{2}-\d{2}", value):
        return value
    for fmt in ("%d %B %Y", "%d %b %Y"):
        try:
            return datetime.strptime(value, fmt).strftime("%Y-%m-%d")
        except ValueError:
            pass
    die(f"Unsupported release date format: {value}")


def split_front_matter(text: str) -> tuple[dict[str, str], str]:
    if not text.startswith("---\n"):
        die("Release file is missing markdown front matter")
    end = text.find("\n---\n", 4)
    if end < 0:
        die("Release file front matter is not terminated")

    data: dict[str, str] = {}
    for line in text[4:end].splitlines():
        if not line.strip():
            continue
        if ":" not in line:
            die(f"Invalid front matter line: {line}")
        key, value = line.split(":", 1)
        data[key.strip()] = value.strip().strip('"')
    return data, text[end + len("\n---\n"):].lstrip("\n")


def parse_markdown_release(
    text: str, expected_tag: str | None = None
) -> ReleaseEntry:
    if "## TODO" in text:
        die("Release file still contains TODO markers")
    meta, body = split_front_matter(text)
    missing = sorted(FRONT_MATTER_KEYS - set(meta))
    extra = sorted(set(meta) - FRONT_MATTER_KEYS)
    if missing:
        die(f"Release file front matter missing keys: {', '.join(missing)}")
    if extra:
        die(f"Release file front matter has unsupported keys: {', '.join(extra)}")

    crate = meta["release_crate"].strip()
    if crate not in CRATES:
        die(f"Unknown release_crate: {crate}")
    version = normalize_version(meta["release_version"])
    if expected_tag:
        want_crate, want_version = split_tag(expected_tag)
        if (crate, version) != (want_crate, want_version):
            die(
                f"Release mismatch: expected {want_crate}-{want_version}, "
                f"found {crate}-{version}"
            )
    date = human_date_to_iso(meta["release_date"])

    lines = body.splitlines()
    i = 0
    while i < len(lines) and not lines[i].strip():
        i += 1
    subject_lines = []
    while i < len(lines):
        if MARKDOWN_HEADING_RE.fullmatch(lines[i].strip()):
            break
        subject_lines.append(lines[i])
        i += 1

    subject = "\n".join(subject_lines).strip()
    if not subject:
        die(f"Release {crate}-{version} is missing its subject block")

    sections: list[tuple[str, list[str]]] = []
    seen_titles: set[str] = set()
    while i < len(lines):
        while i < len(lines) and not lines[i].strip():
            i += 1
        if i >= len(lines):
            break
        match = MARKDOWN_HEADING_RE.fullmatch(lines[i].strip())
        if not match:
            die(f"Release {crate}-{version} has invalid section heading: {lines[i]}")
        title = match.group(1)
        if title in seen_titles:
            die(f"Release {crate}-{version} repeats section {title}")
        seen_titles.add(title)
        i += 1

        items: list[str] = []
        while i < len(lines):
            stripped = lines[i].strip()
            if not stripped:
                i += 1
                if items:
                    break
                continue
            if MARKDOWN_HEADING_RE.fullmatch(stripped):
                break
            bullet = re.fullmatch(r"-\s+(.+)", stripped)
            if bullet:
                items.append(bullet.group(1).strip())
                i += 1
                continue
            # A wrapped bullet: an indented line continuing the one above.
            # Joined into a single item so rendered notes stay one bullet.
            if items and lines[i][:1].isspace():
                items[-1] += f" {stripped}"
                i += 1
                continue
            die(
                f"Release {crate}-{version} has a non-bullet line in "
                f"section {title}: {lines[i]}"
            )
        if not items:
            die(
                f"Release {crate}-{version} section {title} must contain at "
                "least one bullet"
            )
        sections.append((title, items))

    if not sections:
        die(f"Release {crate}-{version} must include at least one section")

    return ReleaseEntry(
        crate=crate, version=version, date=date, subject=subject, sections=sections
    )


def render_release(entry: ReleaseEntry) -> str:
    parts = [
        "---",
        f"release_crate: {entry.crate}",
        f"release_date: {entry.date}",
        f"release_version: {entry.version}",
        "---",
        "",
        entry.subject.strip(),
        "",
    ]
    for title, items in entry.sections:
        parts.append(f"## {title}")
        parts.append("")
        for item in items:
            parts.append(f"- {item}")
        parts.append("")
    return "\n".join(parts).rstrip() + "\n"


def release_files_in_dir(directory: pathlib.Path) -> list[pathlib.Path]:
    if not directory.exists():
        return []
    if not directory.is_dir():
        die(f"Expected a changelog directory: {directory}")
    paths = [
        path
        for path in directory.iterdir()
        if path.is_file() and RELEASE_FILE_RE.fullmatch(path.name)
    ]
    return sorted(paths, key=lambda path: path.name, reverse=True)


def validate_release_file(
    path: pathlib.Path, expected_tag: str | None = None, quiet: bool = False
) -> ReleaseEntry:
    match = RELEASE_FILE_RE.fullmatch(path.name)
    if not match:
        die(f"Invalid release file name: {path.name}")
    file_tag = f"{match.group(1)}-{normalize_version(match.group(2))}"
    entry = parse_markdown_release(path.read_text(encoding="utf-8"),
                                   expected_tag or file_tag)
    if entry.tag != file_tag:
        die(f"Release identity in {path.name} does not match the file name")
    if not quiet:
        print(json.dumps(dataclasses.asdict(entry), indent=2))
    return entry


def scaffold(crate: str, version: str, date: str | None = None) -> str:
    """Render an empty release file for crate at version, with TODO markers."""
    return render_release(
        ReleaseEntry(
            crate=crate,
            version=normalize_version(version),
            date=date or datetime.now(UTC).strftime("%Y-%m-%d"),
            subject="Release subject ## TODO",
            sections=[("Enhancements", ["## TODO"]), ("Fixes", ["## TODO"])],
        )
    )


def command_init(args: argparse.Namespace) -> None:
    crate, version = split_tag(args.tag)
    output_dir = pathlib.Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)
    output_path = output_dir / f"{crate}-{version}.md"
    if output_path.exists():
        die(f"Release file already exists: {output_path}")
    output_path.write_text(scaffold(crate, version, args.date), encoding="utf-8")
    print(output_path)


def command_validate(args: argparse.Namespace) -> None:
    if args.stdin:
        entry = parse_markdown_release(sys.stdin.read(), args.expected_tag)
        if not args.quiet:
            print(json.dumps(dataclasses.asdict(entry), indent=2))
        return

    if args.file:
        validate_release_file(
            pathlib.Path(args.file), args.expected_tag, quiet=args.quiet
        )
        return

    if args.dir:
        files = release_files_in_dir(pathlib.Path(args.dir))
        if not files:
            die(f"No release files found in {args.dir}")
        entries = [validate_release_file(path, quiet=True) for path in files]
        if not args.quiet:
            print(json.dumps([dataclasses.asdict(e) for e in entries], indent=2))
        return

    die("Expected one of --file, --dir, or --stdin")


def command_notes(args: argparse.Namespace) -> None:
    if args.stdin:
        text = sys.stdin.read()
    elif args.file:
        path = pathlib.Path(args.file)
        validate_release_file(path, quiet=True)
        text = path.read_text(encoding="utf-8")
    else:
        die("A release file path is required")

    # Emit the body exactly as written, front matter aside. Re-rendering from
    # the parsed entry would join wrapped bullets onto one line, so a release
    # published from here would not match the file it came from.
    if args.stdin:
        parse_markdown_release(text)
    content = split_front_matter(text)[1].rstrip() + "\n"

    if args.output:
        pathlib.Path(args.output).write_text(content, encoding="utf-8")
    else:
        sys.stdout.write(content)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="libosdp-rs changelog tooling")
    sub = parser.add_subparsers(dest="command", required=True)

    init_cmd = sub.add_parser("init", help="Create a new release changelog template")
    init_cmd.add_argument("--tag", required=True, metavar="CRATE-vX.Y.Z")
    init_cmd.add_argument("--output-dir", default="changelog")
    init_cmd.add_argument("--date", default=None)

    validate = sub.add_parser("validate", help="Validate release files")
    validate.add_argument("--file")
    validate.add_argument("--dir")
    validate.add_argument("--stdin", action="store_true")
    validate.add_argument("--expected-tag", metavar="CRATE-vX.Y.Z")
    validate.add_argument("--quiet", action="store_true")

    notes = sub.add_parser("notes", help="Extract release notes from a release file")
    notes.add_argument("--file")
    notes.add_argument("--stdin", action="store_true")
    notes.add_argument("--output")

    return parser.parse_args()


def main() -> None:
    args = parse_args()
    if args.command == "init":
        command_init(args)
    elif args.command == "validate":
        command_validate(args)
    elif args.command == "notes":
        command_notes(args)


if __name__ == "__main__":
    main()
