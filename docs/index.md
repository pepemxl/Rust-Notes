# Rust Notes 🦀

Notas de un curso de **Rust en 6 meses**: desde instalar el compilador hasta entregar un
servicio completo, observable y listo para producción. Cada semana combina teoría,
ejemplos que compilan y un mini-proyecto que se reutiliza en los meses siguientes.

> **¿Primera vez aquí?** Empieza por [¿Por qué Rust?](chapter_00/section_01.md) y
> después [Instalación y primeros pasos](chapter_01/section_01.md).

---

## Para quién es este curso

- Personas que ya programan en otro lenguaje (Python, C/C++, Java, Go, JavaScript…) y
  quieren aprender Rust **a fondo**, no solo la sintaxis.
- No hace falta experiencia previa con gestión manual de memoria: el
  [Mes 1](chapter_01/section_00.md) la construye desde cero.
- Dedicación estimada: **8-12 horas por semana** (el proyecto final del Mes 6 pide bastante más: ~20 horas por semana).

---

## Plan del curso

| Mes | Tema | Semanas | Proyecto |
| :--- | :--- | :--- | :--- |
| **[1 · Fundamentos](chapter_01/section_00.md)** | Sintaxis, ownership y borrowing | [Instalación](chapter_01/section_01.md) · [Aritmética](chapter_01/section_02.md) · [Variables y control de flujo](chapter_01/section_03.md) · [Ownership](chapter_01/section_04.md) · [Structs y enums](chapter_01/section_05.md) · [Módulos y colecciones](chapter_01/section_06.md) | Todo List CLI |
| **[2 · Genéricos y traits](chapter_02/section_00.md)** | Abstracciones de costo cero y testing | [Generics y lifetimes](chapter_02/section_01.md) · [Traits avanzados](chapter_02/section_02.md) · [Smart pointers](chapter_02/section_03.md) · [Testing y tooling](chapter_02/section_04.md) | Crate `config-loader` |
| **[3 · Async y web](chapter_03/section_00.md)** | Futures, Tokio y Axum | [Async por dentro](chapter_03/section_01.md) · [Tokio y Axum](chapter_03/section_02.md) · [SQLx y Serde](chapter_03/section_03.md) · [Observabilidad y CI/CD](chapter_03/section_04.md) | API *Url Shortener* |
| **[4 · Sistemas](chapter_04/section_00.md)** | CLI, FFI, WebAssembly y parsing | [CLI profesional](chapter_04/section_01.md) · [FFI y `unsafe`](chapter_04/section_02.md) · [WebAssembly](chapter_04/section_03.md) · [Parsing](chapter_04/section_04.md) | `mytool`, Mandelbrot Wasm, *Log Parser* |
| **[5 · Arquitectura](chapter_05/section_00.md)** | Patrones, concurrencia y rendimiento | [Patrones de diseño](chapter_05/section_01.md) · [Lock-free](chapter_05/section_02.md) · [Profiling](chapter_05/section_03.md) · [Especialización](chapter_05/section_04.md) | Refactor y benchmarks |
| **[6 · Capstone](chapter_06/section_00.md)** | Proyecto final de portafolio | [Diseño](chapter_06/section_01.md) · [Core](chapter_06/section_02.md) · [HTTP y auth](chapter_06/section_03.md) · [Pulido final](chapter_06/section_04.md) | Servicio completo |

Cada mes empieza con una **guía de estudio** (plan semanal, recursos, errores frecuentes y
un laboratorio de código) y termina con un **hito** que conviene cumplir antes de avanzar:

| Mes | ✅ Hito |
| :--- | :--- |
| 1 | Todos los ejercicios de `rustlings` resueltos y el **Todo List CLI** funcionando y dividido en módulos. |
| 2 | Dominar `dyn Trait` vs `impl Trait`, lifetimes en structs y smart pointers. Publicar una crate bien testeada y documentada. |
| 3 | API async robusta, tipada, observable y en contenedor. Entender *por qué* importan `Pin` y `Send`/`Sync`. |
| 4 | CLI instalable con `cargo install`, wrapper FFI seguro, módulo Wasm funcionando en el navegador y un parser robusto. |
| 5 | Código rápido, medido y bien estructurado. Saber cuándo `unsafe` es necesario y cómo encapsularlo. |
| 6 | Capstone desplegado, documentado (`README`, `ARCHITECTURE.md`, changelog) y explicado en una demo de 10 minutos. |

---

## Cómo leer estas notas

| Marca | Significado |
| :--- | :--- |
| 💡 | Idea clave o consejo práctico. |
| ⚠️ | Error común o comportamiento sorprendente. |
| `// ❌ NO COMPILA` | Ejemplo **intencionalmente** incorrecto, para leer el error del compilador. |
| 🧪 **Mini-reto** | Ejercicio al final de cada sección; resuélvelo antes de seguir. |
| ✅ **Checklist** | Lo que deberías poder hacer sin mirar las notas. |

Los ejemplos usan **Rust 1.85+ con la edición 2024**. Escríbelos y ejecútalos tú mismo:
leer código de Rust sin compilarlo enseña la mitad.

---

## Herramientas

```bash
# Toolchain y componentes
rustup component add rustfmt clippy rust-src rust-analyzer

# Utilidades de cargo (`cargo add`, `cargo remove` y `cargo tree` ya vienen incluidos)
cargo install cargo-watch cargo-nextest cargo-audit cargo-deny cargo-outdated
cargo install cargo-edit        # para `cargo upgrade`
cargo install flamegraph cargo-criterion cargo-tarpaulin   # rendimiento y cobertura (Mes 2 y 5)
cargo install sqlx-cli          # migraciones y modo offline de SQLx (Mes 3)
```

Editores: **VS Code** con `rust-analyzer`, **Neovim** con `rustaceanvim`, o **RustRover**
(JetBrains).

---

## Biblioteca de referencia

| Recurso | Tipo | Cuándo |
| :--- | :--- | :--- |
| [The Rust Programming Language](https://doc.rust-lang.org/book/) ("The Book") | Libro, gratis | **Base de todo el curso** |
| [Rust by Example](https://doc.rust-lang.org/rust-by-example/) | Ejemplos, gratis | Meses 1-2 |
| [Rustlings](https://github.com/rust-lang/rustlings) | Ejercicios guiados | **Imprescindible** en los meses 1-2 |
| [Effective Rust](https://effective-rust.com/) (David Drysdale) | Libro, gratis | Mes 2 en adelante |
| [Asynchronous Programming in Rust](https://rust-lang.github.io/async-book/) | Libro, gratis | Mes 3 |
| [Zero To Production In Rust](https://www.zero2prod.com/) (Luca Palmieri) | Libro, de pago | Mes 3: backend profesional con Axum, SQLx, testing y CI |
| [The Rustonomicon](https://doc.rust-lang.org/nomicon/) | Libro, gratis | Mes 4: `unsafe` y FFI |
| [Rust Design Patterns](https://rust-unofficial.github.io/patterns/) | Catálogo, gratis | Mes 5 |
| [Rust Atomics and Locks](https://marabos.nl/atomics/) (Mara Bos) | Libro, gratis online | Mes 5: concurrencia de bajo nivel |
| [The Rust Performance Book](https://nnethercote.github.io/perf-book/) (Nicholas Nethercote) | Libro, gratis | Mes 5: optimización |
| [This Week in Rust](https://this-week-in-rust.org/) | Newsletter semanal | Siempre |

---

## Ver estas notas en local

Desde la raíz del repositorio:

```bash
make docs-serve   # sitio con recarga automática en http://localhost:8000
make docs-build   # genera el sitio estático en ./site
make help         # lista todos los comandos
```
