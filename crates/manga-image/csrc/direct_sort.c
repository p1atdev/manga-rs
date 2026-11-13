/*
 * Lossless tile unscrambling/permutation (GigaViewer, Magapoke) on JPEG DCT coefficients (libjpeg API, mozjpeg-sys).
 *
 * Every libjpeg call happens in this file. libjpeg errors longjmp back to dsj_run (conversion) or
 * dsj_cleanup (destruction); both frames stay active for as long as libjpeg can run, so a longjmp
 * never targets a returned frame, and none crosses into Rust. All mutable state lives in a
 * heap-allocated context so cleanup is uniform.
 *
 * Output grows with checked arithmetic and is bounded only by the addressable object size.
 */
#include <setjmp.h>
#include <limits.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <jpeglib.h>
#include <jerror.h>

#include "direct_sort.h"

#if defined(_MSC_VER)
#define DSJ_NORETURN __declspec(noreturn)
#else
#define DSJ_NORETURN __attribute__((noreturn))
#endif

/* The ICC profile sequence/count fields are one byte each. */
#define DSJ_MAX_ICC_CHUNKS 255u
#define DSJ_NUM_CELLS 4u
#define DSJ_DIVISIBLE_WITH 8u

typedef struct {
    struct jpeg_error_mgr pub; /* must be first */
    jmp_buf jb;  /* conversion frame (dsj_run) */
    jmp_buf cjb; /* cleanup frame (dsj_cleanup) */
    int cleaning;
    int cleanup_failed;
    int status;
    char msg[JMSG_LENGTH_MAX];
} dsj_err;

typedef struct {
    dsj_err err;
    struct jpeg_decompress_struct src;
    struct jpeg_compress_struct dst;
    struct jpeg_destination_mgr dest;
    int src_ready;
    int dst_ready;
    const uint8_t *in;
    size_t in_len;
    unsigned char *outbuf;
    size_t outcap;
    size_t outlen;
    JBLOCK *tmp_a;
    JBLOCK *tmp_b;
    int giga;             /* derive tile size and the transpose permutation from the image */
    size_t tile_w_blocks; /* tile size in 8x8 blocks */
    size_t tile_h_blocks;
    uint8_t perm[DSJ_NUM_CELLS * DSJ_NUM_CELLS]; /* destination cell -> source cell */
    jpeg_saved_marker_ptr icc[DSJ_MAX_ICC_CHUNKS];
    unsigned icc_count;
} dsj_ctx;

DSJ_NORETURN static void dsj_throw(dsj_ctx *c, int status, const char *message)
{
    snprintf(c->err.msg, sizeof c->err.msg, "%s", message);
    c->err.status = status;
    longjmp(c->err.jb, 1);
}

/*
 * During cleanup the original status/message are preserved and the jump goes to the cleanup frame,
 * so an error raised while destroying can neither clobber the result nor recurse.
 */
DSJ_NORETURN static void dsj_fail_from_libjpeg(j_common_ptr cinfo, int status)
{
    dsj_err *e = (dsj_err *)cinfo->err;
    if (e->cleaning) {
        e->cleanup_failed = 1;
        longjmp(e->cjb, 1);
    }
    (*cinfo->err->format_message)(cinfo, e->msg);
    e->status = status;
    longjmp(e->jb, 1);
}

static void dsj_error_exit(j_common_ptr cinfo)
{
    int status = DSJ_MALFORMED;
    switch (cinfo->err->msg_code) {
    case JERR_BAD_PRECISION:
    case JERR_SOF_UNSUPPORTED:
        status = DSJ_UNSUPPORTED;
        break;
    case JERR_OUT_OF_MEMORY:
    case JERR_NO_BACKING_STORE:
    case JERR_WIDTH_OVERFLOW:
    case JERR_IMAGE_TOO_BIG:
        status = DSJ_LIMIT;
        break;
    case JERR_BAD_LIB_VERSION:
    case JERR_BAD_STRUCT_SIZE:
        status = DSJ_INTERNAL;
        break;
    default:
        break;
    }
    dsj_fail_from_libjpeg(cinfo, status);
}

/* Any libjpeg warning (premature EOF, extraneous bytes, corrupt data...) is fatal: no silent repair. */
static void dsj_emit_message(j_common_ptr cinfo, int msg_level)
{
    if (msg_level < 0 && !((dsj_err *)cinfo->err)->cleaning) {
        dsj_fail_from_libjpeg(cinfo, DSJ_MALFORMED);
    }
}

static void dsj_output_message(j_common_ptr cinfo)
{
    (void)cinfo; /* never print to stderr */
}

static int dsj_mul(size_t a, size_t b, size_t *out)
{
    if (a != 0 && b > SIZE_MAX / a) {
        return 0;
    }
    *out = a * b;
    return 1;
}

/* Returns the SOFn marker byte found before SOS, or 0 if none could be determined. */
static int dsj_find_sof(const uint8_t *p, size_t n)
{
    size_t i = 2;
    while (i + 4 <= n) {
        uint8_t m;
        size_t seg;
        if (p[i] != 0xFF) {
            return 0;
        }
        m = p[i + 1];
        if (m == 0xFF) {
            i += 1;
            continue;
        }
        if (m == 0x01 || m == 0xD8 || (m >= 0xD0 && m <= 0xD7)) {
            i += 2;
            continue;
        }
        if (m == 0xD9 || m == 0xDA) {
            return 0;
        }
        seg = ((size_t)p[i + 2] << 8) | p[i + 3];
        if (seg < 2) {
            return 0;
        }
        if (m >= 0xC0 && m <= 0xCF && m != 0xC4 && m != 0xC8 && m != 0xCC) {
            return m;
        }
        i += 2 + seg;
    }
    return 0;
}

static void dsj_check_header(dsj_ctx *c)
{
    int i;

    if (c->src.data_precision != 8) {
        dsj_throw(c, DSJ_UNSUPPORTED, "only 8-bit JPEG is supported");
    }
    if (c->src.progressive_mode || c->src.arith_code) {
        dsj_throw(c, DSJ_UNSUPPORTED, "only baseline Huffman JPEG is supported");
    }
    if (c->src.saw_Adobe_marker) {
        dsj_throw(c, DSJ_UNSUPPORTED, "Adobe APP14 marker is not supported");
    }
    if (c->src.num_components == 1) {
        if (c->src.jpeg_color_space != JCS_GRAYSCALE) {
            dsj_throw(c, DSJ_UNSUPPORTED, "unsupported grayscale color space");
        }
    } else if (c->src.num_components == 3) {
        if (c->src.jpeg_color_space != JCS_YCbCr) {
            dsj_throw(c, DSJ_UNSUPPORTED, "only YCbCr 3-component JPEG is supported");
        }
    } else {
        dsj_throw(c, DSJ_UNSUPPORTED, "only grayscale or 3-component JPEG is supported");
    }
    for (i = 0; i < c->src.num_components; i++) {
        if (c->src.comp_info[i].h_samp_factor != 1 || c->src.comp_info[i].v_samp_factor != 1) {
            dsj_throw(c, DSJ_UNSUPPORTED, "only 4:4:4 (no chroma subsampling) is supported");
        }
    }
}

/*
 * Metadata policy: the only accepted segments are one leading JFIF APP0 without a thumbnail
 * (regenerated by libjpeg on output) and a complete, well-formed ICC_PROFILE APP2 sequence
 * (copied verbatim, in sequence order). Anything else (EXIF, XMP, COM, thumbnails, Adobe, ...)
 * is rejected as unsupported rather than copied: orientation or thumbnails would go stale.
 */
static void dsj_check_markers(dsj_ctx *c)
{
    jpeg_saved_marker_ptr m;
    int previous_marker = 0;
    unsigned jfif = 0;
    unsigned expected = 0;
    unsigned seen = 0;

    for (m = c->src.marker_list; m != NULL; m = m->next) {
        if (m->original_length != m->data_length) {
            dsj_throw(c, DSJ_UNSUPPORTED, "truncated metadata segment");
        }
        if (m->marker == JPEG_APP0 && m->data_length >= 5 && memcmp(m->data, "JFIF", 5) == 0) {
            if (previous_marker || jfif) {
                dsj_throw(c, DSJ_UNSUPPORTED, "JFIF marker must appear exactly once, first");
            }
            if (m->data_length != 14 || m->data[5] != 1 || m->data[12] != 0 || m->data[13] != 0) {
                dsj_throw(c, DSJ_UNSUPPORTED, "JFIF with thumbnail or unknown version is not supported");
            }
            jfif = 1;
        } else if (m->marker == JPEG_APP0 + 2 && m->data_length >= 12 &&
                   memcmp(m->data, "ICC_PROFILE", 12) == 0) {
            unsigned seq;
            unsigned cnt;
            if (m->data_length <= 14) {
                dsj_throw(c, DSJ_MALFORMED, "empty or truncated ICC profile chunk");
            }
            seq = m->data[12];
            cnt = m->data[13];
            if (cnt == 0 || seq == 0 || seq > cnt) {
                dsj_throw(c, DSJ_MALFORMED, "invalid ICC profile chunk numbering");
            }
            if (seen == 0) {
                expected = cnt;
            } else if (cnt != expected) {
                dsj_throw(c, DSJ_MALFORMED, "inconsistent ICC profile chunk count");
            }
            if (c->icc[seq - 1] != NULL) {
                dsj_throw(c, DSJ_MALFORMED, "duplicate ICC profile chunk");
            }
            c->icc[seq - 1] = m;
            seen++;
        } else {
            dsj_throw(c, DSJ_UNSUPPORTED, "unsupported metadata segment (only JFIF and ICC are kept)");
        }
        previous_marker = 1;
    }
    if (seen != expected) {
        dsj_throw(c, DSJ_MALFORMED, "incomplete ICC profile");
    }
    if ((c->src.saw_JFIF_marker != 0) != (jfif != 0)) {
        dsj_throw(c, DSJ_UNSUPPORTED, "inconsistent JFIF marker state");
    }
    c->icc_count = expected;
}

static void dsj_dest_init(j_compress_ptr cinfo)
{
    dsj_ctx *c = (dsj_ctx *)cinfo->err;
    c->outlen = 0;
    if (c->outbuf == NULL) {
        /* libjpeg's emit_byte decrements free_in_buffer first, so it must start non-zero. */
        c->outbuf = (unsigned char *)malloc((size_t)64 * 1024);
        if (c->outbuf == NULL) {
            dsj_throw(c, DSJ_LIMIT, "out of memory");
        }
        c->outcap = (size_t)64 * 1024;
    }
    cinfo->dest->next_output_byte = c->outbuf;
    cinfo->dest->free_in_buffer = c->outcap;
}

/* Grow without overflowing size_t or exceeding Rust's maximum addressable slice length. */
static boolean dsj_dest_empty(j_compress_ptr cinfo)
{
    dsj_ctx *c = (dsj_ctx *)cinfo->err;
    size_t newcap;
    unsigned char *grown;

    if (c->outcap >= (size_t)PTRDIFF_MAX) {
        dsj_throw(c, DSJ_LIMIT, "output exceeds addressable size");
    }
    newcap = c->outcap > (size_t)PTRDIFF_MAX / 2
                 ? (size_t)PTRDIFF_MAX : c->outcap * 2;
    grown = (unsigned char *)realloc(c->outbuf, newcap); /* on failure the old buffer stays owned */
    if (grown == NULL) {
        dsj_throw(c, DSJ_LIMIT, "out of memory");
    }
    /* libjpeg calls this only once the whole buffer is full, so the old capacity is all data. */
    cinfo->dest->next_output_byte = grown + c->outcap;
    cinfo->dest->free_in_buffer = newcap - c->outcap;
    c->outbuf = grown;
    c->outcap = newcap;
    return TRUE;
}

static void dsj_dest_term(j_compress_ptr cinfo)
{
    dsj_ctx *c = (dsj_ctx *)cinfo->err;
    c->outlen = c->outcap - cinfo->dest->free_in_buffer;
}

static JBLOCKROW dsj_row(dsj_ctx *c, jvirt_barray_ptr array, JDIMENSION row)
{
    JBLOCKARRAY rows = (*c->src.mem->access_virt_barray)((j_common_ptr)&c->src, array, row, 1, TRUE);
    return rows[0];
}

/*
 * Moves whole 8x8 coefficient blocks so that, for every destination cell d of the 4x4 grid anchored
 * at the origin, the cell at c->perm[d] (row-major) is copied there. Cells are tile_w x tile_h
 * blocks. Blocks outside the grid (right/bottom strips included) are never touched. The
 * permutation is applied cycle by cycle, one block row at a time, staged through two row-segment
 * buffers, so no row pointer is held across another virtual-array access and extra memory is two
 * tile rows. Fixed points are skipped.
 */
static void dsj_permute_tiles(dsj_ctx *c, jvirt_barray_ptr *arrays)
{
    size_t cw = c->tile_w_blocks;
    size_t ch = c->tile_h_blocks;
    size_t bytes;
    int ci;
    unsigned start;

    if (cw == 0 || ch == 0) {
        return;
    }
    if (!dsj_mul(cw, sizeof(JBLOCK), &bytes)) {
        dsj_throw(c, DSJ_LIMIT, "tile size overflow");
    }
    c->tmp_a = (JBLOCK *)malloc(bytes);
    c->tmp_b = (JBLOCK *)malloc(bytes);
    if (c->tmp_a == NULL || c->tmp_b == NULL) {
        dsj_throw(c, DSJ_LIMIT, "out of memory");
    }

    for (ci = 0; ci < c->src.num_components; ci++) {
        jpeg_component_info *comp = &c->src.comp_info[ci];
        if ((uint64_t)DSJ_NUM_CELLS * cw > comp->width_in_blocks ||
            (uint64_t)DSJ_NUM_CELLS * ch > comp->height_in_blocks) {
            dsj_throw(c, DSJ_INTERNAL, "tile grid exceeds coefficient array");
        }
        for (start = 0; start < DSJ_NUM_CELLS * DSJ_NUM_CELLS; start++) {
            unsigned d;
            unsigned s;
            size_t y;
            int lowest = 1; /* handle each cycle once, from its smallest member */
            for (d = c->perm[start]; d != start; d = c->perm[d]) {
                if (d < start) {
                    lowest = 0;
                    break;
                }
            }
            if (!lowest || c->perm[start] == start) {
                continue;
            }
            for (y = 0; y < ch; y++) {
                JBLOCKROW row;
                d = start;
                row = dsj_row(c, arrays[ci], (JDIMENSION)((d / DSJ_NUM_CELLS) * ch + y));
                memcpy(c->tmp_a, row + (d % DSJ_NUM_CELLS) * cw, bytes);
                for (;;) {
                    s = c->perm[d];
                    if (s == start) {
                        row = dsj_row(c, arrays[ci], (JDIMENSION)((d / DSJ_NUM_CELLS) * ch + y));
                        memcpy(row + (d % DSJ_NUM_CELLS) * cw, c->tmp_a, bytes);
                        break;
                    }
                    row = dsj_row(c, arrays[ci], (JDIMENSION)((s / DSJ_NUM_CELLS) * ch + y));
                    memcpy(c->tmp_b, row + (s % DSJ_NUM_CELLS) * cw, bytes);
                    row = dsj_row(c, arrays[ci], (JDIMENSION)((d / DSJ_NUM_CELLS) * ch + y));
                    memcpy(row + (d % DSJ_NUM_CELLS) * cw, c->tmp_b, bytes);
                    d = s;
                }
            }
        }
    }
}

static void dsj_write(dsj_ctx *c, jvirt_barray_ptr *arrays)
{
    unsigned s;

    c->dst_ready = 1;
    jpeg_create_compress(&c->dst);
    /* Prefer mozjpeg's fastest (baseline) profile so defaults do not pick progressive scans. */
    jpeg_c_set_int_param(&c->dst, JINT_COMPRESS_PROFILE, JCP_FASTEST);
    c->dest.init_destination = dsj_dest_init;
    c->dest.empty_output_buffer = dsj_dest_empty;
    c->dest.term_destination = dsj_dest_term;
    c->dst.dest = &c->dest;

    /* Copies dimensions, sampling, component ids and the quantization tables verbatim. */
    jpeg_copy_critical_parameters(&c->src, &c->dst);
    c->dst.progressive_mode = FALSE;
    c->dst.scan_info = NULL;
    c->dst.num_scans = 0;
    c->dst.arith_code = FALSE;
    c->dst.optimize_coding = TRUE;
    c->dst.write_Adobe_marker = FALSE;
    c->dst.write_JFIF_header = c->src.saw_JFIF_marker ? TRUE : FALSE;
    if (c->src.saw_JFIF_marker) {
        c->dst.JFIF_major_version = c->src.JFIF_major_version;
        c->dst.JFIF_minor_version = c->src.JFIF_minor_version;
        c->dst.density_unit = c->src.density_unit;
        c->dst.X_density = c->src.X_density;
        c->dst.Y_density = c->src.Y_density;
    }

    jpeg_write_coefficients(&c->dst, arrays);
    for (s = 0; s < c->icc_count; s++) {
        jpeg_saved_marker_ptr m = c->icc[s];
        jpeg_write_marker(&c->dst, JPEG_APP0 + 2, m->data, m->data_length);
    }
    jpeg_finish_compress(&c->dst);
}

static void dsj_body(dsj_ctx *c)
{
    jvirt_barray_ptr *arrays;
    int sof;
    int m;

    if (c->in_len == 0) {
        dsj_throw(c, DSJ_MALFORMED, "empty input");
    }
    if (c->in_len > (size_t)PTRDIFF_MAX) {
        dsj_throw(c, DSJ_LIMIT, "input exceeds addressable size");
    }
#if SIZE_MAX > ULONG_MAX
    if (c->in_len > (size_t)ULONG_MAX) {
        dsj_throw(c, DSJ_LIMIT, "input exceeds jpeg_mem_src length representation");
    }
#endif
    if (c->in_len < 2 || c->in[0] != 0xFF || c->in[1] != 0xD8) {
        dsj_throw(c, c->in_len < 2 ? DSJ_MALFORMED : DSJ_UNSUPPORTED, "not a JPEG stream");
    }
    sof = dsj_find_sof(c->in, c->in_len);
    if (sof != 0 && sof != 0xC0) {
        dsj_throw(c, DSJ_UNSUPPORTED, "only baseline (SOF0) JPEG is supported");
    }

    c->src_ready = 1;
    jpeg_create_decompress(&c->src);
    jpeg_mem_src(&c->src, (unsigned char *)c->in, (unsigned long)c->in_len);
    for (m = 0; m < 16; m++) {
        jpeg_save_markers(&c->src, JPEG_APP0 + m, 0xFFFF);
    }
    jpeg_save_markers(&c->src, JPEG_COM, 0xFFFF);

    if (jpeg_read_header(&c->src, TRUE) != JPEG_HEADER_OK) {
        dsj_throw(c, DSJ_MALFORMED, "invalid JPEG header");
    }
    dsj_check_header(c);

    if (c->giga) {
        /* Mirrors Solver::solve_buffer: dim / 32 blocks per cell, cells (i, j) <-> (j, i). */
        unsigned d;
        c->tile_w_blocks = (size_t)(c->src.image_width / (DSJ_NUM_CELLS * DSJ_DIVISIBLE_WITH));
        c->tile_h_blocks = (size_t)(c->src.image_height / (DSJ_NUM_CELLS * DSJ_DIVISIBLE_WITH));
        for (d = 0; d < DSJ_NUM_CELLS * DSJ_NUM_CELLS; d++) {
            c->perm[d] = (uint8_t)((d % DSJ_NUM_CELLS) * DSJ_NUM_CELLS + d / DSJ_NUM_CELLS);
        }
    } else if ((uint64_t)DSJ_NUM_CELLS * c->tile_w_blocks * DSJ_DIVISIBLE_WITH > c->src.image_width ||
               (uint64_t)DSJ_NUM_CELLS * c->tile_h_blocks * DSJ_DIVISIBLE_WITH > c->src.image_height) {
        dsj_throw(c, DSJ_INVALID_ARG, "tile grid exceeds image bounds");
    }

    arrays = jpeg_read_coefficients(&c->src);
    if (arrays == NULL) {
        dsj_throw(c, DSJ_MALFORMED, "could not read JPEG coefficients");
    }
    /* EOI has been consumed; every marker is now in the list. Nothing may follow it. */
    if (c->src.src->bytes_in_buffer != 0) {
        dsj_throw(c, DSJ_UNSUPPORTED, "trailing data after EOI");
    }
    dsj_check_markers(c);

    dsj_permute_tiles(c, arrays);
    dsj_write(c, arrays);

    if (c->outbuf == NULL || c->outlen == 0) {
        dsj_throw(c, DSJ_INTERNAL, "output size out of range");
    }
    (void)jpeg_finish_decompress(&c->src);
}

static int dsj_run(dsj_ctx *c)
{
    dsj_ctx *volatile vc = c;
    if (setjmp(vc->err.jb) != 0) {
        return vc->err.status;
    }
    dsj_body(vc);
    return DSJ_OK;
}

/*
 * Destroys both libjpeg objects. Each destroy runs under a setjmp frame belonging to this function,
 * which is still active while libjpeg can raise an error. The ready flag is cleared before the call
 * so a failed destroy is never retried (its memory pool is leaked instead). In cleanup mode the
 * error handler never overwrites the result status/message and never targets the conversion frame.
 */
static void dsj_cleanup(dsj_ctx *volatile c)
{
    c->err.cleaning = 1;
    /* setjmp is only used as the whole controlling expression, as the C standard requires. */
    if (setjmp(c->err.cjb) == 0) {
        if (c->dst_ready) {
            c->dst_ready = 0;
            jpeg_destroy_compress(&c->dst);
        }
    }
    if (setjmp(c->err.cjb) == 0) {
        if (c->src_ready) {
            c->src_ready = 0;
            jpeg_destroy_decompress(&c->src);
        }
    }
    c->err.cleaning = 0;
}

static void dsj_set_msg(char *msg, size_t cap, const char *text)
{
    if (msg != NULL && cap > 0) {
        snprintf(msg, cap, "%s", text);
    }
}

static int dsj_convert(const uint8_t *in, size_t in_len, int giga, size_t tile_w_blocks,
                       size_t tile_h_blocks, const uint8_t *perm, uint8_t **out, size_t *out_len,
                       char *msg, size_t msg_cap)
{
    dsj_ctx *c;
    int status;

    if (out == NULL || out_len == NULL) {
        return DSJ_INTERNAL;
    }
    *out = NULL;
    *out_len = 0;
    dsj_set_msg(msg, msg_cap, "");

    c = (dsj_ctx *)calloc(1, sizeof *c);
    if (c == NULL) {
        dsj_set_msg(msg, msg_cap, "out of memory");
        return DSJ_LIMIT;
    }
    c->giga = giga;
    c->tile_w_blocks = tile_w_blocks;
    c->tile_h_blocks = tile_h_blocks;
    if (perm != NULL) {
        memcpy(c->perm, perm, sizeof c->perm);
    }
    c->in = in;
    c->in_len = in_len;
    jpeg_std_error(&c->err.pub);
    c->err.pub.error_exit = dsj_error_exit;
    c->err.pub.emit_message = dsj_emit_message;
    c->err.pub.output_message = dsj_output_message;
    c->src.err = &c->err.pub;
    c->dst.err = &c->err.pub;

    status = dsj_run(c);

    dsj_cleanup(c);
    free(c->tmp_a);
    free(c->tmp_b);
    if (status == DSJ_OK && c->err.cleanup_failed) {
        status = DSJ_INTERNAL;
        snprintf(c->err.msg, sizeof c->err.msg, "libjpeg cleanup failed");
    }
    if (status == DSJ_OK) {
        *out = c->outbuf;
        *out_len = c->outlen;
    } else {
        free(c->outbuf);
        dsj_set_msg(msg, msg_cap, c->err.msg);
    }
    free(c);
    return status;
}

/* Consume all scanlines to surface corrupt entropy, including premature EOI warnings.
 * This uses no transform-specific SOF, chroma, or metadata policy. */
static int dsj_validate_run(dsj_ctx *c)
{
    dsj_ctx *volatile vc = c;
    JSAMPARRAY row;
    JDIMENSION row_width;
    if (setjmp(vc->err.jb) != 0) {
        return vc->err.status;
    }
    if (vc->in_len == 0 || vc->in_len > (size_t)PTRDIFF_MAX) {
        dsj_throw(vc, vc->in_len == 0 ? DSJ_MALFORMED : DSJ_LIMIT, "invalid input size");
    }
#if SIZE_MAX > ULONG_MAX
    if (vc->in_len > (size_t)ULONG_MAX) {
        dsj_throw(vc, DSJ_LIMIT, "input exceeds jpeg_mem_src length representation");
    }
#endif
    vc->src_ready = 1;
    jpeg_create_decompress(&vc->src);
    jpeg_mem_src(&vc->src, (unsigned char *)vc->in, (unsigned long)vc->in_len);
    if (jpeg_read_header(&vc->src, TRUE) != JPEG_HEADER_OK) {
        dsj_throw(vc, DSJ_MALFORMED, "invalid JPEG header");
    }
    /* Validation consumes every coded MCU; only the discarded pixel output is scaled. */
    vc->src.scale_num = 1;
    vc->src.scale_denom = 8;
    if (!jpeg_start_decompress(&vc->src)) {
        dsj_throw(vc, DSJ_MALFORMED, "invalid JPEG stream");
    }
    if (vc->src.output_components <= 0 ||
        vc->src.output_width > UINT_MAX / (unsigned)vc->src.output_components) {
        dsj_throw(vc, DSJ_LIMIT, "scanline size overflow");
    }
    row_width = vc->src.output_width * (JDIMENSION)vc->src.output_components;
    row = (*vc->src.mem->alloc_sarray)((j_common_ptr)&vc->src, JPOOL_IMAGE, row_width, 1);
    while (vc->src.output_scanline < vc->src.output_height) {
        if (jpeg_read_scanlines(&vc->src, row, 1) != 1) {
            dsj_throw(vc, DSJ_MALFORMED, "incomplete JPEG scanline");
        }
    }
    if (!jpeg_finish_decompress(&vc->src)) {
        dsj_throw(vc, DSJ_MALFORMED, "incomplete JPEG stream");
    }
    return DSJ_OK;
}

int dsj_validate_jpeg(const uint8_t *in, size_t in_len, char *msg, size_t msg_cap)
{
    dsj_ctx *c = (dsj_ctx *)calloc(1, sizeof *c);
    int status;
    dsj_set_msg(msg, msg_cap, "");
    if (c == NULL) {
        dsj_set_msg(msg, msg_cap, "out of memory");
        return DSJ_LIMIT;
    }
    c->in = in;
    c->in_len = in_len;
    jpeg_std_error(&c->err.pub);
    c->err.pub.error_exit = dsj_error_exit;
    c->err.pub.emit_message = dsj_emit_message;
    c->err.pub.output_message = dsj_output_message;
    c->src.err = &c->err.pub;
    status = dsj_validate_run(c);
    dsj_cleanup(c);
    if (status == DSJ_OK && c->err.cleanup_failed) {
        status = DSJ_INTERNAL;
        snprintf(c->err.msg, sizeof c->err.msg, "libjpeg cleanup failed");
    }
    if (status != DSJ_OK) dsj_set_msg(msg, msg_cap, c->err.msg);
    free(c);
    return status;
}

int dsj_sort_jpeg(const uint8_t *in, size_t in_len, uint8_t **out, size_t *out_len,
                  char *msg, size_t msg_cap)
{
    return dsj_convert(in, in_len, 1, 0, 0, NULL, out, out_len, msg, msg_cap);
}

int dsj_permute_jpeg(const uint8_t *in, size_t in_len, uint32_t tile_width, uint32_t tile_height,
                     const uint8_t *perm, size_t perm_len, uint8_t **out, size_t *out_len,
                     char *msg, size_t msg_cap)
{
    unsigned char seen[DSJ_NUM_CELLS * DSJ_NUM_CELLS] = {0};
    size_t i;

    if (out == NULL || out_len == NULL) {
        return DSJ_INTERNAL;
    }
    *out = NULL;
    *out_len = 0;
    if (tile_width == 0 || tile_height == 0) {
        dsj_set_msg(msg, msg_cap, "tile dimensions must be nonzero");
        return DSJ_INVALID_ARG;
    }
    if (perm == NULL || perm_len != DSJ_NUM_CELLS * DSJ_NUM_CELLS) {
        dsj_set_msg(msg, msg_cap, "permutation must have exactly 16 entries");
        return DSJ_INVALID_ARG;
    }
    for (i = 0; i < perm_len; i++) {
        if (perm[i] >= DSJ_NUM_CELLS * DSJ_NUM_CELLS || seen[perm[i]]) {
            dsj_set_msg(msg, msg_cap, "permutation is not a bijection over 0..16");
            return DSJ_INVALID_ARG;
        }
        seen[perm[i]] = 1;
    }
    if (tile_width % DSJ_DIVISIBLE_WITH != 0 || tile_height % DSJ_DIVISIBLE_WITH != 0) {
        dsj_set_msg(msg, msg_cap, "tile dimensions are not multiples of 8");
        return DSJ_UNSUPPORTED;
    }
    return dsj_convert(in, in_len, 0, tile_width / DSJ_DIVISIBLE_WITH, tile_height / DSJ_DIVISIBLE_WITH,
                       perm, out, out_len, msg, msg_cap);
}

void dsj_free(void *ptr)
{
    free(ptr);
}
