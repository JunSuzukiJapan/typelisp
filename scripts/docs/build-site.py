#!/usr/bin/env python3
"""Builds the documentation site: every released version, in every language.

    scripts/docs/build-site.py [--out DIR] [--only-tag vX.Y.Z]...

The site holds one copy of the documentation per MAJOR.MINOR, taken from the
newest tag of that line (v0.1.0 and v0.1.1 make one version, 0.1, built from
v0.1.1), so a fix released as a patch replaces the page it fixes. The working
tree's docs/ is the next release and is not published until it is tagged.

Layout of the output, which is what GitHub Pages serves:

    <lang>/<MAJOR.MINOR>/   one build
    <lang>/latest/          a copy of the newest version that has the language
    <lang>/versions.json    what the version selector lists
    index.html              redirects to a language's latest

A tag is read with `git archive`, never checked out, so every version is built
from its own files and from this script's rules. The set of languages is the
set of docs/<lang>/ directories of that tag (docs/dev is not documentation for
users and is left out); the first releases have Japanese only.

The navigation is not written down anywhere: it is the order of the links in
the tag's docs/<lang>/README.md, grouped by its `##` headings, which is what
that index is for. Links from the documents to files outside docs/ (the
READMEs of the repository and of the editors) go to GitHub at the same tag.

Needs scripts/docs/requirements.txt installed.
"""

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path

REPO_URL = "https://github.com/JunSuzukiJapan/typelisp"
SITE_URL = "https://junsuzukijapan.github.io/typelisp"
FIRST_VERSION = (0, 1)

# docs/<dir> -> (name shown in the language selector, Material's UI language).
LANGUAGES = {
    "ar": ("العربية", "ar"),
    "de": ("Deutsch", "de"),
    "en": ("English", "en"),
    "es": ("Español", "es"),
    "fr": ("Français", "fr"),
    "hi": ("हिन्दी", "hi"),
    "id": ("Bahasa Indonesia", "id"),
    "it": ("Italiano", "it"),
    "ja": ("日本語", "ja"),
    "ko": ("한국어", "ko"),
    "nl": ("Nederlands", "nl"),
    "pl": ("Polski", "pl"),
    "pt-BR": ("Português (Brasil)", "pt-BR"),
    "ru": ("Русский", "ru"),
    "sv": ("Svenska", "sv"),
    "th": ("ไทย", "th"),
    "tr": ("Türkçe", "tr"),
    "uk": ("Українська", "uk"),
    "vi": ("Tiếng Việt", "vi"),
    "zh-CN": ("简体中文", "zh"),
    "zh-TW": ("繁體中文", "zh-TW"),
}


def git(*args):
    return subprocess.run(
        ["git", *args], check=True, capture_output=True, text=True
    ).stdout


def released_versions(only_tags):
    """MAJOR.MINOR -> the newest tag of that line, oldest line first."""
    newest = {}
    for tag in git("tag", "--list", "v*").split():
        m = re.fullmatch(r"v(\d+)\.(\d+)\.(\d+)", tag)
        if not m:
            continue
        major, minor, patch = map(int, m.groups())
        if (major, minor) < FIRST_VERSION:
            continue
        if only_tags and tag not in only_tags:
            continue
        line = (major, minor)
        if line not in newest or newest[line][0] < patch:
            newest[line] = (patch, tag)
    return {f"{ma}.{mi}": newest[(ma, mi)][1] for ma, mi in sorted(newest)}


def extract_docs(tag, dest):
    """docs/ of `tag` into dest, without docs/dev."""
    Path(dest).mkdir(parents=True)
    archive = Path(dest) / "docs.tar"
    with open(archive, "wb") as f:
        subprocess.run(
            ["git", "archive", "--format=tar", tag, "docs"], check=True, stdout=f
        )
    with tarfile.open(archive) as tar:
        members = [m for m in tar.getmembers() if not m.name.startswith("docs/dev")]
        tar.extractall(dest, members=members, filter="data")
    archive.unlink()
    return Path(dest) / "docs"


LINK = re.compile(r"(\]\()([^)\s#]*)((?:#[^)\s]*)?\))")


def rewrite_external_links(root, tag):
    """Links that leave docs/<lang>/ go to GitHub at `tag`."""
    root = root.resolve()
    for md in root.rglob("*.md"):
        text = md.read_text(encoding="utf-8")

        def fix(m):
            target = m.group(2)
            if not target or re.match(r"[a-zA-Z][a-zA-Z0-9+.-]*:", target):
                return m.group(0)
            resolved = (md.parent / target).resolve()
            if resolved == root or root in resolved.parents:
                return m.group(0)
            # `root` is <tmp>/docs/<lang>; the repository is two levels up.
            repo_root = root.parent.parent.resolve()
            try:
                rel = resolved.relative_to(repo_root)
            except ValueError:
                return m.group(0)
            return f"{m.group(1)}{REPO_URL}/blob/{tag}/{rel.as_posix()}{m.group(3)}"

        new = LINK.sub(fix, text)
        if new != text:
            md.write_text(new, encoding="utf-8")


def navigation(root):
    """The nav of one language, from the links of its README.md."""
    nav = [{"Home": "README.md"}]
    section, entries = None, []

    def flush():
        if section and entries:
            nav.append({section: list(entries)})

    seen = {"README.md"}
    for line in (root / "README.md").read_text(encoding="utf-8").splitlines():
        heading = re.match(r"##\s+(.*\S)", line)
        if heading:
            flush()
            section, entries = heading.group(1), []
            continue
        m = re.match(r"\s*[-*]\s+\[([^\]]+)\]\(([^)\s#]+\.md)(?:#[^)]*)?\)", line)
        if not m or section is None:
            continue
        title, path = m.group(1).replace("`", ""), m.group(2)
        if not (root / path).is_file() or path in seen:
            continue
        seen.add(path)
        if path.endswith("/README.md"):
            # A directory index: its other pages hang under it.
            directory = (root / path).parent
            children = [path]
            for sibling in sorted(directory.glob("*.md")):
                rel = sibling.relative_to(root).as_posix()
                if rel not in seen:
                    seen.add(rel)
                    children.append(rel)
            entries.append({title: children})
        else:
            entries.append({title: path})
    flush()
    return nav


def yaml_scalar(value):
    return json.dumps(value, ensure_ascii=False)


def yaml_nav(items, indent=0):
    pad = "  " * indent
    out = []
    for item in items:
        (title, value), = item.items()
        if isinstance(value, str):
            out.append(f"{pad}- {yaml_scalar(title)}: {yaml_scalar(value)}")
        else:
            out.append(f"{pad}- {yaml_scalar(title)}:")
            if all(isinstance(v, str) for v in value):
                out.extend(f"{pad}    - {yaml_scalar(v)}" for v in value)
            else:
                out.extend(yaml_nav(value, indent + 2))
    return out


def mkdocs_config(lang, version, langs_of_version, root, site_dir, nav):
    label, ui_lang = LANGUAGES[lang]
    alternates = "\n".join(
        f"    - name: {yaml_scalar(LANGUAGES[l][0])}\n"
        f"      link: {yaml_scalar(f'{SITE_URL}/{l}/{version}/')}\n"
        f"      lang: {yaml_scalar(l)}"
        for l in langs_of_version
    )
    return f"""site_name: typelisp
site_url: {yaml_scalar(f'{SITE_URL}/{lang}/{version}/')}
repo_url: {yaml_scalar(REPO_URL)}
docs_dir: {yaml_scalar(str(root))}
site_dir: {yaml_scalar(str(site_dir))}
use_directory_urls: true
theme:
  name: material
  language: {yaml_scalar(ui_lang)}
  features:
    - navigation.sections
    - navigation.indexes
    - navigation.top
    - search.highlight
    - content.code.copy
plugins:
  - search
markdown_extensions:
  - tables
  - toc:
      permalink: false
extra:
  version:
    provider: mike
  alternate:
{alternates}
nav:
""" + "\n".join(yaml_nav(nav)) + "\n"


def build_one(lang, version, langs_of_version, root, tag, out_dir, work):
    rewrite_external_links(root, tag)
    config = work / f"mkdocs-{lang}.yml"
    config.write_text(
        mkdocs_config(lang, version, langs_of_version, root, out_dir, navigation(root)),
        encoding="utf-8",
    )
    subprocess.run(
        [sys.executable, "-m", "mkdocs", "build", "--quiet", "-f", str(config)],
        check=True,
        # Material's banner about MkDocs 2.0, which this does not use (the
        # version is pinned in requirements.txt), once per build.
        env={**os.environ, "NO_MKDOCS_2_WARNING": "true"},
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--out", default="target/docs-site")
    parser.add_argument(
        "--only-tag", action="append", default=[], help="build only this tag"
    )
    args = parser.parse_args()

    versions = released_versions(set(args.only_tag))
    if not versions:
        sys.exit("error: no release tag to build the documentation from")
    out = Path(args.out).resolve()
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)

    # lang -> [versions that have it, oldest first]
    available = {}
    with tempfile.TemporaryDirectory() as tmp:
        for version, tag in versions.items():
            docs = extract_docs(tag, Path(tmp) / version.replace(".", "_"))
            langs = sorted(
                d.name for d in docs.iterdir() if d.is_dir() and (d / "README.md").is_file()
            )
            unknown = [l for l in langs if l not in LANGUAGES]
            if unknown:
                sys.exit(f"error: {tag}: docs/{unknown[0]} is not in LANGUAGES")
            for lang in langs:
                print(f"{tag} -> {lang}/{version}", flush=True)
                build_one(
                    lang, version, langs, docs / lang, tag,
                    out / lang / version, Path(tmp),
                )
                available.setdefault(lang, []).append(version)

    for lang, have in available.items():
        newest = have[-1]
        shutil.copytree(out / lang / newest, out / lang / "latest")
        listing = [
            {"version": v, "title": v, "aliases": ["latest"] if v == newest else []}
            for v in reversed(have)
        ]
        (out / lang / "versions.json").write_text(
            json.dumps(listing, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
        )

    home = "en" if "en" in available else sorted(available)[0]
    (out / "index.html").write_text(
        '<!doctype html><meta charset="utf-8">'
        f'<meta http-equiv="refresh" content="0; url={home}/latest/">'
        f'<link rel="canonical" href="{SITE_URL}/{home}/latest/">'
        f'<a href="{home}/latest/">typelisp documentation</a>\n',
        encoding="utf-8",
    )
    (out / ".nojekyll").write_text("", encoding="utf-8")


if __name__ == "__main__":
    main()
