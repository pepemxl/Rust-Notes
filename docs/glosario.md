# Glosario

Los términos de Rust se usan en inglés en el ecosistema (documentación, mensajes del
compilador, crates), así que las notas los mantienen. Aquí está su significado y dónde se
estudian. Las siglas (FFI, UB, GC…) muestran su definición al pasar el cursor sobre
ellas en cualquier página.

## Memoria y ownership

**Ownership (propiedad)**
:   Cada valor tiene un único dueño; cuando el dueño sale de scope, el valor se libera
    (`drop`). Es la base de la seguridad de memoria sin GC.
    → [Semana 2](chapter_01/section_04.md)

**Move**
:   Transferencia del ownership al asignar o pasar un valor por valor. El nombre anterior
    queda inválido. → [Semana 2](chapter_01/section_04.md#move-transferencia-de-ownership)

**Copy / Clone**
:   `Copy`: copia implícita bit a bit de tipos baratos (`i32`, `bool`, `&T`). `Clone`:
    copia explícita y posiblemente costosa (`.clone()`).
    → [Semana 2](chapter_01/section_04.md)

**Borrowing (préstamo)**
:   Usar un valor mediante una referencia sin tomar su ownership: `&T` (compartido, solo
    lectura) o `&mut T` (exclusivo, permite modificar).
    → [Semana 2](chapter_01/section_04.md#referencias-y-borrowing)

**Borrow checker**
:   La parte del compilador que verifica las reglas de préstamos: muchas `&T` **o** una
    sola `&mut T`, y ninguna referencia puede vivir más que el dato.

**Lifetime (tiempo de vida)**
:   La región del programa durante la que una referencia es válida. Se anota como `'a`
    cuando el compilador no puede inferirla.
    → [Semana 5](chapter_02/section_01.md)

**Slice**
:   Vista prestada a una secuencia contigua sin ownership: `&str` (texto) o `&[T]`.
    → [Semana 2](chapter_01/section_04.md#slices-vistas-sin-ownership)

**Stack / Heap**
:   *Stack* (pila): memoria rápida de tamaño fijo conocido al compilar. *Heap*
    (montículo): memoria dinámica, como la que usan `String`, `Vec` y `Box`.

**Smart pointer**
:   Tipo que actúa como puntero con comportamiento extra: `Box<T>` (heap), `Rc<T>` /
    `Arc<T>` (conteo de referencias), `RefCell<T>` (mutabilidad interior).
    → [Semana 7](chapter_02/section_03.md)

**Interior mutability (mutabilidad interior)**
:   Modificar un valor a través de una referencia compartida, con la verificación
    trasladada a runtime (`RefCell`, `Mutex`, atómicos).
    → [Semana 7](chapter_02/section_03.md)

## Tipos y abstracción

**Trait**
:   Conjunto de métodos que un tipo puede implementar; equivale a una interfaz. Permite
    genéricos (`T: Trait`) y dispatch dinámico (`dyn Trait`).
    → [Semanas 4-6](chapter_02/section_02.md)

**Generics y monomorphization**
:   Código parametrizado por tipos (`fn f<T>`). El compilador genera una copia
    especializada por cada tipo usado, así que no hay costo en runtime.
    → [Semana 5](chapter_02/section_01.md)

**`dyn Trait` / trait object**
:   Valor cuyo tipo concreto se decide en runtime; las llamadas pasan por una *vtable*.
    → [Semana 6](chapter_02/section_02.md)

**Zero-cost abstraction**
:   Abstracción que no cuesta más en runtime que el código equivalente escrito a mano
    (iteradores, genéricos, closures).

**Pattern matching**
:   Desestructurar valores con `match`, `if let` y `let else`. El compilador exige cubrir
    todos los casos (exhaustividad).
    → [Semana 3](chapter_01/section_05.md)

**Newtype / Typestate**
:   *Newtype*: struct de un campo para dar un tipo propio a un valor (`struct Metros(f64)`).
    *Typestate*: codificar el estado en el tipo para que las transiciones inválidas no
    compilen. → [Semana 17](chapter_05/section_01.md)

**Crate / módulo / workspace**
:   *Crate*: unidad de compilación (binario o librería). *Módulo*: espacio de nombres
    dentro de un crate. *Workspace*: varios crates que comparten `Cargo.lock` y `target/`.
    → [Semana 4](chapter_01/section_06.md)

**Edición**
:   Versión del lenguaje que declara cada crate (`edition = "2024"`); permite cambios
    incompatibles sin romper el código existente.
    → [Instalación](chapter_01/section_01.md)

## Concurrencia y async

**`Send` / `Sync`**
:   `Send`: el valor se puede mover a otro hilo. `Sync`: se puede compartir por referencia
    entre hilos. El compilador los usa para impedir *data races*.
    → [Semana 9](chapter_03/section_01.md)

**Future**
:   Valor que representa un cálculo que terminará más adelante. No hace nada hasta que un
    executor lo sondea con `poll`.
    → [Semana 9](chapter_03/section_01.md)

**Executor / runtime**
:   Quien sondea las futures y las despierta cuando pueden avanzar. Tokio es el más usado.
    → [Semana 10](chapter_03/section_02.md)

**`Pin`**
:   Garantía de que un valor no se moverá en memoria; necesaria para las futures que
    guardan referencias a sí mismas.
    → [Semana 9](chapter_03/section_01.md)

**Data race**
:   Dos hilos acceden a la misma memoria a la vez y al menos uno escribe, sin
    sincronización. En Rust seguro es un error de compilación.

## Seguridad y bajo nivel

**`unsafe`**
:   Bloque o función donde el programador asume la responsabilidad de invariantes que el
    compilador no puede verificar (punteros raw, FFI).
    → [Semana 14](chapter_04/section_02.md)

**Panic**
:   Error irrecuperable que detiene el hilo actual de forma controlada (por ejemplo, un
    índice fuera de rango o un overflow en modo *debug*).
    → [Aritmética](chapter_01/section_02.md)
