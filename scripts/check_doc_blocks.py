#!/usr/bin/env python3
"""Compila cada bloque ```rust de docs/ para detectar ejemplos rotos.

Cada bloque se clasifica en:

  ok         compila (tal cual, o envuelto en `fn main() { ... }` si es un fragmento
             de sentencias).
  intencional contiene "NO COMPILA" o la cerca es ```rust,compile_fail / ```rust,ignore:
             no se exige nada.
  externo    usa crates fuera de std (tokio, axum, serde...): no se puede compilar
             con `rustc` solo, se omite.
  falla      no compila por otra razón: fragmento que depende de un bloque anterior
             o error real.

Los fallos ya conocidos se guardan en scripts/doc_blocks_baseline.json. El script
termina con error solo si aparece un fallo NUEVO, así la deuda existente no bloquea
el CI pero no puede crecer. Cuando arregles bloques, regenera la línea base con
--update-baseline para que no vuelvan a romperse.

Uso:
  python3 scripts/check_doc_blocks.py                  # verificar
  python3 scripts/check_doc_blocks.py --update-baseline
  python3 scripts/check_doc_blocks.py --verbose docs/chapter_01/section_02.md
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DOCS = ROOT / "docs"
BASELINE = ROOT / "scripts" / "doc_blocks_baseline.json"
EDITION = "2024"

FENCE_OPEN = re.compile(r"^(\s*)```rust\b(.*)$")
SNIPPET = re.compile(r'^\s*--8<--\s+"([^"]+)"\s*$')

# Errores que indican que el bloque necesita una crate externa.
EXTERNAL_ERRORS = re.compile(
    r"unresolved import `(?!crate|self|super|std|core|alloc)"
    r"|use of unresolved module or unlinked crate"
    r"|use of undeclared crate or module"
    r"|cannot find module or crate"
    r"|can't find crate for"
    r"|cannot find attribute `"
    r"|cannot find derive macro `"
    r"|failed to resolve: use of undeclared crate"
)


@dataclass
class Block:
    file: Path
    line: int
    code: str
    info: str = ""  # atributos de la cerca: ```rust,compile_fail → "compile_fail"

    @property
    def key(self) -> str:
        normalized = "\n".join(l.rstrip() for l in self.code.strip().splitlines())
        return hashlib.sha1(normalized.encode()).hexdigest()[:16]


@dataclass
class Result:
    block: Block
    status: str  # ok | intencional | externo | falla
    error: str = ""


def extract_blocks(path: Path) -> list[Block]:
    blocks: list[Block] = []
    lines = path.read_text(encoding="utf-8").splitlines()
    i = 0
    while i < len(lines):
        m = FENCE_OPEN.match(lines[i])
        if not m:
            i += 1
            continue
        indent, info, start, body = m.group(1), m.group(2), i + 1, []
        i += 1
        while i < len(lines) and lines[i].strip() != "```":
            line = lines[i]
            body.append(line[len(indent):] if line.startswith(indent) else line.lstrip())
            i += 1
        code = "\n".join(_expand_snippets(body))
        blocks.append(Block(path, start, code, info))
        i += 1
    return blocks


def _expand_snippets(body: list[str]) -> list[str]:
    out: list[str] = []
    for line in body:
        m = SNIPPET.match(line)
        if m:
            out.extend((ROOT / m.group(1)).read_text(encoding="utf-8").splitlines())
        else:
            out.append(line)
    return out


def rustc(code: str, crate_type: str, workdir: str) -> subprocess.CompletedProcess:
    src = Path(workdir) / "bloque.rs"
    src.write_text(code, encoding="utf-8")
    return subprocess.run(
        ["rustc", "--edition", EDITION, "--crate-type", crate_type,
         "--crate-name", "bloque", "--emit=metadata", "--cap-lints", "allow",
         "--out-dir", workdir, str(src)],
        capture_output=True, text=True,
    )


def check(block: Block) -> Result:
    if "NO COMPILA" in block.code or re.search(r"compile_fail|ignore", block.info):
        return Result(block, "intencional")
    with tempfile.TemporaryDirectory() as tmp:
        has_main = re.search(r"^\s*(pub\s+)?fn\s+main\s*\(", block.code, re.M)
        first = rustc(block.code, "bin" if has_main else "lib", tmp)
        if first.returncode == 0:
            return Result(block, "ok")
        if EXTERNAL_ERRORS.search(first.stderr):
            return Result(block, "externo")
        stderr = first.stderr
        if not has_main:
            # Fragmento de sentencias sueltas (`let x = ...;`): probar dentro de main.
            wrapped = rustc("fn main() {\n" + block.code + "\n}\n", "bin", tmp)
            if wrapped.returncode == 0:
                return Result(block, "ok")
            if EXTERNAL_ERRORS.search(wrapped.stderr):
                return Result(block, "externo")
            if "expected item" in stderr:
                stderr = wrapped.stderr  # el error útil es el del fragmento envuelto
        return Result(block, "falla", _first_error(stderr))


def _first_error(stderr: str) -> str:
    lines = [l for l in stderr.splitlines() if l.startswith("error")]
    return lines[0] if lines else stderr.strip().splitlines()[0] if stderr.strip() else ""


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("files", nargs="*", type=Path, help="archivos .md (por defecto: todo docs/)")
    parser.add_argument("--update-baseline", action="store_true")
    parser.add_argument("--verbose", "-v", action="store_true", help="lista cada bloque que falla")
    args = parser.parse_args()

    files = [f.resolve() for f in args.files] or sorted(DOCS.rglob("*.md"))
    blocks = [b for f in files for b in extract_blocks(f)]
    with ThreadPoolExecutor(max_workers=os.cpu_count() or 4) as pool:
        results = list(pool.map(check, blocks))

    failing: dict[str, list[str]] = {}
    for r in results:
        if r.status == "falla":
            failing.setdefault(str(r.block.file.relative_to(ROOT)), []).append(r.block.key)

    if args.update_baseline:
        BASELINE.write_text(json.dumps(
            {f: sorted(set(k)) for f, k in sorted(failing.items())},
            indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        print(f"Línea base actualizada: {sum(map(len, failing.values()))} bloques con fallo conocido.")
        return 0

    baseline: dict[str, list[str]] = json.loads(BASELINE.read_text()) if BASELINE.exists() else {}
    known = {(f, k) for f, ks in baseline.items() for k in ks}
    checked_files = {str(f.relative_to(ROOT)) for f in files}

    new = [r for r in results if r.status == "falla"
           and (str(r.block.file.relative_to(ROOT)), r.block.key) not in known]
    still_failing = {(str(r.block.file.relative_to(ROOT)), r.block.key)
                     for r in results if r.status == "falla"}
    fixed = [(f, k) for f, k in known if f in checked_files and (f, k) not in still_failing]

    counts: dict[str, int] = {}
    for r in results:
        counts[r.status] = counts.get(r.status, 0) + 1
    print(f"{len(results)} bloques en {len(files)} archivos: "
          + ", ".join(f"{counts.get(s, 0)} {s}" for s in ("ok", "intencional", "externo", "falla")))

    if args.verbose:
        for r in results:
            if r.status == "falla":
                mark = "NUEVO " if r in new else "conocido"
                print(f"  [{mark}] {r.block.file.relative_to(ROOT)}:{r.block.line}  {r.error}")

    if fixed:
        print(f"\n✅ {len(fixed)} bloque(s) de la línea base ya no fallan (o cambiaron).")
        print("   Ejecuta con --update-baseline para fijar el avance.")

    if new:
        print(f"\n❌ {len(new)} bloque(s) nuevos no compilan:\n")
        for r in new:
            print(f"  {r.block.file.relative_to(ROOT)}:{r.block.line}")
            print(f"      {r.error}")
        print("\nArréglalos, márcalos con `// ❌ NO COMPILA` si el error es intencional,")
        print("o (si dependen de un bloque anterior) ejecuta --update-baseline.")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
