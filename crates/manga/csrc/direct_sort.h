#ifndef MANGA_DIRECT_SORT_H
#define MANGA_DIRECT_SORT_H

#include <stddef.h>
#include <stdint.h>

/* Result codes returned by dsj_sort_jpeg. Keep in sync with src/viewer/giga/direct_sort.rs. */
enum {
    DSJ_OK = 0,
    DSJ_UNSUPPORTED = 1, /* valid-looking input outside the supported subset */
    DSJ_MALFORMED = 2,   /* corrupt, truncated, or otherwise invalid JPEG */
    DSJ_LIMIT = 3,       /* representation or libjpeg limit, or allocation failure */
    DSJ_INTERNAL = 4,    /* unexpected failure inside the shim */
    DSJ_INVALID_ARG = 5  /* caller error: bad tile size, permutation or grid out of bounds */
};

/*
 * Restores the GigaViewer 4x4 tile scramble directly on JPEG DCT coefficients.
 *
 * On DSJ_OK, *out is a malloc'd buffer of *out_len bytes that must be released with dsj_free.
 * On failure, *out is NULL and msg (NUL-terminated, msg_cap bytes) describes the error.
 * All libjpeg errors are handled with setjmp/longjmp inside this translation unit.
 * Thread-safe: no global state.
 */
int dsj_sort_jpeg(const uint8_t *in, size_t in_len, uint8_t **out, size_t *out_len,
                  char *msg, size_t msg_cap);

/*
 * Applies an arbitrary 4x4 tile permutation on DCT coefficients. Tiles are tile_width x tile_height
 * pixels anchored at the origin; both must be multiples of 8 (else DSJ_UNSUPPORTED). perm[d] is the
 * source tile for destination tile d, row-major; perm_len must be 16 and perm a bijection on 0..15,
 * and the 4x4 grid must fit in the image, else DSJ_INVALID_ARG. Same input policy and output
 * ownership as dsj_sort_jpeg. Pixels outside the grid are untouched.
 */
int dsj_permute_jpeg(const uint8_t *in, size_t in_len, uint32_t tile_width, uint32_t tile_height,
                     const uint8_t *perm, size_t perm_len, uint8_t **out, size_t *out_len,
                     char *msg, size_t msg_cap);

void dsj_free(void *ptr);

#endif
