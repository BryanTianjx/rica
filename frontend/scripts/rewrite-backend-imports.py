"""F0-2 mechanical rewrite: every page/component/test import moves from
services/endpoints + services/api to the unified services/backend layer.

- Pass 1: import statements (single- and multi-line) of endpoints →
  `import { backend } from '<same-depth>/backend'`
- Pass 2: import statements of ApiError from services/api → services/backend
- Pass 3: call sites, word-boundary: candidatesApi.list → backend.candidates.list

backend/index.ts itself is excluded (it owns the endpoints import).
"""
import os
import re
import sys

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "src")

DOMAINS = [
    "candidatesApi", "positionsApi", "conversationsApi", "messagesApi",
    "importsApi", "followUpsApi", "draftsApi", "draftGroupsApi", "aiApi",
    "dashboardApi", "authApi", "settingsApi", "browserApi",
]
# domain key on the backend client (strip the Api suffix, lower-case first)
KEY = {name: name[:-3][0].lower() + name[:-3][1:] for name in DOMAINS}

IMPORT_RE = re.compile(
    r"import\s*\{[^}]*\}\s*from\s*(['\"])((?:[^'\"]*?services[/\\\\])?endpoints)\1;?",
    re.DOTALL,
)
APIERROR_RE = re.compile(
    r"import\s*\{\s*ApiError\s*\}\s*from\s*(['\"])((?:[^'\"]*?services[/\\\\])?api)\1;?"
)

changed = []
skipped = []

for dirpath, _dirnames, filenames in os.walk(ROOT):
    for filename in filenames:
        if not filename.endswith((".ts", ".tsx")):
            continue
        if dirpath.replace(os.sep, "/").endswith("services/backend"):
            continue
        path = os.path.join(dirpath, filename)
        original = text = open(path, encoding="utf-8").read()
        rel = os.path.relpath(path, ROOT).replace(os.sep, "/")

        if "services/endpoints" in text or re.search(r"services[/\\]endpoints", text):
            # 1. rewrite the endpoints import(s) → single backend import
            def import_sub(match):
                quote, prefix = match.group(1), match.group(2)
                depth = prefix.split("services")[0]
                return f"import {{ backend }} from '{depth}services/backend';"
            text = IMPORT_RE.sub(import_sub, text)
            # 2. call sites, word-boundary
            for api_name, key in KEY.items():
                text = re.sub(
                    rf"\b{api_name}\b",
                    f"backend.{key}",
                    text,
                )
            changed.append(rel)
        elif APIERROR_RE.search(text):
            pass

        # 3. ApiError imports → backend (works for files only importing it)
        def apierr_sub(match):
            quote, prefix = match.group(1), match.group(2)
            depth = prefix.split("services")[0]
            return f"import {{ ApiError }} from '{depth}services/backend';"
        new_text = APIERROR_RE.sub(apierr_sub, text)
        if new_text != original:
            open(path, "w", encoding="utf-8", newline="\n").write(new_text)
            if rel not in changed:
                changed.append(rel)

print(f"rewrote {len(changed)} files:")
for rel in sorted(changed):
    print("  ", rel)

# acceptance greps
remaining_endpoints = []
remaining_api = []
for dirpath, _dirnames, filenames in os.walk(ROOT):
    if dirpath.replace(os.sep, "/").endswith("services"):
        continue
    for filename in filenames:
        if not filename.endswith((".ts", ".tsx")):
            continue
        path = os.path.join(dirpath, filename)
        text = open(path, encoding="utf-8").read()
        if re.search(r"from\s*['\"][^'\"]*services/endpoints['\"]", text):
            remaining_endpoints.append(os.path.relpath(path, ROOT))
        if re.search(r"from\s*['\"][^'\"]*services/api['\"]", text):
            remaining_api.append(os.path.relpath(path, ROOT))
print("endpoints imports outside backend:", remaining_endpoints or "NONE")
print("api imports outside backend:", remaining_api or "NONE")
