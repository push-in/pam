#ifndef PAM_SHARED_H
#define PAM_SHARED_H

/*
 * Shared tables and task workers: implemented in Rust (src/native_shared.rs)
 * and handed to the PHP glue as a table of function pointers before PHP
 * starts. Rows are opaque byte ranges to Rust; this glue owns the column
 * layout and the conversion to and from PHP values.
 */

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define PAM_SHARED_OPS_VERSION 1u

typedef struct {
    uint32_t offset;
    uint32_t length;
    const unsigned char *bytes;
} pam_shared_patch;

typedef struct {
    const unsigned char *data;
    size_t length;
} pam_shared_bytes;

typedef struct {
    uint32_t version;
    void *(*table_open)(
        const char *name, size_t name_length, uint64_t rows, uint32_t key_size,
        uint32_t value_size, uint64_t schema, char **error);
    void (*table_close)(void *table);
    /* 1 stored, 0 table full, -1 error */
    int (*table_set)(
        void *table, const char *key, size_t key_length,
        const pam_shared_patch *patches, size_t count, char **error);
    int (*table_get)(
        void *table, const char *key, size_t key_length,
        unsigned char *output, size_t output_length);
    int (*table_exists)(void *table, const char *key, size_t key_length);
    int (*table_del)(void *table, const char *key, size_t key_length);
    uint64_t (*table_count)(void *table);
    /* 1 updated, 0 table full, -1 error */
    int (*table_incr)(
        void *table, const char *key, size_t key_length, uint32_t offset,
        int is_float, int64_t int_delta, double float_delta,
        int64_t *int_result, double *float_result, char **error);
    /* Rows as [u32 key length][key][value]; *cursor >= size at the end. */
    size_t (*table_scan)(
        void *table, uint64_t *cursor, size_t limit,
        unsigned char **output, size_t *output_length);
    void (*table_info)(
        void *table, uint64_t *capacity, uint64_t *memory, uint64_t *recoveries);
    int (*task_available)(void);
    /* results[i].data == NULL: still running when the wait ended. */
    int (*task_resolve)(
        const pam_shared_bytes *payloads, size_t count, uint64_t wait_ms,
        pam_shared_bytes *results, char **error);
    int (*task_dispatch)(const pam_shared_bytes *payloads, size_t count, char **error);
    size_t (*task_flush)(uint64_t timeout_ms);
    void (*free_buffer)(unsigned char *data, size_t length);
    void (*free_string)(char *value);
} pam_shared_ops;

void pam_php_register_shared_ops(const pam_shared_ops *ops);

#ifdef __cplusplus
}
#endif

#endif
