# Rust Notes 🦀

Notas en español de un curso de **Rust en 6 meses**: de la instalación del compilador a un
proyecto final listo para producción. El contenido está escrito en Markdown y se publica
como sitio web con [MkDocs Material](https://squidfunk.github.io/mkdocs-material/).

## Contenido

| Mes | Tema | Proyecto |
| :--- | :--- | :--- |
| [0](docs/chapter_00/section_01.md) | ¿Por qué Rust? | — |
| [1](docs/chapter_01/section_00.md) | Fundamentos: sintaxis, ownership, borrowing, enums, errores | Todo List CLI |
| [2](docs/chapter_02/section_00.md) | Genéricos, traits, lifetimes, smart pointers, testing | Crate `config-loader` |
| [3](docs/chapter_03/section_00.md) | Async, Tokio, Axum, SQLx, observabilidad, CI/CD | API *Url Shortener* |
| [4](docs/chapter_04/section_00.md) | CLI, FFI y `unsafe`, WebAssembly, parsing | `mytool`, Mandelbrot Wasm, *Log Parser* |
| [5](docs/chapter_05/section_00.md) | Patrones de diseño, concurrencia lock-free, profiling | Refactor y benchmarks |
| [6](docs/chapter_06/section_00.md) | Proyecto final (*capstone*) | Servicio completo de portafolio |

Cada mes empieza con una guía de estudio (`section_00.md`) con el plan semanal y los
recursos, y sigue con una sección por semana. La [portada](docs/index.md) tiene el índice
completo, las herramientas recomendadas y la biblioteca de referencia.

Los ejemplos usan **Rust 1.85+ con la edición 2024**.

## Ver el sitio en local

Con Docker:

```bash
make docs-serve   # http://localhost:8000 con recarga automática
make docs-build   # genera el sitio estático en ./site
make help         # todos los comandos disponibles
```

Sin Docker, con Python 3:

```bash
pip install -r requirements-docs.txt
mkdocs serve
```

## Verificación

`make check` corre lo mismo que el CI (`.github/workflows/ci.yml`):

| Objetivo | Qué verifica |
| :--- | :--- |
| `make check-rust` | `fmt`, `clippy -D warnings` y tests del workspace de ejemplos |
| `make check-doc-blocks` | Compila cada bloque ` ```rust ` de `docs/` con `rustc` (edición 2024) |
| `make check-docs` | Construye el sitio con `mkdocs build --strict` (enlaces y snippets) |
| `make check-spelling` | Ortografía en español con `cspell` (requiere Node) |
| `make check-python` | `ruff` y `mypy --strict` sobre `scripts/` |

Para agregar contenido o ejemplos (plantilla de sección, snippets, convenciones y formato
de commits), lee [CONTRIBUTING.md](CONTRIBUTING.md).

## Estructura del repositorio

```text
.
├── docs/                    # contenido del curso (Markdown)
│   ├── index.md             # portada del sitio
│   ├── chapter_00/          # introducción
│   ├── chapter_01/ … chapter_06/
│   │   ├── section_00.md    # guía de estudio del mes
│   │   └── section_0N.md    # una sección por semana
│   └── images/
├── src/chapter_NN/<ejemplo> # workspace de Cargo con el código que las notas incluyen
├── projects/                # proyectos de práctica (reproductor de video)
├── scripts/                 # verificación de los bloques de código de docs/
├── Cargo.toml               # workspace de los ejemplos
├── cspell.json              # corrector ortográfico (palabras técnicas aceptadas)
├── pyproject.toml           # configuración de ruff y mypy para scripts/
├── CONTRIBUTING.md          # cómo agregar contenido y convenciones
├── mkdocs.yml               # configuración y navegación del sitio
├── Dockerfile.docs          # imagen para servir / construir la documentación
├── docker-compose.docs.yml
└── Makefile
```

## Autor

Jose Alonzo
