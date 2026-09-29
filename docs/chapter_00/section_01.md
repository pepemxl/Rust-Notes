# ¿Por qué Rust?

Antes de escribir la primera línea de código conviene entender **qué problema resuelve
Rust** y cuándo vale la pena pagar su curva de aprendizaje. En esta sección veremos:

- El problema que Rust ataca: **seguridad de memoria sin recolector de basura**.
- De dónde viene el lenguaje y quién lo usa hoy en producción.
- Cómo se compara con C/C++, Go y Python.
- Cuándo **sí** y cuándo **no** elegir Rust.
- Cómo está organizado este curso.

> 💡 **Idea central:** *Rust traslada una gran familia de bugs (memoria, concurrencia)
> del tiempo de ejecución al tiempo de compilación. Pagas con un compilador más estricto;
> cobras con programas que fallan mucho menos en producción.*

---

## El problema: rendimiento *y* seguridad

Hoy en día los sistemas *data intensive* y *compute intensive* son cada vez más comunes.
Un patrón frecuente es construir el MVP en un lenguaje productivo como Python y, una vez
que la prueba de concepto funciona y la demanda de cómputo crece, **migrar poco a poco
las partes críticas** a un lenguaje más eficiente. Para que esa migración sea posible,
el sistema debe estar construido con servicios desacoplados (o desacoplarse después, lo
cual siempre es más costoso, aunque muy común).

Durante décadas la opción para esas partes críticas fue C/C++. El problema es que en
proyectos grandes C/C++ se vuelve un cuello de botella:

- **Errores de memoria**: *use-after-free*, *double free*, desbordamientos de buffer,
  punteros nulos. Son difíciles de rastrear y muchos son vulnerabilidades de seguridad.
  Microsoft ha reportado que cerca del **70 %** de las vulnerabilidades que corrige con
  CVE son de seguridad de memoria.
- **Concurrencia frágil**: las *data races* no se detectan al compilar; aparecen como
  fallos intermitentes bajo carga.
- **Ecosistema fragmentado**: varios sistemas de build, gestión manual de dependencias,
  librerías antiguas sin mantenimiento.
- **Costo de mantenimiento**: cada cambio exige revisar a mano invariantes que el
  compilador no comprueba.

Los lenguajes con recolector de basura (Java, Go, Python) eliminan buena parte de los
errores de memoria, pero a cambio de pausas del GC, mayor consumo de memoria y menos
control sobre el hardware.

**Rust ofrece una tercera vía**: gestión de memoria determinista *sin* recolector de
basura, verificada por el compilador mediante el sistema de **ownership** (propiedad) y
**borrowing** (préstamos), que estudiaremos a fondo en la
[Semana 2](../chapter_01/section_04.md).

---

## Qué es Rust

Rust es un lenguaje de programación **compilado**, **multiparadigma** y **de tipado
estático**, enfocado en tres objetivos:

| Objetivo | Cómo lo logra |
| :--- | :--- |
| **Rendimiento** | Compila a código nativo vía LLVM. *Zero-cost abstractions*: iteradores, genéricos y closures cuestan lo mismo que el código equivalente escrito a mano. Sin runtime ni GC. |
| **Seguridad de memoria** | El *borrow checker* garantiza en compilación que no hay referencias colgantes, dobles liberaciones ni accesos a memoria liberada. No existen referencias nulas: la ausencia de valor se modela con `Option<T>`. |
| **Concurrencia sin miedo** | Los traits `Send` y `Sync` hacen que una *data race* sea un **error de compilación**, no un bug en producción. |

La sintaxis recuerda a C++ (llaves, `::`, genéricos con `<T>`), pero con influencias
fuertes de lenguajes funcionales como OCaml y Haskell: `match`, tipos algebraicos
(`enum` con datos), inferencia de tipos y casi todo es una expresión.

### Un poco de historia

- **2006**: Graydon Hoare lo inicia como proyecto personal.
- **2009**: Mozilla empieza a patrocinarlo; lo usa para construir **Servo**, un motor de
  navegador experimental, y más tarde partes de **Firefox**.
- **2015**: se publica **Rust 1.0** con una promesa de estabilidad: el código que compila
  en una versión estable sigue compilando en las siguientes.
- **2021**: se crea la **Rust Foundation** (AWS, Google, Huawei, Microsoft, Mozilla,
  entre otros).
- Desde entonces sale una versión estable **cada 6 semanas**, y cada ~3 años una nueva
  **edición** (2015, 2018, 2021, 2024) que permite evolucionar el lenguaje sin romper
  código existente. En este curso usamos la **edición 2024**.

### Quién lo usa en producción

| Dónde | Para qué |
| :--- | :--- |
| **Kernel de Linux** | Soporte oficial para escribir drivers en Rust desde la versión 6.1 (2022). |
| **Android** | Google escribe en Rust buena parte del código nuevo de sistema; las vulnerabilidades de memoria bajaron drásticamente al dejar de escribir código nuevo en C/C++. |
| **Windows** | Microsoft ha reescrito en Rust componentes del kernel y de la librería gráfica. |
| **AWS** | *Firecracker*, las microVMs detrás de Lambda y Fargate. |
| **Cloudflare** | *Pingora*, su proxy HTTP, que reemplazó a NGINX. |
| **Discord** | Migró un servicio crítico de Go a Rust para eliminar los picos de latencia del GC. |
| **Herramientas de datos** | `polars` (DataFrames), `datafusion` (motor SQL), `ruff` y `uv` (tooling de Python). |

La última fila es un spoiler de hacia dónde vamos: **datos**. Rust brilla cuando hay que
procesar mucho volumen con poca memoria y latencia predecible.

---

## Rust frente a otros lenguajes

| | **C/C++** | **Rust** | **Go** | **Python** |
| :--- | :--- | :--- | :--- | :--- |
| Gestión de memoria | Manual / RAII | Ownership (verificado por el compilador) | GC | GC + conteo de referencias |
| Seguridad de memoria | ❌ Responsabilidad del programador | ✅ Garantizada (salvo `unsafe`) | ✅ | ✅ |
| Data races | Posibles | ❌ Error de compilación | Posibles (detector en runtime) | Limitadas por el GIL |
| Rendimiento | Máximo | Máximo | Alto | Bajo (salvo extensiones nativas) |
| Curva de aprendizaje | Alta | **Alta al inicio** | Baja | Muy baja |
| Tiempo de compilación | Medio/alto | Alto | Muy bajo | — |
| Tooling | Fragmentado (CMake, Make, vcpkg…) | Unificado (`cargo`) | Unificado (`go`) | `pip` / `uv` / `poetry` |

Algunas precisiones honestas:

- Todo lo que se puede hacer en Rust se puede hacer en C++. La diferencia es **cuánto
  cuesta hacerlo bien y mantenerlo**: en Rust el compilador verifica invariantes que en
  C++ dependen de la disciplina del equipo. C++ conserva la ventaja de un ecosistema de
  librerías mucho más grande y maduro.
- **Go** suele dar mayor productividad inicial para APIs y microservicios: el lenguaje es
  pequeño y las goroutines hacen la concurrencia muy accesible. A cambio, tienes un GC y
  menos garantías en compilación.
- **Python** sigue siendo imbatible para prototipos y ciencia de datos. Rust y Python se
  complementan muy bien: con `PyO3` se escriben extensiones de Python en Rust.

---

## ¿Cuándo elegir Rust?

**Buenas señales:**

- El rendimiento o el consumo de memoria importan (servicios de alto tráfico, procesamiento
  de datos, sistemas embebidos, WebAssembly).
- El costo de un fallo es alto (infraestructura, seguridad, sistemas difíciles de reiniciar).
- Hay mucha concurrencia y los bugs intermitentes ya te han costado caro.
- Quieres distribuir un **binario único** sin runtime (CLIs, agentes, herramientas).

**Señales de que quizá no:**

- Estás validando una idea y la velocidad de iteración lo es todo.
- El equipo no tiene tiempo para absorber la curva de aprendizaje.
- El dominio depende de librerías que solo existen (maduras) en otro ecosistema.

---

## Cómo está organizado este curso

El curso dura **6 meses**, con 4 semanas por mes y proyectos que crecen de un mes a otro:

| Mes | Tema | Proyecto principal |
| :--- | :--- | :--- |
| [1](../chapter_01/section_00.md) | Fundamentos y *borrow checker* | Todo List CLI |
| [2](../chapter_02/section_00.md) | Genéricos, traits y testing | Crate `config-loader` |
| [3](../chapter_03/section_00.md) | Async y ecosistema web | API *Url Shortener* |
| [4](../chapter_04/section_00.md) | Sistemas, CLI y WebAssembly | CLI, wrapper FFI, Mandelbrot en Wasm, *Log Parser* |
| [5](../chapter_05/section_00.md) | Arquitectura y rendimiento | Refactor y optimización de los proyectos anteriores |
| [6](../chapter_06/section_00.md) | Proyecto final (*capstone*) | Servicio completo para tu portafolio |

Cada mes empieza con una **guía de estudio** con el plan semanal y sigue con una sección
por semana que termina en un **mini-reto** y una **checklist**.

---

## ✅ Checklist antes de empezar

- [ ] Puedo explicar con mis palabras qué significa "seguridad de memoria sin GC".
- [ ] Sé qué tipo de proyectos se benefician de Rust y cuáles no.
- [ ] Tengo claro que la curva inicial es empinada y que el compilador es un aliado,
  no un enemigo.

> **Siguiente paso:** [Instalación y primeros pasos](../chapter_01/section_01.md).
