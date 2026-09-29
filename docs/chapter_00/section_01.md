# Curso de Rust

## ¿Por qué Rust?

Hoy en día los sistemas "data intensive" o "compute intensive" son cada vez más utilizados por el gran escalamiento de los proyectos, muchas veces es mejor utilizar un lenguaje como python para desarrollar un MVP, y una vez que la prueba de concepto está funcionando empezar a migrar los sistemas poco a poco a lenguajes más robustos, conforme la demanda de cómputo se incremente. Para ello el proyecto inicial debe crearse con servicios desacoplados o empezar a desacoplar los servicios, lo cual siempre es más costoso en instancias posteriores, sin embargo muy común.

Rust nos permite suplir esta demanda de sistemas "data intensive" o "compute intensive" que requieren más desarrollo de lado del sistema, usualmente la elección era C/C++, sin embargo cuando el desarrollo conlleva la creación de complejos sistemas C/C++ se convierte en un cuello de botella, hay varias razones para ello,

- retrocompatibilidad de compilador, implica largos tiempos de compilación para modificaciones sencillas,
- manejo eficiente de la memoria, y patrones laziness provocan muchos errores, al acceder a memoria,
- uso de antiguas librerías que dejaron de tener soporte,
- ...

usualmente las mejoras no están completamente definidas,  muchos errores de codificación se cometen llevando a errores difíciles de trackear elevando el costo de desarrollo y mantenimiento del sistema.

Rust es un lenguaje de programación multiparadigma, enfocado principalmente en performance y seguridad, específicamente en concurrencia, este es uno de los grandes problemas de la programación hoy en día, varias técnicas han sido implementadas para sacar el poder de tener multiprocesadores, sin embargo estas han fallado con el tiempo, o han mostrado que es bastante complejo desarrollar aplicaciones genéricas, es cuando fue más obvia la necesidad de lenguajes que fueran seguros respecto a la memoria o hilos (memory o thread-safe).

La sintaxis en Rust es similar a la de C++; sin embargo, tenemos una alternativa a garbage collection que veremos más adelante. Rust nació como proyecto personal de Graydon Hoare y fue patrocinado por Mozilla, que lo usó para construir el motor experimental Servo y partes de Firefox; fue tan efectivo que ahora se utiliza en lugar de C/C++ en muchos otros sistemas, no es que sea exactamente mejor o más rápido, en general todo lo que podemos hacer con Rust lo podemos hacer con C++, C++ tiene  la ventaja de que hay miles de librerías, sin embargo, Rust permite un desarrollo más sustentable, donde se abarata el mantenimiento e inclusión de nuevos features a nuestros sistemas, aquí un punto importante es que en Rust seguro no existen referencias nulas (la ausencia de valor se modela con `Option<T>`), lo cual en el desarrollo diario con C/C++ siempre es el origen de muchos problemas recurrentes.

Por el momento nos enfocaremos en aprender el lenguaje y en otra instancia veremos ejemplos de aplicaciones donde el poder de Rust aparece naturalmente (spoiler alert DATA!). Aunque podemos usarlo para la creación de APIs, otros lenguajes como Go pueden ofrecer mayor productividad inicial: la simpleza del lenguaje junto con las goroutines permite un desarrollo rápido, a costa de un recolector de basura y menos garantías en tiempo de compilación.
