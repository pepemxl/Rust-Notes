#ifndef BUFFER_H
#define BUFFER_H

#include <stddef.h>
#include <stdint.h>

/* Códigos de error */
#define BUF_OK          0
#define BUF_ERR_NULL   -1
#define BUF_ERR_RANGE  -2
#define BUF_ERR_ALLOC  -3

/* Handle opaco — los usuarios nunca ven la estructura interna */
typedef struct BufHandle BufHandle;

/* Crear un buffer de `len` bytes inicializados a cero.
 * Devuelve NULL si falla la asignación de memoria. */
BufHandle* buf_crear(size_t len);

/* Liberar un buffer. Es seguro pasar NULL. */
void buf_liberar(BufHandle* b);

/* Longitud del buffer en bytes. */
size_t buf_len(const BufHandle* b);

/* Escribir `n` bytes de `data` en posición `offset`.
 * Devuelve BUF_OK o código de error. */
int buf_escribir(BufHandle* b, size_t offset, const uint8_t* data, size_t n);

/* Leer `n` bytes desde posición `offset` hacia `dest`.
 * Devuelve BUF_OK o código de error. */
int buf_leer(const BufHandle* b, size_t offset, uint8_t* dest, size_t n);

/* Aplicar XOR con `clave` a todo el buffer (cifrado/descifrado simétrico).
 * Devuelve BUF_OK o código de error. */
int buf_xor(BufHandle* b, uint8_t clave);

/* Borrar el contenido del buffer (poner a cero).
 * Devuelve BUF_OK o BUF_ERR_NULL. */
int buf_limpiar(BufHandle* b);

#endif /* BUFFER_H */
