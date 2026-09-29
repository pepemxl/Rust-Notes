# Cómo trabajar en estas notas

Guía para agregar o editar contenido sin romper el sitio ni los ejemplos.

## Flujo de trabajo

1. Levanta el sitio con recarga automática: `make docs-serve` → <http://localhost:8000>.
2. Edita el Markdown en `docs/` y, si hace falta, el código en `src/`.
3. Antes de hacer commit, ejecuta `make check` (es lo mismo que corre el CI).

## Plantilla de una sección semanal

Todas las secciones `section_0N.md` siguen la misma estructura:

````markdown
# Título de la sección

Una frase de contexto. En esta sección aprenderemos:

- Objetivo 1.
- Objetivo 2.

!!! quote "Filosofía de la Semana N"

    *La idea central en una o dos líneas.*

---

## Tema

Explicación, ejemplo, salida real del programa.

## 🧪 Mini-reto: nombre

Enunciado y salida esperada.

??? success "Solución"

    ```rust
    --8<-- "src/chapter_NN/<crate>/src/main.rs"
    ```

---

## ✅ Checklist de la Semana N

- [ ] Algo que deberías poder hacer sin mirar las notas.

!!! abstract "Siguiente paso"

    [Título de la siguiente sección](section_0M.md).
````

## Convenciones de contenido

| Qué | Cómo |
| :--- | :--- |
| Consejo, advertencia, idea clave | `!!! tip`, `!!! warning`, `!!! note` con título en español |
| Soluciones | `??? success "Solución"` (colapsado por defecto) |
| Ejemplo que no compila a propósito | Comentario `// ❌ NO COMPILA` en el bloque, o ` ```rust,compile_fail ` |
| Salidas de terminal y errores del compilador | ` ```text `, copiadas de una ejecución real (no inventadas) |
| Rutas en salidas | Estilo Linux/macOS (`~/proyectos/...`); menciona Windows solo si difiere |
| Siglas | Si es nueva, agrégala a `docs/includes/abreviaturas.md` (tooltip en todo el sitio) |
| Términos de Rust | Agrégalos al [glosario](docs/glosario.md) con enlace a la semana donde se explican |
| Diagramas | Bloques ` ```mermaid ` |

## Código de ejemplo

**Ejemplos cortos**: escríbelos directamente en el Markdown. `make check-doc-blocks`
compila cada bloque ` ```rust ` con `rustc` (edición 2024).

**Programas completos, soluciones y cualquier cosa con tests**: crea un crate en el
workspace e inclúyelo con un snippet, así lo que se lee es exactamente lo que compila:

```bash
cargo new src/chapter_NN/nombre_unico   # el nombre debe ser único en todo el workspace
```

````markdown
```rust
--8<-- "src/chapter_NN/nombre_unico/src/main.rs"
```
````

Para incluir solo una parte del archivo, márcala con comentarios y usa
`--8<-- "ruta:seccion"`:

```rust
// --8<-- [start:solucion]
fn solucion() {}
// --8<-- [end:solucion]
```

Si un ejemplo es deliberadamente poco idiomático (por ejemplo, `println!("{}", "texto")`
para enseñar placeholders), silencia el lint **solo en ese crate** con `[lints.clippy]` en
su `Cargo.toml` y un comentario que explique por qué.

**Línea base de bloques.** Los fragmentos que dependen de un bloque anterior no compilan
solos; están registrados en `scripts/doc_blocks_baseline.json`. El CI solo falla con
fallos **nuevos**. Si arreglas bloques o agregas fragmentos a propósito, actualízala:

```bash
python3 scripts/check_doc_blocks.py --verbose            # ver qué falla y por qué
python3 scripts/check_doc_blocks.py --update-baseline    # fijar el nuevo estado
```

## Ortografía

`make check-spelling` revisa la prosa en español (el código se ignora). Si marca una
palabra técnica o una forma válida que el diccionario no conoce ("compílalo",
"backpressure"), agrégala a `words` en `cspell.json`.

## Mensajes de commit

Formato [Conventional Commits](https://www.conventionalcommits.org/es/), en español, con el
mes como *scope* cuando aplica:

```text
<tipo>(<scope>): <qué cambió, en imperativo>
```

| Tipo | Para |
| :--- | :--- |
| `docs` | Contenido nuevo o reescrito |
| `fix` | Corregir un error en las notas o en un ejemplo |
| `feat` | Nueva funcionalidad del sitio (plugin, página, diagrama) |
| `ci` | Workflows de GitHub Actions |
| `build` | Docker, Makefile, dependencias |
| `chore` | Limpieza sin impacto en el contenido |

Ejemplos:

```text
docs(mes1): reescribe Instalación y primeros pasos con la plantilla semanal
fix(mes2): Cache::get no compilaba por el borrow checker (NLL problem case #3)
feat(sitio): agrega glosario y tooltips para siglas
ci: compila los bloques de código con Rust 1.85 y stable
build(docs): ejecuta el contenedor con el UID del host
```

Un commit por tema: es más fácil de revisar y de revertir que un "fase 1" con 40 archivos.
