#!/usr/bin/env python3
"""Compila cada bloque ```rust de docs/ para detectar ejemplos rotos.

Cada bloque se clasifica en:

  ok         compila (tal cual, o envuelto en `fn main() { ... }` si es un fragmento
             de sentencias).
  intencional contiene "NO COMPILA" o la cerca es ```rust,compile_fail / ```rust,ignore:
             no se exige nada.
  externo    usa una crate que no está en scripts/doc_deps/Cargo.toml: se omite.
             Las que sí están (tokio, axum, serde...) se compilan una vez con Cargo y
             se pasan a rustc con --extern, así esos ejemplos también se verifican.
  requiere_bd usa sqlx::query! (o similares) y no hay DATABASE_URL: se omite. Con
             DATABASE_URL (como en el CI) el SQL se valida contra PostgreSQL real.
  crate      el bloque es solo un snippet (--8<--) de un crate del workspace: ya lo
             compila `cargo` en su contexto real (make check-rust), no se repite aquí.
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
import shutil
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

# Errores que indican que el bloque necesita una crate que no está disponible.
EXTERNAL_ERRORS = re.compile(
    r"unresolved import `(?!crate|self|super|std|core|alloc)"
    r"|use of unresolved module or unlinked crate"
    r"|use of undeclared crate or module"
    r"|cannot find module or crate"
    r"|can't find crate for"
    r"|failed to resolve: use of undeclared crate"
)
# Sin doc_deps, un `#[derive(Serialize)]` o `#[serde(...)]` sin crate también es
# "externo". Con doc_deps disponibles, ese error significa que falta un `use`.
EXTERNAL_ERRORS_SIN_DEPS = re.compile(
    EXTERNAL_ERRORS.pattern + r"|cannot find attribute `|cannot find derive macro `"
)


SIN_CACHE_SQLX = "SQLX_OFFLINE=true` but there is no cached data"


def _es_externo(stderr: str) -> bool:
    patron = EXTERNAL_ERRORS if EXTERN_ARGS else EXTERNAL_ERRORS_SIN_DEPS
    return bool(patron.search(stderr))


@dataclass
class Block:
    """Un bloque ```rust extraído de un archivo Markdown."""

    file: Path
    line: int
    code: str
    info: str = ""  # atributos de la cerca: ```rust,compile_fail → "compile_fail"
    from_crate: bool = False  # el bloque es solo un snippet de src/ (lo compila cargo)

    @property
    def key(self) -> str:
        """Hash estable del contenido (ignora espacios finales) para la línea base."""
        normalized = "\n".join(ln.rstrip() for ln in self.code.strip().splitlines())
        return hashlib.sha1(normalized.encode()).hexdigest()[:16]


@dataclass
class Result:
    """Resultado de compilar un bloque."""

    block: Block
    status: str  # ok | intencional | externo | falla
    error: str = ""


def extract_blocks(path: Path) -> list[Block]:
    """Extrae los bloques ```rust de un archivo, resolviendo snippets e indentación."""
    blocks: list[Block] = []
    lines = path.read_text(encoding="utf-8").splitlines()
    i = 0
    while i < len(lines):
        m = FENCE_OPEN.match(lines[i])
        if not m:
            i += 1
            continue
        indent, info, start = m.group(1), m.group(2), i + 1
        body: list[str] = []
        i += 1
        while i < len(lines) and lines[i].strip() != "```":
            line = lines[i]
            body.append(line[len(indent):] if line.startswith(indent) else line.lstrip())
            i += 1
        code = "\n".join(_expand_snippets(body))
        from_crate = bool(body) and all(
            SNIPPET.match(ln) and '"src/' in ln for ln in body if ln.strip()
        )
        blocks.append(Block(path, start, code, info, from_crate))
        i += 1
    return blocks


SECTION_MARK = re.compile(r"--8<--\s*\[(start|end):([\w-]+)\]")


def _expand_snippets(body: list[str]) -> list[str]:
    """Resuelve `--8<-- "ruta"` y `--8<-- "ruta:seccion"` como pymdownx.snippets."""
    out: list[str] = []
    for line in body:
        m = SNIPPET.match(line)
        if not m:
            out.append(line)
            continue
        path, _, section = m.group(1).partition(":")
        lines = (ROOT / path).read_text(encoding="utf-8").splitlines()
        if section:
            inside, selected = False, []
            for ln in lines:
                mark = SECTION_MARK.search(ln)
                if mark and mark.group(2) == section:
                    inside = mark.group(1) == "start"
                elif inside:
                    selected.append(ln)
            lines = selected
        out.extend(ln for ln in lines if not SECTION_MARK.search(ln))
    return out


EXTERN_ARGS: list[str] = []  # --extern / -L de doc_deps; se llena en load_externs()

# Las macros de sqlx (query!, migrate!) se resuelven como si el bloque estuviera dentro
# del proyecto del Mes 3: ahí están migrations/ y el caché offline .sqlx/. Con
# DATABASE_URL definida, el SQL de los ejemplos se valida contra esa base real.
SQLX_PROJECT = ROOT / "src" / "chapter_03" / "url_shortener_v2"


def load_externs() -> list[str]:
    """Compila scripts/doc_deps y devuelve los argumentos --extern para rustc."""
    meta = json.loads(subprocess.run(
        ["cargo", "metadata", "--format-version", "1"],
        cwd=ROOT, capture_output=True, text=True, check=True,
    ).stdout)
    pkg = next(p for p in meta["packages"] if p["name"] == "doc_deps")
    node = next(n for n in meta["resolve"]["nodes"] if n["id"] == pkg["id"])
    wanted = {d["pkg"]: d["name"] for d in node["deps"]}  # id del paquete → nombre extern

    # Con --target explícito, Cargo separa las crates del programa (target/<triple>/)
    # de las que usan las proc-macros y build scripts (target/debug/). Sin eso habría
    # dos `libtokio-*.rlib` con features distintas y rustc no sabría cuál usar.
    host = next(
        ln.split()[1] for ln in subprocess.run(
            ["rustc", "-vV"], capture_output=True, text=True, check=True,
        ).stdout.splitlines() if ln.startswith("host:")
    )
    build = subprocess.run(
        ["cargo", "build", "-p", "doc_deps", "--target", host, "--message-format=json"],
        cwd=ROOT, capture_output=True, text=True,
    )
    if build.returncode != 0:
        raise RuntimeError(build.stderr.strip().splitlines()[-1])
    target_dir = meta["target_directory"]
    args = [
        "-L", f"dependency={target_dir}/{host}/debug/deps",
        "-L", f"dependency={target_dir}/debug/deps",
    ]
    for line in build.stdout.splitlines():
        msg = json.loads(line)
        if msg.get("reason") != "compiler-artifact" or msg["package_id"] not in wanted:
            continue
        kinds = set(msg["target"]["kind"])
        if "proc-macro" in kinds:
            suffixes: tuple[str, ...] = (".so", ".dylib", ".dll")
        elif kinds & {"lib", "rlib"}:
            suffixes = (".rlib",)
        else:
            continue
        path = next((f for f in msg["filenames"] if f.endswith(suffixes)), None)
        if path and ("proc-macro" in kinds or f"/{host}/" in path):
            args += ["--extern", f"{wanted[msg['package_id']]}={path}"]
    return args


def rustc(code: str, crate_type: str, workdir: str) -> subprocess.CompletedProcess[str]:
    """Compila `code` con rustc (solo metadata, sin generar binario)."""
    src = Path(workdir) / "bloque.rs"
    src.write_text(code, encoding="utf-8")
    return subprocess.run(
        ["rustc", "--edition", EDITION, "--crate-type", crate_type,
         "--crate-name", "bloque", "--emit=metadata", "--cap-lints", "allow",
         "--out-dir", workdir, *EXTERN_ARGS, str(src)],
        capture_output=True, text=True,
        env={
            **os.environ,
            "CARGO_MANIFEST_DIR": str(SQLX_PROJECT),
            "CARGO": shutil.which("cargo") or "cargo",  # sqlx lo usa en modo offline
            # Lo que Cargo define para cada crate: clap (#[command(version)]) y
            # wasm-bindgen los leen al expandir sus macros.
            "CARGO_PKG_NAME": "bloque",
            "CARGO_PKG_VERSION": "0.1.0",
            "CARGO_PKG_VERSION_MAJOR": "0",
            "CARGO_PKG_VERSION_MINOR": "1",
            "CARGO_PKG_VERSION_PATCH": "0",
            "CARGO_PKG_AUTHORS": "",
            "CARGO_PKG_DESCRIPTION": "Ejemplo de Rust Notes",
            "CARGO_CRATE_NAME": "bloque",
            "SQLX_OFFLINE": "false" if os.environ.get("DATABASE_URL") else "true",
        },
    )


def check(block: Block) -> Result:
    """Clasifica un bloque: ok, intencional, externo o falla."""
    if "NO COMPILA" in block.code or re.search(r"compile_fail|ignore", block.info):
        return Result(block, "intencional")
    if block.from_crate:
        return Result(block, "crate")
    with tempfile.TemporaryDirectory() as tmp:
        has_main = re.search(r"^\s*(pub\s+)?fn\s+main\s*\(", block.code, re.M)
        first = rustc(block.code, "bin" if has_main else "lib", tmp)
        if first.returncode == 0:
            return Result(block, "ok")
        if _es_externo(first.stderr):
            return Result(block, "externo")
        if SIN_CACHE_SQLX in first.stderr:
            return Result(block, "requiere_bd")
        stderr = first.stderr
        if not has_main:
            # Fragmento de sentencias sueltas (`let x = ...;`): probar dentro de main.
            wrapped = rustc("fn main() {\n" + block.code + "\n}\n", "bin", tmp)
            if wrapped.returncode == 0:
                return Result(block, "ok")
            if ".await" in block.code:
                # Fragmento con `.await` sueltos: probar dentro de una función async.
                in_async = rustc("async fn _bloque() {\n" + block.code + "\n}\n", "lib", tmp)
                if in_async.returncode == 0:
                    return Result(block, "ok")
                wrapped = in_async
            if _es_externo(wrapped.stderr):
                return Result(block, "externo")
            if SIN_CACHE_SQLX in wrapped.stderr:
                return Result(block, "requiere_bd")
            if "expected item" in stderr:
                stderr = wrapped.stderr  # el error útil es el del fragmento envuelto
        return Result(block, "falla", _first_error(stderr))


def _first_error(stderr: str) -> str:
    lines = [ln for ln in stderr.splitlines() if ln.startswith("error")]
    return lines[0] if lines else stderr.strip().splitlines()[0] if stderr.strip() else ""


def main() -> int:
    """Punto de entrada: verifica los bloques o actualiza la línea base."""
    parser = argparse.ArgumentParser(description=(__doc__ or "").split("\n")[0])
    parser.add_argument(
        "files", nargs="*", type=Path, help="archivos .md (por defecto: todo docs/)",
    )
    parser.add_argument("--update-baseline", action="store_true")
    parser.add_argument("--verbose", "-v", action="store_true", help="lista cada bloque que falla")
    parser.add_argument(
        "--sin-deps", action="store_true",
        help="no compilar scripts/doc_deps (solo std; los bloques con crates quedan como externo)",
    )
    args = parser.parse_args()

    if not args.sin_deps:
        try:
            EXTERN_ARGS.extend(load_externs())
        except (RuntimeError, subprocess.CalledProcessError, StopIteration) as err:
            print(f"⚠️  No se pudieron compilar las dependencias de doc_deps ({err}).")
            print("    Se verifica solo con std; usa --sin-deps para omitir este paso.")

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
        total = sum(map(len, failing.values()))
        print(f"Línea base actualizada: {total} bloques con fallo conocido.")
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
          + ", ".join(f"{counts.get(s, 0)} {s}"
                      for s in ("ok", "crate", "intencional", "externo", "requiere_bd", "falla")))

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
