# Curso de Rust

## Por que Rust?

Hoy en dia los sistemas "data intesive" o "compute intesive" son cada vez más utilizados por el gran escalamiento de los proyectos, muchas veces es mejor utilizar un lenguaje como python para desarrollar un MVP, y una vez que la prueba de concepto esta funcionando empezar a migrar los sistemas poco a poco a lenguajes más robustos, conforme la demanda de computo se incremente. Para ello el proyecto incial debe crearse con servicios desacoplados o empezar a desacomplar los servicios, lo cual siempre es más costoso en instancias posteriores, sin embargo muy común.

Rust nos permite suplir esta demanda de sistemas "data intesive" o "compute intesive" que requieren más desarrollo de lado del sistema, usualmente la elección era C/C++, sin embargo cuando el desarrollo con lleva la creación de complejos sistemas C/C++ se convierte en un cuello de botella, hay varias razones para ello,

- retrocompatibilidad de compilador, implica largos tiempos de compilación para modificaciones sencillas,
- manejo eficiente de la memoria, y patrones laziness provocan muchos errores, al accesar memoria,
- uso de antiguas librerias qque dejaron de tener soporte,
- ...

usualmente las mejoras no están completamente definidas,  muchos errores de codificación se comenten llevando a errores dificiles de trackear elevando el costo de desarrollo y mantenimiento del sistema.

Rust es un lenguaje de programación multi paradigma, enfocado principalmente en performance y seguridad, especificamente en concurrencia, este es uno de los grandes problemas de la programación hoy en día, varias tecnicas han sido implementadas para sacar el poder de tener multiprocesadores, sin embargo estas han fallado con el tiempo, o han mostrado que es bastante complejo hacer desarrollar aplicaciones genericas, es cuando fue más obvia la necesidad de lenguajes que fueran seguros respecto a la memoria o hilos(memory o thread-safe).

La sintaxis en Rust es similar  la de C++, sin membargo tenemos una alternativa a garbage collection que veremos más adelante. Este lenguaje fue desarrollado para mejorar el explorador Modzilla, fue tan efectivo que ahora se empezo a utilizar en lugar de C/C++ en muchos otros sistemas, no es que sea exactamente mejor o más rapido, en general todo lo que podemos hacer con Rust lo podemos hacer con C++, C++ tiene  la ventaja de que hay miles de librerias, sin embargo, Rust permite un desarollo más sustentable, donde se abarata el mantenimiento e inclusion de nuevos features a nuestros sistemas, aqui un punto importante es que Rust no permite punteros nulos, lo cual en el desarrollo diario con C/C++ siempre es el origen de muchos problemas recurrentes.

Por el momento nos enfocaremos en aprender el lenguaje y en otra instancia veremos ejemplos de aplicaciones donde el poder de Rust aparece naturalmente(spoiler alert DATA!). Aunque podemos usarlo para la creación de APIs otros lenguajes podrian tener un mejor desempeño como Golang, donde la simpleza del lenaguaje junto con las gorountines permite un buen y rapido desarrollo.
