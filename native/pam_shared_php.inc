/*
 * PHP functions for shared tables and task workers (included by pam_php.c).
 *
 * Every PHP value is converted before the Rust side takes a lock and after it
 * releases it, so a PHP error (memory limit, exception) can never leave a
 * shared lock held.
 */

#include <Zend/zend_exceptions.h>
#include <ext/spl/spl_exceptions.h>

#include "pam_shared.h"

#define PAM_TABLE_MAX_COLUMNS 256
#define PAM_TABLE_TYPE_INT 1
#define PAM_TABLE_TYPE_FLOAT 2
#define PAM_TABLE_TYPE_STRING 3

static const pam_shared_ops *pam_shared = NULL;

void pam_php_register_shared_ops(const pam_shared_ops *ops)
{
    if (ops != NULL && ops->version == PAM_SHARED_OPS_VERSION) {
        pam_shared = ops;
    }
}

typedef struct {
    char *name;
    size_t name_length;
    uint8_t type;
    uint32_t size;
    uint32_t offset;
} pam_table_column;

typedef struct {
    void *table;
    uint32_t key_size;
    uint32_t value_size;
    uint32_t column_count;
    pam_table_column *columns;
} pam_table_handle;

static pam_table_handle **pam_tables = NULL;
static size_t pam_table_slots = 0;

static bool pam_shared_ready(void)
{
    if (pam_shared == NULL) {
        zend_throw_exception(spl_ce_RuntimeException, "PAM shared tables are not available in this runtime", 0);
        return false;
    }
    return true;
}

static void pam_shared_throw(const char *prefix, char *error)
{
    zend_throw_exception_ex(spl_ce_RuntimeException, 0, "%s: %s", prefix, error != NULL ? error : "unknown error");
    if (error != NULL) {
        pam_shared->free_string(error);
    }
}

static void pam_table_free(pam_table_handle *handle)
{
    uint32_t index;

    if (handle == NULL) {
        return;
    }
    if (handle->table != NULL && pam_shared != NULL) {
        pam_shared->table_close(handle->table);
    }
    for (index = 0; index < handle->column_count; index++) {
        free(handle->columns[index].name);
    }
    free(handle->columns);
    free(handle);
}

static pam_table_handle *pam_table_from_id(zend_long id)
{
    if (id < 1 || (size_t) id > pam_table_slots || pam_tables[id - 1] == NULL) {
        zend_throw_exception(spl_ce_RuntimeException, "Invalid or closed PAM table", 0);
        return NULL;
    }
    return pam_tables[id - 1];
}

static const pam_table_column *pam_table_column_named(const pam_table_handle *handle, const char *name, size_t length)
{
    uint32_t index;

    for (index = 0; index < handle->column_count; index++) {
        const pam_table_column *column = &handle->columns[index];
        if (column->name_length == length && memcmp(column->name, name, length) == 0) {
            return column;
        }
    }
    return NULL;
}

static uint64_t pam_fnv(uint64_t hash, const void *data, size_t length)
{
    const unsigned char *bytes = data;
    size_t index;

    for (index = 0; index < length; index++) {
        hash = (hash ^ bytes[index]) * 0x100000001b3ULL;
    }
    return hash;
}

ZEND_BEGIN_ARG_INFO_EX(arginfo_pam_native_table_open, 0, 0, 3)
    ZEND_ARG_INFO(0, name)
    ZEND_ARG_INFO(0, rows)
    ZEND_ARG_INFO(0, columns)
    ZEND_ARG_INFO(0, key_size)
ZEND_END_ARG_INFO()

static PHP_FUNCTION(pam_native_table_open)
{
    zend_string *name;
    zend_long rows;
    HashTable *columns;
    zend_long key_size = 64;
    pam_table_handle *handle;
    zval *definition;
    uint32_t offset = 0;
    uint64_t schema = 0xcbf29ce484222325ULL;
    char *error = NULL;
    size_t slot;

    ZEND_PARSE_PARAMETERS_START(3, 4)
        Z_PARAM_STR(name)
        Z_PARAM_LONG(rows)
        Z_PARAM_ARRAY_HT(columns)
        Z_PARAM_OPTIONAL
        Z_PARAM_LONG(key_size)
    ZEND_PARSE_PARAMETERS_END();

    if (!pam_shared_ready()) {
        RETURN_THROWS();
    }
    if (zend_hash_num_elements(columns) > PAM_TABLE_MAX_COLUMNS) {
        zend_argument_value_error(3, "cannot declare more than %d columns", PAM_TABLE_MAX_COLUMNS);
        RETURN_THROWS();
    }
    if (rows < 1 || key_size < 1 || key_size > 1024) {
        zend_argument_value_error(rows < 1 ? 2 : 4, "is out of range");
        RETURN_THROWS();
    }

    handle = calloc(1, sizeof(*handle));
    if (handle == NULL) {
        zend_throw_exception(spl_ce_RuntimeException, "Out of memory", 0);
        RETURN_THROWS();
    }
    handle->columns = calloc(zend_hash_num_elements(columns) + 1, sizeof(pam_table_column));
    if (handle->columns == NULL) {
        free(handle);
        zend_throw_exception(spl_ce_RuntimeException, "Out of memory", 0);
        RETURN_THROWS();
    }

    ZEND_HASH_FOREACH_VAL(columns, definition) {
        zval *column_name, *column_type, *column_size;
        pam_table_column *column = &handle->columns[handle->column_count];
        zend_long type, size;

        ZVAL_DEREF(definition);
        if (Z_TYPE_P(definition) != IS_ARRAY
            || (column_name = zend_hash_index_find(Z_ARRVAL_P(definition), 0)) == NULL
            || (column_type = zend_hash_index_find(Z_ARRVAL_P(definition), 1)) == NULL) {
            pam_table_free(handle);
            zend_argument_value_error(3, "must be a list of [name, type, size] columns");
            RETURN_THROWS();
        }
        ZVAL_DEREF(column_name);
        ZVAL_DEREF(column_type);
        if (Z_TYPE_P(column_name) != IS_STRING
            || Z_STRLEN_P(column_name) == 0
            || Z_STRLEN_P(column_name) > 255
            || Z_TYPE_P(column_type) != IS_LONG) {
            pam_table_free(handle);
            zend_argument_value_error(3, "must be a list of [name, type, size] columns");
            RETURN_THROWS();
        }
        column_size = zend_hash_index_find(Z_ARRVAL_P(definition), 2);
        if (column_size != NULL) {
            ZVAL_DEREF(column_size);
        }
        type = Z_LVAL_P(column_type);
        size = column_size != NULL && Z_TYPE_P(column_size) == IS_LONG ? Z_LVAL_P(column_size) : 0;
        if (pam_table_column_named(handle, Z_STRVAL_P(column_name), Z_STRLEN_P(column_name)) != NULL) {
            pam_table_free(handle);
            zend_argument_value_error(3, "declares column \"%s\" twice", Z_STRVAL_P(column_name));
            RETURN_THROWS();
        }
        if (type == PAM_TABLE_TYPE_STRING) {
            if (size < 1 || size > 16 * 1024 * 1024) {
                pam_table_free(handle);
                zend_argument_value_error(3, "string column \"%s\" needs a size between 1 and 16777216 bytes", Z_STRVAL_P(column_name));
                RETURN_THROWS();
            }
            offset = (offset + 3u) & ~3u;
            column->size = (uint32_t) size;
            column->offset = offset;
            offset += 4u + (uint32_t) size;
        } else if (type == PAM_TABLE_TYPE_INT || type == PAM_TABLE_TYPE_FLOAT) {
            offset = (offset + 7u) & ~7u;
            column->size = 8;
            column->offset = offset;
            offset += 8u;
        } else {
            pam_table_free(handle);
            zend_argument_value_error(3, "column \"%s\" has an unknown type", Z_STRVAL_P(column_name));
            RETURN_THROWS();
        }
        if (offset > 16u * 1024u * 1024u) {
            pam_table_free(handle);
            zend_argument_value_error(3, "rows cannot exceed 16777216 bytes");
            RETURN_THROWS();
        }
        column->type = (uint8_t) type;
        column->name_length = Z_STRLEN_P(column_name);
        column->name = malloc(column->name_length + 1);
        if (column->name == NULL) {
            pam_table_free(handle);
            zend_throw_exception(spl_ce_RuntimeException, "Out of memory", 0);
            RETURN_THROWS();
        }
        memcpy(column->name, Z_STRVAL_P(column_name), column->name_length + 1);
        handle->column_count++;
        schema = pam_fnv(schema, column->name, column->name_length + 1);
        schema = pam_fnv(schema, &column->type, sizeof(column->type));
        schema = pam_fnv(schema, &column->size, sizeof(column->size));
    } ZEND_HASH_FOREACH_END();

    handle->key_size = (uint32_t) key_size;
    handle->value_size = (offset + 7u) & ~7u;
    handle->table = pam_shared->table_open(
        ZSTR_VAL(name), ZSTR_LEN(name), (uint64_t) rows, handle->key_size,
        handle->value_size, schema, &error);
    if (handle->table == NULL) {
        pam_table_free(handle);
        pam_shared_throw("Cannot create the PAM table", error);
        RETURN_THROWS();
    }

    for (slot = 0; slot < pam_table_slots; slot++) {
        if (pam_tables[slot] == NULL) {
            break;
        }
    }
    if (slot == pam_table_slots) {
        size_t slots = pam_table_slots == 0 ? 8 : pam_table_slots * 2;
        pam_table_handle **grown = realloc(pam_tables, slots * sizeof(*grown));
        if (grown == NULL) {
            pam_table_free(handle);
            zend_throw_exception(spl_ce_RuntimeException, "Out of memory", 0);
            RETURN_THROWS();
        }
        memset(grown + pam_table_slots, 0, (slots - pam_table_slots) * sizeof(*grown));
        pam_tables = grown;
        pam_table_slots = slots;
    }
    pam_tables[slot] = handle;
    RETURN_LONG((zend_long) slot + 1);
}

ZEND_BEGIN_ARG_INFO_EX(arginfo_pam_native_table_handle, 0, 0, 1)
    ZEND_ARG_INFO(0, table)
ZEND_END_ARG_INFO()

static PHP_FUNCTION(pam_native_table_close)
{
    zend_long id;

    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_LONG(id)
    ZEND_PARSE_PARAMETERS_END();

    RETVAL_NULL();
    if (id >= 1 && (size_t) id <= pam_table_slots && pam_tables[id - 1] != NULL) {
        pam_table_free(pam_tables[id - 1]);
        pam_tables[id - 1] = NULL;
    }
}

ZEND_BEGIN_ARG_INFO_EX(arginfo_pam_native_table_set, 0, 0, 3)
    ZEND_ARG_INFO(0, table)
    ZEND_ARG_INFO(0, key)
    ZEND_ARG_INFO(0, values)
ZEND_END_ARG_INFO()

static PHP_FUNCTION(pam_native_table_set)
{
    zend_long id;
    zend_string *key;
    HashTable *values;
    pam_table_handle *handle;
    pam_shared_patch patches[PAM_TABLE_MAX_COLUMNS];
    size_t count = 0;
    unsigned char *scratch;
    zend_string *column_name;
    zval *value;
    char *error = NULL;
    int status;

    ZEND_PARSE_PARAMETERS_START(3, 3)
        Z_PARAM_LONG(id)
        Z_PARAM_STR(key)
        Z_PARAM_ARRAY_HT(values)
    ZEND_PARSE_PARAMETERS_END();

    if (!pam_shared_ready() || (handle = pam_table_from_id(id)) == NULL) {
        RETURN_THROWS();
    }
    if (ZSTR_LEN(key) > handle->key_size) {
        zend_argument_value_error(2, "cannot be longer than %u bytes", handle->key_size);
        RETURN_THROWS();
    }
    scratch = emalloc(handle->value_size + 8);

    ZEND_HASH_FOREACH_STR_KEY_VAL(values, column_name, value) {
        const pam_table_column *column;
        unsigned char *target;

        if (column_name == NULL
            || (column = pam_table_column_named(handle, ZSTR_VAL(column_name), ZSTR_LEN(column_name))) == NULL
            || count >= PAM_TABLE_MAX_COLUMNS) {
            continue;
        }
        target = scratch + column->offset;
        if (column->type == PAM_TABLE_TYPE_INT) {
            int64_t number = (int64_t) zval_get_long(value);
            memcpy(target, &number, sizeof(number));
            patches[count].length = 8;
        } else if (column->type == PAM_TABLE_TYPE_FLOAT) {
            double number = zval_get_double(value);
            memcpy(target, &number, sizeof(number));
            patches[count].length = 8;
        } else {
            zend_string *text = zval_try_get_string(value);
            uint32_t length;
            if (text == NULL) {
                efree(scratch);
                RETURN_THROWS();
            }
            if (ZSTR_LEN(text) > column->size) {
                zend_value_error(
                    "Value for column \"%s\" is %zu bytes; the column holds %u",
                    column->name, ZSTR_LEN(text), column->size);
                zend_string_release(text);
                efree(scratch);
                RETURN_THROWS();
            }
            length = (uint32_t) ZSTR_LEN(text);
            memcpy(target, &length, sizeof(length));
            memcpy(target + 4, ZSTR_VAL(text), length);
            zend_string_release(text);
            patches[count].length = 4 + length;
        }
        patches[count].offset = column->offset;
        patches[count].bytes = target;
        count++;
    } ZEND_HASH_FOREACH_END();

    status = pam_shared->table_set(handle->table, ZSTR_VAL(key), ZSTR_LEN(key), patches, count, &error);
    efree(scratch);
    if (status < 0) {
        pam_shared_throw("Cannot write the PAM table", error);
        RETURN_THROWS();
    }
    RETURN_BOOL(status == 1);
}

static void pam_table_column_value(const pam_table_column *column, const unsigned char *row, zval *result)
{
    const unsigned char *cell = row + column->offset;

    if (column->type == PAM_TABLE_TYPE_INT) {
        int64_t number;
        memcpy(&number, cell, sizeof(number));
        ZVAL_LONG(result, (zend_long) number);
    } else if (column->type == PAM_TABLE_TYPE_FLOAT) {
        double number;
        memcpy(&number, cell, sizeof(number));
        ZVAL_DOUBLE(result, number);
    } else {
        uint32_t length;
        memcpy(&length, cell, sizeof(length));
        if (length > column->size) {
            length = column->size;
        }
        ZVAL_STRINGL(result, (const char *) cell + 4, length);
    }
}

static void pam_table_row_array(const pam_table_handle *handle, const unsigned char *row, zval *result)
{
    uint32_t index;

    array_init_size(result, handle->column_count);
    for (index = 0; index < handle->column_count; index++) {
        zval value;
        pam_table_column_value(&handle->columns[index], row, &value);
        zend_hash_str_add_new(Z_ARRVAL_P(result), handle->columns[index].name, handle->columns[index].name_length, &value);
    }
}

ZEND_BEGIN_ARG_INFO_EX(arginfo_pam_native_table_get, 0, 0, 2)
    ZEND_ARG_INFO(0, table)
    ZEND_ARG_INFO(0, key)
    ZEND_ARG_INFO(0, field)
ZEND_END_ARG_INFO()

static PHP_FUNCTION(pam_native_table_get)
{
    zend_long id;
    zend_string *key;
    zend_string *field = NULL;
    pam_table_handle *handle;
    const pam_table_column *column = NULL;
    unsigned char *row;

    ZEND_PARSE_PARAMETERS_START(2, 3)
        Z_PARAM_LONG(id)
        Z_PARAM_STR(key)
        Z_PARAM_OPTIONAL
        Z_PARAM_STR_OR_NULL(field)
    ZEND_PARSE_PARAMETERS_END();

    if (!pam_shared_ready() || (handle = pam_table_from_id(id)) == NULL) {
        RETURN_THROWS();
    }
    if (field != NULL
        && (column = pam_table_column_named(handle, ZSTR_VAL(field), ZSTR_LEN(field))) == NULL) {
        RETURN_FALSE;
    }
    if (ZSTR_LEN(key) > handle->key_size) {
        RETURN_FALSE;
    }
    row = emalloc(handle->value_size + 8);
    if (!pam_shared->table_get(handle->table, ZSTR_VAL(key), ZSTR_LEN(key), row, handle->value_size)) {
        efree(row);
        RETURN_FALSE;
    }
    if (column != NULL) {
        pam_table_column_value(column, row, return_value);
    } else {
        pam_table_row_array(handle, row, return_value);
    }
    efree(row);
}

ZEND_BEGIN_ARG_INFO_EX(arginfo_pam_native_table_key, 0, 0, 2)
    ZEND_ARG_INFO(0, table)
    ZEND_ARG_INFO(0, key)
ZEND_END_ARG_INFO()

static PHP_FUNCTION(pam_native_table_exists)
{
    zend_long id;
    zend_string *key;
    pam_table_handle *handle;

    ZEND_PARSE_PARAMETERS_START(2, 2)
        Z_PARAM_LONG(id)
        Z_PARAM_STR(key)
    ZEND_PARSE_PARAMETERS_END();

    if (!pam_shared_ready() || (handle = pam_table_from_id(id)) == NULL) {
        RETURN_THROWS();
    }
    RETURN_BOOL(ZSTR_LEN(key) <= handle->key_size
        && pam_shared->table_exists(handle->table, ZSTR_VAL(key), ZSTR_LEN(key)));
}

static PHP_FUNCTION(pam_native_table_del)
{
    zend_long id;
    zend_string *key;
    pam_table_handle *handle;

    ZEND_PARSE_PARAMETERS_START(2, 2)
        Z_PARAM_LONG(id)
        Z_PARAM_STR(key)
    ZEND_PARSE_PARAMETERS_END();

    if (!pam_shared_ready() || (handle = pam_table_from_id(id)) == NULL) {
        RETURN_THROWS();
    }
    RETURN_BOOL(ZSTR_LEN(key) <= handle->key_size
        && pam_shared->table_del(handle->table, ZSTR_VAL(key), ZSTR_LEN(key)));
}

static PHP_FUNCTION(pam_native_table_count)
{
    zend_long id;
    pam_table_handle *handle;

    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_LONG(id)
    ZEND_PARSE_PARAMETERS_END();

    if (!pam_shared_ready() || (handle = pam_table_from_id(id)) == NULL) {
        RETURN_THROWS();
    }
    RETURN_LONG((zend_long) pam_shared->table_count(handle->table));
}

ZEND_BEGIN_ARG_INFO_EX(arginfo_pam_native_table_incr, 0, 0, 4)
    ZEND_ARG_INFO(0, table)
    ZEND_ARG_INFO(0, key)
    ZEND_ARG_INFO(0, column)
    ZEND_ARG_INFO(0, by)
ZEND_END_ARG_INFO()

static PHP_FUNCTION(pam_native_table_incr)
{
    zend_long id;
    zend_string *key;
    zend_string *column_name;
    zval *by;
    pam_table_handle *handle;
    const pam_table_column *column;
    int64_t int_result = 0;
    double float_result = 0;
    char *error = NULL;
    int status;

    ZEND_PARSE_PARAMETERS_START(4, 4)
        Z_PARAM_LONG(id)
        Z_PARAM_STR(key)
        Z_PARAM_STR(column_name)
        Z_PARAM_NUMBER(by)
    ZEND_PARSE_PARAMETERS_END();

    if (!pam_shared_ready() || (handle = pam_table_from_id(id)) == NULL) {
        RETURN_THROWS();
    }
    column = pam_table_column_named(handle, ZSTR_VAL(column_name), ZSTR_LEN(column_name));
    if (column == NULL || column->type == PAM_TABLE_TYPE_STRING) {
        zend_argument_value_error(3, "must name an int or float column");
        RETURN_THROWS();
    }
    if (ZSTR_LEN(key) > handle->key_size) {
        zend_argument_value_error(2, "cannot be longer than %u bytes", handle->key_size);
        RETURN_THROWS();
    }
    status = pam_shared->table_incr(
        handle->table, ZSTR_VAL(key), ZSTR_LEN(key), column->offset,
        column->type == PAM_TABLE_TYPE_FLOAT,
        (int64_t) zval_get_long(by), zval_get_double(by),
        &int_result, &float_result, &error);
    if (status < 0) {
        pam_shared_throw("Cannot update the PAM table", error);
        RETURN_THROWS();
    }
    if (status == 0) {
        RETURN_FALSE;
    }
    if (column->type == PAM_TABLE_TYPE_FLOAT) {
        RETURN_DOUBLE(float_result);
    }
    RETURN_LONG((zend_long) int_result);
}

ZEND_BEGIN_ARG_INFO_EX(arginfo_pam_native_table_scan, 0, 0, 2)
    ZEND_ARG_INFO(0, table)
    ZEND_ARG_INFO(0, cursor)
    ZEND_ARG_INFO(0, limit)
ZEND_END_ARG_INFO()

/* Returns [next cursor (-1 at the end), list of [key, row]]. */
static PHP_FUNCTION(pam_native_table_scan)
{
    zend_long id;
    zend_long cursor;
    zend_long limit = 64;
    pam_table_handle *handle;
    uint64_t next;
    uint64_t capacity = 0, memory = 0, recoveries = 0;
    unsigned char *buffer = NULL;
    size_t buffer_length = 0;
    size_t rows, index, offset = 0;
    zval list;

    ZEND_PARSE_PARAMETERS_START(2, 3)
        Z_PARAM_LONG(id)
        Z_PARAM_LONG(cursor)
        Z_PARAM_OPTIONAL
        Z_PARAM_LONG(limit)
    ZEND_PARSE_PARAMETERS_END();

    if (!pam_shared_ready() || (handle = pam_table_from_id(id)) == NULL) {
        RETURN_THROWS();
    }
    if (cursor < 0 || limit < 1) {
        zend_argument_value_error(cursor < 0 ? 2 : 3, "must not be negative");
        RETURN_THROWS();
    }
    next = (uint64_t) cursor;
    rows = pam_shared->table_scan(handle->table, &next, (size_t) limit, &buffer, &buffer_length);
    pam_shared->table_info(handle->table, &capacity, &memory, &recoveries);

    array_init_size(&list, (uint32_t) rows);
    for (index = 0; index < rows && offset + 4 <= buffer_length; index++) {
        uint32_t key_length;
        zval pair, key, row;

        memcpy(&key_length, buffer + offset, sizeof(key_length));
        if (offset + 4 + key_length + handle->value_size > buffer_length) {
            break;
        }
        ZVAL_STRINGL(&key, (const char *) buffer + offset + 4, key_length);
        pam_table_row_array(handle, buffer + offset + 4 + key_length, &row);
        array_init_size(&pair, 2);
        add_next_index_zval(&pair, &key);
        add_next_index_zval(&pair, &row);
        add_next_index_zval(&list, &pair);
        offset += 4 + key_length + handle->value_size;
    }
    if (buffer != NULL) {
        pam_shared->free_buffer(buffer, buffer_length);
    }
    array_init_size(return_value, 2);
    add_next_index_long(return_value, next >= capacity ? -1 : (zend_long) next);
    add_next_index_zval(return_value, &list);
}

static PHP_FUNCTION(pam_native_table_info)
{
    zend_long id;
    pam_table_handle *handle;
    uint64_t capacity = 0, memory = 0, recoveries = 0;

    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_LONG(id)
    ZEND_PARSE_PARAMETERS_END();

    if (!pam_shared_ready() || (handle = pam_table_from_id(id)) == NULL) {
        RETURN_THROWS();
    }
    pam_shared->table_info(handle->table, &capacity, &memory, &recoveries);
    array_init_size(return_value, 4);
    add_assoc_long(return_value, "size", (zend_long) capacity);
    add_assoc_long(return_value, "memory_size", (zend_long) memory);
    add_assoc_long(return_value, "key_size", (zend_long) handle->key_size);
    add_assoc_long(return_value, "recoveries", (zend_long) recoveries);
}

ZEND_BEGIN_ARG_INFO_EX(arginfo_pam_native_task_available, 0, 0, 0)
ZEND_END_ARG_INFO()

static PHP_FUNCTION(pam_native_task_available)
{
    ZEND_PARSE_PARAMETERS_NONE();
    RETURN_BOOL(pam_shared != NULL && pam_shared->task_available());
}

static bool pam_task_payloads(HashTable *input, pam_shared_bytes **payloads, size_t *count)
{
    zval *payload;
    size_t index = 0;

    *count = zend_hash_num_elements(input);
    *payloads = safe_emalloc(*count + 1, sizeof(pam_shared_bytes), 0);
    ZEND_HASH_FOREACH_VAL(input, payload) {
        ZVAL_DEREF(payload);
        if (Z_TYPE_P(payload) != IS_STRING) {
            efree(*payloads);
            zend_argument_type_error(1, "must be a list of strings");
            return false;
        }
        (*payloads)[index].data = (const unsigned char *) Z_STRVAL_P(payload);
        (*payloads)[index].length = Z_STRLEN_P(payload);
        index++;
    } ZEND_HASH_FOREACH_END();
    return true;
}

ZEND_BEGIN_ARG_INFO_EX(arginfo_pam_native_task_resolve, 0, 0, 2)
    ZEND_ARG_INFO(0, payloads)
    ZEND_ARG_INFO(0, wait_ms)
ZEND_END_ARG_INFO()

/* Returns a list with the result string of each task, or null when it was
 * still running at the end of the wait. */
static PHP_FUNCTION(pam_native_task_resolve)
{
    HashTable *input;
    zend_long wait_ms;
    pam_shared_bytes *payloads;
    pam_shared_bytes *results;
    size_t count, index;
    char *error = NULL;

    ZEND_PARSE_PARAMETERS_START(2, 2)
        Z_PARAM_ARRAY_HT(input)
        Z_PARAM_LONG(wait_ms)
    ZEND_PARSE_PARAMETERS_END();

    if (!pam_shared_ready()) {
        RETURN_THROWS();
    }
    if (!pam_task_payloads(input, &payloads, &count)) {
        RETURN_THROWS();
    }
    results = ecalloc(count + 1, sizeof(pam_shared_bytes));
    if (pam_shared->task_resolve(payloads, count, wait_ms < 0 ? 0 : (uint64_t) wait_ms, results, &error) != 0) {
        efree(payloads);
        efree(results);
        pam_shared_throw("Cannot run the tasks", error);
        RETURN_THROWS();
    }
    array_init_size(return_value, (uint32_t) count);
    for (index = 0; index < count; index++) {
        if (results[index].data == NULL) {
            add_next_index_null(return_value);
            continue;
        }
        add_next_index_stringl(return_value, (const char *) results[index].data, results[index].length);
        pam_shared->free_buffer((unsigned char *) results[index].data, results[index].length);
    }
    efree(payloads);
    efree(results);
}

ZEND_BEGIN_ARG_INFO_EX(arginfo_pam_native_task_dispatch, 0, 0, 1)
    ZEND_ARG_INFO(0, payloads)
ZEND_END_ARG_INFO()

static PHP_FUNCTION(pam_native_task_dispatch)
{
    HashTable *input;
    pam_shared_bytes *payloads;
    size_t count;
    char *error = NULL;
    int status;

    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_ARRAY_HT(input)
    ZEND_PARSE_PARAMETERS_END();

    RETVAL_NULL();
    if (!pam_shared_ready()) {
        RETURN_THROWS();
    }
    if (!pam_task_payloads(input, &payloads, &count)) {
        RETURN_THROWS();
    }
    status = pam_shared->task_dispatch(payloads, count, &error);
    efree(payloads);
    if (status != 0) {
        pam_shared_throw("Cannot dispatch the tasks", error);
        RETURN_THROWS();
    }
}

ZEND_BEGIN_ARG_INFO_EX(arginfo_pam_native_task_flush, 0, 0, 0)
    ZEND_ARG_INFO(0, timeout_ms)
ZEND_END_ARG_INFO()

static PHP_FUNCTION(pam_native_task_flush)
{
    zend_long timeout_ms = 5000;

    ZEND_PARSE_PARAMETERS_START(0, 1)
        Z_PARAM_OPTIONAL
        Z_PARAM_LONG(timeout_ms)
    ZEND_PARSE_PARAMETERS_END();

    if (pam_shared == NULL) {
        RETURN_LONG(0);
    }
    RETURN_LONG((zend_long) pam_shared->task_flush(timeout_ms < 0 ? 0 : (uint64_t) timeout_ms));
}

#define PAM_SHARED_FUNCTION_ENTRIES \
    PHP_FE(pam_native_table_open, arginfo_pam_native_table_open) \
    PHP_FE(pam_native_table_close, arginfo_pam_native_table_handle) \
    PHP_FE(pam_native_table_set, arginfo_pam_native_table_set) \
    PHP_FE(pam_native_table_get, arginfo_pam_native_table_get) \
    PHP_FE(pam_native_table_exists, arginfo_pam_native_table_key) \
    PHP_FE(pam_native_table_del, arginfo_pam_native_table_key) \
    PHP_FE(pam_native_table_count, arginfo_pam_native_table_handle) \
    PHP_FE(pam_native_table_incr, arginfo_pam_native_table_incr) \
    PHP_FE(pam_native_table_scan, arginfo_pam_native_table_scan) \
    PHP_FE(pam_native_table_info, arginfo_pam_native_table_handle) \
    PHP_FE(pam_native_task_available, arginfo_pam_native_task_available) \
    PHP_FE(pam_native_task_resolve, arginfo_pam_native_task_resolve) \
    PHP_FE(pam_native_task_dispatch, arginfo_pam_native_task_dispatch) \
    PHP_FE(pam_native_task_flush, arginfo_pam_native_task_flush)
