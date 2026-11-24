/*
 * PHP functions of the pam-pg Postgres driver (included by pam_php.c).
 *
 * pam_pg_connect(array $options): int
 * pam_pg_close(int $session): void
 * pam_pg_execute(int $session, string $sql, array $params = [], int $flags = 0): array|int
 *     [int $affected, list<string> $columns, list<array> $rows]; with
 *     PAM_PG_SIMPLE the affected rows only (simple query protocol, no
 *     parameters). Rows are keyed by column name, or by position with
 *     PAM_PG_NUM. Values come back as pdo_pgsql returns them: int for
 *     int2/int4/int8/oid, float for float4/float8, bool for bool, a
 *     read-only memory stream for bytea, strings otherwise.
 *     Parameters: null, bool, int, float, string or Stringable, keyed by
 *     position or by name (`name` or `:name`); a one-element array [string]
 *     is sent as a binary value (PDO::PARAM_LOB).
 * pam_pg_release(int $session, string $sql): void  (closes the statement prepared for $sql)
 * pam_pg_transaction(int $session, int $operation): void  (0 begin, 1 commit, 2 rollback)
 * pam_pg_status(int $session): int  (-1 unusable, 0 idle, 1 in transaction, 2 failed)
 * pam_pg_parameter(int $session, string $name): ?string
 *
 * Failures throw PDOException (RuntimeException without ext-pdo) carrying the
 * SQLSTATE as code and errorInfo, the way pdo_pgsql reports them.
 */

#include "pam_pg.h"

#define PAM_PG_SIMPLE 1
#define PAM_PG_NUM 2

static const pam_pg_ops *pam_pg = NULL;

void pam_php_register_pg_ops(const pam_pg_ops *ops)
{
    if (ops != NULL && ops->version == PAM_PG_OPS_VERSION) {
        pam_pg = ops;
    }
}

static bool pam_pg_ready(void)
{
    if (pam_pg == NULL) {
        zend_throw_exception(spl_ce_RuntimeException, "The pam-pg driver is not available in this runtime", 0);
        return false;
    }
    return true;
}

static void pam_pg_throw(const char *sqlstate, const char *message)
{
    zend_string *name = zend_string_init("PDOException", sizeof("PDOException") - 1, 0);
    zend_class_entry *ce = zend_lookup_class_ex(name, NULL, ZEND_FETCH_CLASS_NO_AUTOLOAD);
    zend_string_release(name);
    bool pdo = ce != NULL;
    if (ce == NULL) {
        ce = spl_ce_RuntimeException;
    }
    zend_object *exception = zend_throw_exception_ex(ce, 0, "SQLSTATE[%s]: %s", sqlstate, message);
    if (exception == NULL || !pdo) {
        return;
    }
    zval code;
    ZVAL_STRING(&code, sqlstate);
    zend_update_property_ex(zend_ce_exception, exception, ZSTR_KNOWN(ZEND_STR_CODE), &code);
    zval_ptr_dtor(&code);
    zval info;
    array_init_size(&info, 3);
    add_next_index_string(&info, sqlstate);
    /* pdo_pgsql's driver code for a failed command (PGRES_FATAL_ERROR). */
    add_next_index_long(&info, 7);
    add_next_index_string(&info, message);
    zend_update_property(ce, exception, "errorInfo", sizeof("errorInfo") - 1, &info);
    zval_ptr_dtor(&info);
}

static void pam_pg_throw_error(pam_pg_error *error)
{
    error->sqlstate[5] = '\0';
    pam_pg_throw(error->sqlstate[0] != '\0' ? error->sqlstate : "HY000", error->message != NULL ? error->message : "unknown error");
    if (error->message != NULL) {
        pam_pg->free_string(error->message);
        error->message = NULL;
    }
}

static bool pam_pg_handle(zend_long handle, uint64_t *out)
{
    if (handle <= 0) {
        pam_pg_throw("08003", "The Postgres session is closed.");
        return false;
    }
    *out = (uint64_t) handle;
    return true;
}

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_pam_pg_connect, 0, 1, IS_LONG, 0)
    ZEND_ARG_TYPE_INFO(0, options, IS_ARRAY, 0)
ZEND_END_ARG_INFO()

static PHP_FUNCTION(pam_pg_connect)
{
    HashTable *options;
    zend_string *key;
    zval *value;

    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_ARRAY_HT(options)
    ZEND_PARSE_PARAMETERS_END();

    if (!pam_pg_ready()) {
        RETURN_THROWS();
    }
    uint32_t count = zend_hash_num_elements(options);
    pam_pg_str *keys = safe_emalloc(count + 1, sizeof(pam_pg_str), 0);
    pam_pg_str *values = safe_emalloc(count + 1, sizeof(pam_pg_str), 0);
    zend_string **held = safe_emalloc(count + 1, sizeof(zend_string *), 0);
    size_t used = 0;
    bool failed = false;
    ZEND_HASH_FOREACH_STR_KEY_VAL(options, key, value) {
        if (key == NULL) {
            zend_argument_value_error(1, "must be keyed by libpq option names");
            failed = true;
            break;
        }
        zend_string *text = zval_try_get_string(value);
        if (text == NULL) {
            failed = true;
            break;
        }
        keys[used].data = ZSTR_VAL(key);
        keys[used].length = ZSTR_LEN(key);
        values[used].data = ZSTR_VAL(text);
        values[used].length = ZSTR_LEN(text);
        held[used] = text;
        used++;
    } ZEND_HASH_FOREACH_END();

    uint64_t handle = 0;
    pam_pg_error error = {{0}, NULL};
    if (!failed) {
        handle = pam_pg->connect(keys, values, used, &error);
    }
    for (size_t index = 0; index < used; index++) {
        zend_string_release(held[index]);
    }
    efree(held);
    efree(values);
    efree(keys);
    if (failed) {
        RETURN_THROWS();
    }
    if (handle == 0) {
        pam_pg_throw_error(&error);
        RETURN_THROWS();
    }
    RETURN_LONG((zend_long) handle);
}

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_pam_pg_close, 0, 1, IS_VOID, 0)
    ZEND_ARG_TYPE_INFO(0, session, IS_LONG, 0)
ZEND_END_ARG_INFO()

static PHP_FUNCTION(pam_pg_close)
{
    zend_long handle;

    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_LONG(handle)
    ZEND_PARSE_PARAMETERS_END();

    (void) return_value;
    if (pam_pg != NULL && handle > 0) {
        pam_pg->close((uint64_t) handle);
    }
}

/* Parses a decimal integer cell (no NUL terminator). */
static zend_long pam_pg_long(const unsigned char *data, ptrdiff_t length)
{
    char buffer[32];
    if (length <= 0 || length >= (ptrdiff_t) sizeof(buffer)) {
        return 0;
    }
    memcpy(buffer, data, (size_t) length);
    buffer[length] = '\0';
    return ZEND_STRTOL(buffer, NULL, 10);
}

static double pam_pg_double(const unsigned char *data, ptrdiff_t length)
{
    char buffer[64];
    if (length == 8 && memcmp(data, "Infinity", 8) == 0) {
        return ZEND_INFINITY;
    }
    if (length == 9 && memcmp(data, "-Infinity", 9) == 0) {
        return -ZEND_INFINITY;
    }
    if (length == 3 && memcmp(data, "NaN", 3) == 0) {
        return ZEND_NAN;
    }
    if (length <= 0 || length >= (ptrdiff_t) sizeof(buffer)) {
        return 0.0;
    }
    memcpy(buffer, data, (size_t) length);
    buffer[length] = '\0';
    return zend_strtod(buffer, NULL);
}

static int pam_pg_hex(unsigned char digit)
{
    if (digit >= '0' && digit <= '9') {
        return digit - '0';
    }
    if (digit >= 'a' && digit <= 'f') {
        return digit - 'a' + 10;
    }
    if (digit >= 'A' && digit <= 'F') {
        return digit - 'A' + 10;
    }
    return -1;
}

/* bytea text output (hex `\x...` or the escape format) as raw bytes. */
static zend_string *pam_pg_bytea(const unsigned char *data, size_t length)
{
    if (length >= 2 && data[0] == '\\' && data[1] == 'x') {
        zend_string *out = zend_string_alloc((length - 2) / 2, 0);
        size_t written = 0;
        for (size_t index = 2; index + 1 < length; index += 2) {
            int high = pam_pg_hex(data[index]);
            int low = pam_pg_hex(data[index + 1]);
            if (high < 0 || low < 0) {
                break;
            }
            ZSTR_VAL(out)[written++] = (char) (high * 16 + low);
        }
        ZSTR_LEN(out) = written;
        ZSTR_VAL(out)[written] = '\0';
        return out;
    }
    zend_string *out = zend_string_alloc(length, 0);
    size_t written = 0;
    for (size_t index = 0; index < length; index++) {
        if (data[index] == '\\' && index + 1 < length && data[index + 1] == '\\') {
            ZSTR_VAL(out)[written++] = '\\';
            index++;
        } else if (data[index] == '\\' && index + 3 < length
            && data[index + 1] >= '0' && data[index + 1] <= '3'
            && data[index + 2] >= '0' && data[index + 2] <= '7'
            && data[index + 3] >= '0' && data[index + 3] <= '7') {
            ZSTR_VAL(out)[written++] = (char) (((data[index + 1] - '0') << 6) | ((data[index + 2] - '0') << 3) | (data[index + 3] - '0'));
            index += 3;
        } else {
            ZSTR_VAL(out)[written++] = (char) data[index];
        }
    }
    ZSTR_LEN(out) = written;
    ZSTR_VAL(out)[written] = '\0';
    return out;
}

static void pam_pg_value(zval *out, const pam_pg_cell *cell, uint32_t type_oid)
{
    if (cell->length < 0) {
        ZVAL_NULL(out);
        return;
    }
    switch (type_oid) {
        case 16: /* bool */
            ZVAL_BOOL(out, cell->length > 0 && cell->data[0] == 't');
            return;
        case 20: /* int8 */
        case 21: /* int2 */
        case 23: /* int4 */
        case 26: /* oid */
            ZVAL_LONG(out, pam_pg_long(cell->data, cell->length));
            return;
        case 700: /* float4 */
        case 701: /* float8 */
            ZVAL_DOUBLE(out, pam_pg_double(cell->data, cell->length));
            return;
        case 17: { /* bytea: a stream, as pdo_pgsql returns it */
            zend_string *bytes = pam_pg_bytea(cell->data, (size_t) cell->length);
            php_stream *stream = php_stream_memory_open(TEMP_STREAM_READONLY, bytes);
            zend_string_release(bytes);
            if (stream == NULL) {
                ZVAL_NULL(out);
                return;
            }
            php_stream_to_zval(stream, out);
            return;
        }
        default:
            ZVAL_STRINGL_FAST(out, (const char *) cell->data, (size_t) cell->length);
            return;
    }
}

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_MASK_EX(arginfo_pam_pg_execute, 0, 2, MAY_BE_ARRAY | MAY_BE_LONG)
    ZEND_ARG_TYPE_INFO(0, session, IS_LONG, 0)
    ZEND_ARG_TYPE_INFO(0, sql, IS_STRING, 0)
    ZEND_ARG_TYPE_INFO_WITH_DEFAULT_VALUE(0, params, IS_ARRAY, 0, "[]")
    ZEND_ARG_TYPE_INFO_WITH_DEFAULT_VALUE(0, flags, IS_LONG, 0, "0")
ZEND_END_ARG_INFO()

static PHP_FUNCTION(pam_pg_execute)
{
    zend_long session;
    zend_string *sql;
    HashTable *params = NULL;
    zend_long flags = 0;
    zend_ulong index;
    zend_string *key;
    zval *value;

    ZEND_PARSE_PARAMETERS_START(2, 4)
        Z_PARAM_LONG(session)
        Z_PARAM_STR(sql)
        Z_PARAM_OPTIONAL
        Z_PARAM_ARRAY_HT(params)
        Z_PARAM_LONG(flags)
    ZEND_PARSE_PARAMETERS_END();

    uint64_t handle;
    if (!pam_pg_ready() || !pam_pg_handle(session, &handle)) {
        RETURN_THROWS();
    }
    uint32_t count = params != NULL ? zend_hash_num_elements(params) : 0;
    if ((flags & PAM_PG_SIMPLE) && count > 0) {
        zend_argument_value_error(3, "must be empty for a simple query");
        RETURN_THROWS();
    }
    pam_pg_param *bound = count > 0 ? safe_emalloc(count, sizeof(pam_pg_param), 0) : NULL;
    zend_string **held = count > 0 ? safe_emalloc(count, sizeof(zend_string *), 0) : NULL;
    size_t used = 0;
    size_t kept = 0;
    bool failed = false;
    /* Positional keys bind in key order whatever the insertion order (bindValue(2, ...)
     * before bindValue(1, ...)): they must be consecutive from the smallest one. */
    zend_ulong base = 0;
    bool positional = false;
    bool named = false;
    if (params != NULL) {
        ZEND_HASH_FOREACH_KEY(params, index, key) {
            if (key != NULL) {
                named = true;
            } else if (!positional || index < base) {
                base = index;
                positional = true;
            } else {
                positional = true;
            }
        } ZEND_HASH_FOREACH_END();
    }
    if (named && positional) {
        if (bound != NULL) {
            efree(bound);
            efree(held);
        }
        pam_pg_throw("HY093", "Invalid parameter number: mixed named and positional parameters");
        RETURN_THROWS();
    }
    if (positional) {
        for (uint32_t slot = 0; slot < count; slot++) {
            bound[slot].kind = -1;
        }
    }
    if (params != NULL) {
        ZEND_HASH_FOREACH_KEY_VAL(params, index, key, value) {
            pam_pg_param *param = &bound[used];
            if (key == NULL) {
                zend_ulong slot = index - base;
                if (slot >= count || bound[slot].kind != -1) {
                    failed = true;
                    pam_pg_throw("HY093", "Invalid parameter number: number of bound variables does not match number of tokens");
                    break;
                }
                param = &bound[slot];
            }
            param->name = NULL;
            param->name_length = 0;
            if (key != NULL) {
                param->name = ZSTR_VAL(key);
                param->name_length = ZSTR_LEN(key);
                if (param->name_length > 0 && param->name[0] == ':') {
                    param->name++;
                    param->name_length--;
                }
            }
            ZVAL_DEREF(value);
            param->kind = 1;
            switch (Z_TYPE_P(value)) {
                case IS_NULL:
                    param->kind = 0;
                    param->value = NULL;
                    param->value_length = 0;
                    break;
                case IS_FALSE:
                case IS_TRUE:
                    param->value = Z_TYPE_P(value) == IS_TRUE ? "t" : "f";
                    param->value_length = 1;
                    break;
                case IS_STRING:
                    param->value = Z_STRVAL_P(value);
                    param->value_length = Z_STRLEN_P(value);
                    break;
                case IS_ARRAY: {
                    zval *binary = zend_hash_num_elements(Z_ARRVAL_P(value)) == 1
                        ? zend_hash_index_find(Z_ARRVAL_P(value), 0)
                        : NULL;
                    if (binary != NULL) {
                        ZVAL_DEREF(binary);
                    }
                    if (binary == NULL || Z_TYPE_P(binary) != IS_STRING) {
                        zend_argument_type_error(3, "values must be scalar, null, Stringable or [string] for binary data");
                        failed = true;
                        break;
                    }
                    param->kind = 2;
                    param->value = Z_STRVAL_P(binary);
                    param->value_length = Z_STRLEN_P(binary);
                    break;
                }
                default: {
                    /* int, float and Stringable: converted like (string). */
                    if (Z_TYPE_P(value) != IS_LONG && Z_TYPE_P(value) != IS_DOUBLE && Z_TYPE_P(value) != IS_OBJECT) {
                        zend_argument_type_error(3, "values must be scalar, null, Stringable or [string] for binary data");
                        failed = true;
                        break;
                    }
                    zend_string *text = zval_try_get_string(value);
                    if (text == NULL) {
                        failed = true;
                        break;
                    }
                    held[kept++] = text;
                    param->value = ZSTR_VAL(text);
                    param->value_length = ZSTR_LEN(text);
                    break;
                }
            }
            if (failed) {
                break;
            }
            used++;
        } ZEND_HASH_FOREACH_END();
    }

    pam_pg_result result = {0};
    pam_pg_error error = {{0}, NULL};
    int ok = 0;
    if (!failed) {
        ok = pam_pg->execute(handle, ZSTR_VAL(sql), ZSTR_LEN(sql), bound, used, (flags & PAM_PG_SIMPLE) ? 1 : 0, &result, &error);
    }
    for (size_t at = 0; at < kept; at++) {
        zend_string_release(held[at]);
    }
    if (held != NULL) {
        efree(held);
    }
    if (bound != NULL) {
        efree(bound);
    }
    if (failed) {
        RETURN_THROWS();
    }
    if (!ok) {
        pam_pg_throw_error(&error);
        RETURN_THROWS();
    }
    if (flags & PAM_PG_SIMPLE) {
        pam_pg->result_free(result.owner);
        RETURN_LONG((zend_long) result.affected);
    }

    size_t width = result.column_count;
    zval columns;
    array_init_size(&columns, (uint32_t) width);
    zend_string **names = width > 0 ? safe_emalloc(width, sizeof(zend_string *), 0) : NULL;
    for (size_t column = 0; column < width; column++) {
        names[column] = zend_string_init(result.columns[column].name, result.columns[column].name_length, 0);
        zend_string_hash_val(names[column]);
        add_next_index_str(&columns, zend_string_copy(names[column]));
    }
    zval rows;
    array_init_size(&rows, (uint32_t) result.row_count);
    zend_hash_real_init_packed(Z_ARRVAL(rows));
    const pam_pg_cell *cell = result.cells;
    for (size_t row = 0; row < result.row_count; row++) {
        zval line;
        array_init_size(&line, (uint32_t) width);
        if (flags & PAM_PG_NUM) {
            zend_hash_real_init_packed(Z_ARRVAL(line));
        }
        for (size_t column = 0; column < width; column++, cell++) {
            zval item;
            pam_pg_value(&item, cell, result.columns[column].type_oid);
            if (flags & PAM_PG_NUM) {
                zend_hash_next_index_insert_new(Z_ARRVAL(line), &item);
            } else {
                /* A repeated column name keeps the last value, as PDO::FETCH_ASSOC does. */
                zend_hash_update(Z_ARRVAL(line), names[column], &item);
            }
        }
        zend_hash_next_index_insert_new(Z_ARRVAL(rows), &line);
    }
    uint64_t affected = result.affected;
    pam_pg->result_free(result.owner);
    for (size_t column = 0; column < width; column++) {
        zend_string_release(names[column]);
    }
    if (names != NULL) {
        efree(names);
    }

    array_init_size(return_value, 3);
    add_next_index_long(return_value, (zend_long) affected);
    add_next_index_zval(return_value, &columns);
    add_next_index_zval(return_value, &rows);
}

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_pam_pg_release, 0, 2, IS_VOID, 0)
    ZEND_ARG_TYPE_INFO(0, session, IS_LONG, 0)
    ZEND_ARG_TYPE_INFO(0, sql, IS_STRING, 0)
ZEND_END_ARG_INFO()

static PHP_FUNCTION(pam_pg_release)
{
    zend_long session;
    zend_string *sql;

    ZEND_PARSE_PARAMETERS_START(2, 2)
        Z_PARAM_LONG(session)
        Z_PARAM_STR(sql)
    ZEND_PARSE_PARAMETERS_END();

    (void) return_value;
    if (pam_pg != NULL && session > 0) {
        pam_pg->release((uint64_t) session, ZSTR_VAL(sql), ZSTR_LEN(sql));
    }
}

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_pam_pg_transaction, 0, 2, IS_VOID, 0)
    ZEND_ARG_TYPE_INFO(0, session, IS_LONG, 0)
    ZEND_ARG_TYPE_INFO(0, operation, IS_LONG, 0)
ZEND_END_ARG_INFO()

static PHP_FUNCTION(pam_pg_transaction)
{
    zend_long session;
    zend_long operation;

    ZEND_PARSE_PARAMETERS_START(2, 2)
        Z_PARAM_LONG(session)
        Z_PARAM_LONG(operation)
    ZEND_PARSE_PARAMETERS_END();

    (void) return_value;
    uint64_t handle;
    if (!pam_pg_ready() || !pam_pg_handle(session, &handle)) {
        RETURN_THROWS();
    }
    if (operation < 0 || operation > 2) {
        zend_argument_value_error(2, "must be 0 (begin), 1 (commit) or 2 (rollback)");
        RETURN_THROWS();
    }
    pam_pg_error error = {{0}, NULL};
    if (!pam_pg->transaction(handle, (int) operation, &error)) {
        pam_pg_throw_error(&error);
        RETURN_THROWS();
    }
}

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_pam_pg_status, 0, 1, IS_LONG, 0)
    ZEND_ARG_TYPE_INFO(0, session, IS_LONG, 0)
ZEND_END_ARG_INFO()

static PHP_FUNCTION(pam_pg_status)
{
    zend_long session;

    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_LONG(session)
    ZEND_PARSE_PARAMETERS_END();

    if (pam_pg == NULL || session <= 0) {
        RETURN_LONG(-1);
    }
    RETURN_LONG(pam_pg->status((uint64_t) session));
}

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_pam_pg_parameter, 0, 2, IS_STRING, 1)
    ZEND_ARG_TYPE_INFO(0, session, IS_LONG, 0)
    ZEND_ARG_TYPE_INFO(0, name, IS_STRING, 0)
ZEND_END_ARG_INFO()

static PHP_FUNCTION(pam_pg_parameter)
{
    zend_long session;
    zend_string *name;
    char *value = NULL;

    ZEND_PARSE_PARAMETERS_START(2, 2)
        Z_PARAM_LONG(session)
        Z_PARAM_STR(name)
    ZEND_PARSE_PARAMETERS_END();

    if (pam_pg == NULL || session <= 0
        || !pam_pg->parameter((uint64_t) session, ZSTR_VAL(name), ZSTR_LEN(name), &value)) {
        RETURN_NULL();
    }
    RETVAL_STRING(value);
    pam_pg->free_string(value);
}

#define PAM_PG_FUNCTION_ENTRIES \
    PHP_FE(pam_pg_connect, arginfo_pam_pg_connect) \
    PHP_FE(pam_pg_close, arginfo_pam_pg_close) \
    PHP_FE(pam_pg_execute, arginfo_pam_pg_execute) \
    PHP_FE(pam_pg_release, arginfo_pam_pg_release) \
    PHP_FE(pam_pg_transaction, arginfo_pam_pg_transaction) \
    PHP_FE(pam_pg_status, arginfo_pam_pg_status) \
    PHP_FE(pam_pg_parameter, arginfo_pam_pg_parameter)
