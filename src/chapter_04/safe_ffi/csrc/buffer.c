#include "buffer.h"
#include <stdlib.h>
#include <string.h>

struct BufHandle {
    uint8_t* data;
    size_t   len;
};

BufHandle* buf_crear(size_t len) {
    BufHandle* b = (BufHandle*)malloc(sizeof(BufHandle));
    if (!b) return NULL;
    b->data = (uint8_t*)calloc(len, 1);  /* inicializado a cero */
    if (!b->data) { free(b); return NULL; }
    b->len = len;
    return b;
}

void buf_liberar(BufHandle* b) {
    if (!b) return;
    /* Borrar memoria antes de liberar (evitar que secretos queden en heap) */
    memset(b->data, 0, b->len);
    free(b->data);
    free(b);
}

size_t buf_len(const BufHandle* b) {
    if (!b) return 0;
    return b->len;
}

int buf_escribir(BufHandle* b, size_t offset, const uint8_t* data, size_t n) {
    if (!b || !data) return BUF_ERR_NULL;
    if (offset + n > b->len) return BUF_ERR_RANGE;
    memcpy(b->data + offset, data, n);
    return BUF_OK;
}

int buf_leer(const BufHandle* b, size_t offset, uint8_t* dest, size_t n) {
    if (!b || !dest) return BUF_ERR_NULL;
    if (offset + n > b->len) return BUF_ERR_RANGE;
    memcpy(dest, b->data + offset, n);
    return BUF_OK;
}

int buf_xor(BufHandle* b, uint8_t clave) {
    if (!b) return BUF_ERR_NULL;
    for (size_t i = 0; i < b->len; i++) b->data[i] ^= clave;
    return BUF_OK;
}

int buf_limpiar(BufHandle* b) {
    if (!b) return BUF_ERR_NULL;
    memset(b->data, 0, b->len);
    return BUF_OK;
}
