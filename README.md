# Rust-Notes

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


## Temario
1. Introducción a Rust
    1. [Instalación de Rust y primeros pasos](./chapter_01/section_01.md)
        - Hello World
        - Imprimiendo combinaciones de cadenas de caracteres
        - Imprimiendo una gran cantidad de lineas de texto
        - Imprimiendo numeros enteros
        - Comentando el código
    2. [Aritmética en Rust](./chapter_01/section_02.md)
        1. Sumando números enteros
        2. Otros operadores de números enteros
        3. Aritmetica de punto flotante
        4. Rompiendo cadenas de caracteres
2. Nombrando Objetos
    1. Asociando nombres a valores
    2. Variables mutables
    3. Variables mutables no mutadas
    4. Variables no inicializadas
    5. Prefijo underscore
    6. Valores Boolean
    7. Expresiones Boolean
    8. Consistencia de tipo en asignaciones
    9. Inferencia de tipos
    10. Cambio de tipo en mutabilidad
    11. Composición de operadores de asignación
    12. Usando la funciones de la librería estandar
4. Controlando el flujo de ejecución
    1. Estructura de control if
    2. Expresiones condicionales
    3. Estructura de control iterativa while
    4. Estructura de control iterativa infinita loop
    5. Estructura de control iterativa con contador for
    6. Alcance de variables
5. Uso de secuencia de datos
    1. Arreglos
    2. Atributos en Rust
    3. Panicking
    4. Arreglos mutables
    5. Arreglos de tamaño explicitamente especificado
    6. Arreglos multidimensionales
    7. Vectors
    8. Operaciones en vectores
    9. Arreglos y vectores vacios
    10. Depuracion print
    11. Copiando arreglos y vectores
6. Uso de tipos primitivos
    1. Bases no decimales
    2. El prefijo underscore en literales numericos
    3. Notación exponencial
    4. Tipos de enteros con signo
    5. Tipos de enteros sin signo
    6. Tipos de enteros con objetivo dependiente
    7. Tipos inferidos
    8. Algoritmo de inferencia de tipos
    9. Tipo de números de punto flotante
    10. Conversiones explicitas
    11. Tipos de sufijos de literales numericas
    12. Todos los tipos numericos
    13. Booleans y caracteres
    14. La tupla vacia
    15. Arreglos y vectores
    16. Constantes
    17. Descubriendo el tipo de una expresión
7. Enumeraciones y matching
    1. Enumeraciones
    2. El constructor match
    3. Operadores relacionales y Enums
    4. Manejando todos los casos
    5. Usando match con números
    6. Enumeraciones con Data
    7. Declaración match con variables en patrones
    8. Expresiones match
    9. Uso de Guards en constructores match
    10. Constructores if-let y while-let
8. Uso de estructura de datos heterogenas
    1. Las tuplas
    2. Las estructuras
    3. La estructura tupla
    4. Convenciones lexicas
9. Definición de funciones
    1. Definición e invocación de funciones
    2. Definición de funciones después de su uso
    3. Funciones shadowing otras funciones
    4. Pasando argumentos a una función
    5. Pasando argumentos por valor
    6. Retornando un valor de una función
    7. Salida prematura
    8. Retornando muchos valores
    9. Como cambiar una variable proviniente del invocador
    10. Pasando argumentos por referencia
    11. Usar Referencias
    12. Mutabilidad de referencias
10. Definición de funciones genericas y tipos



