#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Post release <version> to the project's Telegram channel, from the version's section of
CHANGELOG.ru.md only: a title, a line saying how to update, each change's headline under its
heading (Новое, Изменено, Исправлено), the #обновление tag, and two buttons, Скачать (the
release page) and Все изменения (CHANGELOG.ru.md at the release's tag). A headline is cut the way
the app's About page cuts it (apps/tanki-studio/src/changelog.rs): up to the first ":" or ". ",
at most 120 characters, with its bold lead kept bold and without a closing aside in brackets. A version without a Russian section is not
posted. The Telegram workflow runs it when a release is published (docs/releasing.md).

    packaging/telegram-post.py 0.5.0 --dry-run    # print the post, send nothing
    packaging/telegram-post.py 0.5.0 --json       # print the Bot API request, send nothing

--changelog <file> reads another CHANGELOG.ru.md, --url <page> links another release page.
Without --dry-run or --json it sends the post with curl to the chat in TELEGRAM_CHAT_ID, as the
bot in TELEGRAM_BOT_TOKEN; when either is empty it warns and exits 0. The token reaches curl
through its standard input, never its arguments, and is never printed.
"""

import html
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REPOSITORY = "YuraZakaryan/tanki-skin-studio"
# Telegram's limit for one message, in characters (UTF-16 code units). It is counted here with
# the tags, so the text Telegram counts is shorter.
LIMIT = 4096
# The longest headline, in characters, as in changelog.rs.
HEADLINE = 120
VERSION = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$")
# The changelog's headings as the post shows them; any other heading is shown as it is.
HEADINGS = {
    "Добавлено": "✨ Новое",
    "Изменено": "🔄 Изменено",
    "Исправлено": "🐞 Исправлено",
    "Удалено": "🗑 Убрано",
    "Безопасность": "🔒 Безопасность",
}


def section(path, version):
    """The lines of <path> after "## [<version>]" up to the next "## [" heading, or None."""
    if not path.is_file():
        return None
    lines, found = [], False
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.startswith("## ["):
            if found:
                break
            found = line.startswith(f"## [{version}]")
            continue
        if found and not re.match(r"^\[[^]]+\]: ", line):
            lines.append(line)
    return lines if found else None


def groups(lines):
    """[(heading, [change, ...]), ...]: each change's text joined from its continuation lines."""
    result, note = [], None

    def flush():
        nonlocal note
        if note is not None:
            if not result:
                result.append(("", []))
            result[-1][1].append(note)
            note = None

    for line in lines:
        if line.startswith("### "):
            flush()
            result.append((line[4:].strip(), []))
        elif line.startswith("#") or not line.strip():
            flush()
        elif line.startswith("- "):
            flush()
            note = line[2:]
        elif line.startswith("  ") and note is not None:
            note += " " + line.strip()
    flush()
    return [(heading, notes) for heading, notes in result if notes]


def first_end(text):
    """Where the headline ends, as in changelog.rs: the first ": ", ". " or ":"."""
    ends = [i for i in (text.find(": "), text.find(". "), text.find(":")) if i >= 0]
    return min(ends, default=len(text))


def headline(text):
    """The change's headline as Telegram HTML: its bold parts bold, everything else escaped."""
    raw = text.strip().replace("`", "")
    plain = raw.replace("**", "")
    # A closing aside ("… в браузере (исходный код, выпуски…)") stays in the changelog.
    raw = re.sub(r"\s*\([^()]*\)$", "", raw[: first_end(raw)].rstrip(". *"))
    if len(plain[: first_end(plain)].rstrip(". ")) > HEADLINE:
        # Too long to keep its markup: plain text, cut at a word with "…".
        cut = plain[:HEADLINE]
        if " " in cut:
            cut = cut[: cut.rfind(" ")]
        return html.escape(cut.rstrip(", ") + "…", quote=False)
    parts = raw.split("**")
    if len(parts) % 2 == 0:
        # A bold part the cut left open ("**Гараж показывает…:** фильтр") closes at the cut.
        parts.append("")
    return "".join(
        f"<b>{html.escape(part, quote=False)}</b>" if i % 2 and part else html.escape(part, quote=False)
        for i, part in enumerate(parts)
    ).strip()


def utf16(text):
    return len(text.encode("utf-16-le")) // 2


def post(version, changelog, url):
    """The Bot API sendMessage fields, without chat_id, or None for a version with no section."""
    lines = section(changelog, version)
    if lines is None:
        return None
    head = (
        f"🚀 <b>Tanki Skin Studio {html.escape(version, quote=False)}</b>\n"
        "Студия сама предложит обновиться при запуске. Или скачайте по кнопке ниже."
    )
    blocks = []
    for heading, notes in groups(lines):
        title = HEADINGS.get(heading, heading)
        items = [f"• {headline(note)}" for note in notes]
        blocks.append(([f"<b>{html.escape(title, quote=False)}</b>"] if title else []) + items)

    def text(more):
        body = ["\n".join(block) for block in blocks]
        tail = ["…и другие изменения — по кнопке «Все изменения»."] if more else []
        return "\n\n".join([head, *body, *tail, "#обновление"])

    # A very long section loses its last headlines, never the title, the tag or the buttons.
    more = False
    while blocks and utf16(text(more)) > LIMIT:
        more = True
        blocks[-1].pop()
        # A heading left without changes goes too.
        if not any(line.startswith("• ") for line in blocks[-1]):
            blocks.pop()
    repository = os.environ.get("GITHUB_REPOSITORY") or REPOSITORY
    buttons = [
        {"text": "⬇️ Скачать", "url": url},
        {
            "text": "📋 Все изменения",
            "url": f"https://github.com/{repository}/blob/v{version}/CHANGELOG.ru.md",
        },
    ]
    return {
        "text": text(more),
        "parse_mode": "HTML",
        "link_preview_options": {"is_disabled": True},
        "reply_markup": {"inline_keyboard": [buttons]},
    }


def warn(text):
    # In a workflow run this is a visible warning.
    print(f"::warning::{text}", file=sys.stderr)


def send(fields, token, chat):
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "message.txt"
        path.write_bytes(fields["text"].encode("utf-8"))
        result = subprocess.run(
            [
                "curl", "--silent", "--show-error", "--fail-with-body",
                "--retry", "3", "--max-time", "60",
                "--config", "-",
                "--data-urlencode", f"chat_id={chat}",
                "--data-urlencode", f"parse_mode={fields['parse_mode']}",
                "--data-urlencode", "link_preview_options=" + json.dumps(fields["link_preview_options"]),
                "--data-urlencode", "reply_markup=" + json.dumps(fields["reply_markup"], ensure_ascii=False),
                "--data-urlencode", f"text@{path}",
            ],
            # curl reads the URL, and with it the token, from its configuration on standard input.
            input=f'url = "https://api.telegram.org/bot{token}/sendMessage"\n'.encode(),
            capture_output=True,
        )
    # Telegram's answer holds the message and the chat, never the token.
    answer = result.stdout.decode("utf-8", "replace")
    if result.returncode != 0:
        print(f"::error::Telegram refused the post: {answer}", file=sys.stderr)
        print(result.stderr.decode("utf-8", "replace"), file=sys.stderr)
        return 1
    print("Posted to Telegram")
    return 0


def out(text):
    sys.stdout.buffer.write(text.encode("utf-8") + b"\n")


def main(args):
    flags = {"--dry-run": False, "--json": False}
    options = {"--url": None, "--changelog": None}
    rest = []
    it = iter(args)
    for arg in it:
        if arg in flags:
            flags[arg] = True
        elif arg in options:
            options[arg] = next(it, "")
        else:
            rest.append(arg)
    if len(rest) != 1 or not VERSION.match(rest[0]) or "" in options.values():
        print(
            "usage: telegram-post.py <version> [--url <release page>] [--changelog <file>] [--dry-run | --json]",
            file=sys.stderr,
        )
        return 2
    version = rest[0]
    repository = os.environ.get("GITHUB_REPOSITORY") or REPOSITORY
    url = options["--url"] or f"https://github.com/{repository}/releases/tag/v{version}"
    changelog = Path(options["--changelog"] or ROOT / "CHANGELOG.ru.md")
    fields = post(version, changelog, url)
    if fields is None:
        warn(f"CHANGELOG.ru.md has no section for {version}; nothing is posted")
        return 0
    if flags["--json"]:
        out(json.dumps(fields, ensure_ascii=False, indent=2))
        return 0
    if flags["--dry-run"]:
        buttons = fields["reply_markup"]["inline_keyboard"][0]
        out(fields["text"])
        out("\n" + "   ".join(f"[{b['text']}] → {b['url']}" for b in buttons))
        return 0
    token = os.environ.get("TELEGRAM_BOT_TOKEN", "")
    chat = os.environ.get("TELEGRAM_CHAT_ID", "")
    if not token or not chat:
        warn("TELEGRAM_BOT_TOKEN or TELEGRAM_CHAT_ID is empty; nothing was posted (docs/releasing.md)")
        return 0
    return send(fields, token, chat)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
