#ifndef PAM_PG_H
#define PAM_PG_H

/*
 * Postgres sessions for the pam-pg database driver: implemented in Rust
 * (src/native_pg.rs, src/pg_client.rs) and handed to the PHP glue as a table
 * of function pointers before PHP starts. Result cells point into Rust-owned
 * buffers until result_free.
 */

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define PAM_PG_OPS_VERSION 1u

typedef struct {
    const char *data;
    size_t length;
} pam_pg_str;

typedef struct {
    /* NULL: positional. */
    const char *name;
    size_t name_length;
    const char *value;
    size_t value_length;
    /* 0 NULL, 1 text, 2 binary */
    int kind;
} pam_pg_param;

typedef struct {
    const char *name;
    size_t name_length;
    uint32_t type_oid;
} pam_pg_column;

typedef struct {
    const unsigned char *data;
    /* -1: SQL NULL */
    ptrdiff_t length;
} pam_pg_cell;

typedef struct {
    void *owner;
    const pam_pg_column *columns;
    size_t column_count;
    /* row_count * column_count cells, row by row */
    const pam_pg_cell *cells;
    size_t row_count;
    uint64_t affected;
} pam_pg_result;

typedef struct {
    char sqlstate[6];
    char *message;
} pam_pg_error;

typedef struct {
    uint32_t version;
    /* 0 on failure (error filled) */
    uint64_t (*connect)(const pam_pg_str *keys, const pam_pg_str *values, size_t count, pam_pg_error *error);
    void (*close)(uint64_t handle);
    /* 1 ok (result filled), 0 failure (error filled) */
    int (*execute)(
        uint64_t handle, const char *sql, size_t sql_length, const pam_pg_param *params,
        size_t count, int simple, pam_pg_result *result, pam_pg_error *error);
    void (*result_free)(void *owner);
    /* Closes the server-side statement of sql (with the next command). */
    void (*release)(uint64_t handle, const char *sql, size_t sql_length);
    /* operation: 0 begin, 1 commit, 2 rollback; 1 ok, 0 failure */
    int (*transaction)(uint64_t handle, int operation, pam_pg_error *error);
    /* -1 closed or unusable, 0 idle, 1 in transaction, 2 failed transaction */
    int (*status)(uint64_t handle);
    /* 1 found (*value allocated, free with free_string), 0 absent */
    int (*parameter)(uint64_t handle, const char *name, size_t name_length, char **value);
    void (*free_string)(char *value);
} pam_pg_ops;

void pam_php_register_pg_ops(const pam_pg_ops *ops);

#ifdef __cplusplus
}
#endif

#endif
