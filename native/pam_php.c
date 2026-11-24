#include <sapi/embed/php_embed.h>
#include <main/php_memory_streams.h>
#include <main/php_network.h>
#if PHP_VERSION_ID >= 80200
#include <ext/random/php_random.h>
#endif
#include <Zend/zend_smart_str.h>
#include <ext/standard/url.h>
#include <ext/date/php_date.h>

#include "pam.h"

#include <stdbool.h>
#include <signal.h>
#include <stdlib.h>
#include <string.h>

#ifdef PAM_FIBER_STACK_POOL
/* native/fiber_stack_pool.c: Fiber C stack reuse (Linux). */
unsigned long pam_fiber_stack_pool_reuses(void);
#endif

enum pam_exit_code {
    PAM_SUCCESS = 0,
    PAM_EXECUTION_FAILURE = 1,
    PAM_INITIALIZATION_FAILURE = 70,
};

static bool pam_initialized = false;
static bool pam_native_functions_registered = false;
static char *pam_ini_entries = NULL;
static zend_class_entry *pam_runtime_class = NULL;
static zend_function *pam_server_config_function = NULL;
static zend_function *pam_runtime_info_function = NULL;
static zend_function *pam_runtime_metrics_function = NULL;
static zend_function *pam_runtime_diagnostics_function = NULL;
static zend_function *pam_routes_info_function = NULL;
static zend_function *pam_begin_http_dispatch_function = NULL;
static zend_function *pam_resume_http_dispatch_function = NULL;
static zend_function *pam_cancel_http_dispatch_function = NULL;
static zend_function *pam_dispatch_ws_function = NULL;

static void pam_set_php_binary(const char *executable)
{
    zend_string *name;
    zend_constant *constant;

    if (executable == NULL) {
        return;
    }
    name = zend_string_init("PHP_BINARY", sizeof("PHP_BINARY") - 1, 0);
    constant = zend_get_constant_ptr(name);
    zend_string_release(name);
    if (constant == NULL) {
        return;
    }
    zval_ptr_dtor(&constant->value);
    ZVAL_STR(
        &constant->value,
        zend_string_init(executable, strlen(executable), 1)
    );
}

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(
    arginfo_pam_native_abi_version,
    0,
    0,
    IS_LONG,
    0
)
ZEND_END_ARG_INFO()

static PHP_FUNCTION(pam_native_abi_version)
{
    ZEND_PARSE_PARAMETERS_NONE();
    RETURN_LONG((zend_long) PAM_NATIVE_ABI_VERSION);
}

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(
    arginfo_pam_native_fiber_stack_reuses,
    0,
    0,
    IS_LONG,
    0
)
ZEND_END_ARG_INFO()

/* Fiber C stacks served from the per-process pool (0 where there is none). */
static PHP_FUNCTION(pam_native_fiber_stack_reuses)
{
    ZEND_PARSE_PARAMETERS_NONE();
#ifdef PAM_FIBER_STACK_POOL
    RETURN_LONG((zend_long) pam_fiber_stack_pool_reuses());
#else
    RETURN_LONG(0);
#endif
}

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_MASK_EX(
    arginfo_pam_native_encode_http_response,
    0,
    4,
    MAY_BE_STRING | MAY_BE_NULL
)
    ZEND_ARG_TYPE_INFO(0, status, IS_LONG, 0)
    ZEND_ARG_TYPE_INFO(0, headers, IS_ARRAY, 0)
    ZEND_ARG_TYPE_INFO(0, body, IS_STRING, 0)
    ZEND_ARG_TYPE_INFO(0, chunks, IS_ARRAY, 0)
ZEND_END_ARG_INFO()

static void pam_append_u16(smart_str *output, uint32_t value)
{
    char bytes[2] = {(char) ((value >> 8) & 0xff), (char) (value & 0xff)};
    smart_str_appendl(output, bytes, 2);
}

static void pam_append_u32(smart_str *output, size_t value)
{
    char bytes[4] = {
        (char) ((value >> 24) & 0xff),
        (char) ((value >> 16) & 0xff),
        (char) ((value >> 8) & 0xff),
        (char) (value & 0xff),
    };
    smart_str_appendl(output, bytes, 4);
}

/*
 * Runtime::encodeHttpResponse() in C: same bytes as its pack('n'/'N')
 * encoding. Returns null (the caller falls back to the PHP encoder, which
 * raises the precise error) when a limit is exceeded or a value is not a
 * plain string/int.
 */
static PHP_FUNCTION(pam_native_encode_http_response)
{
    zend_long status;
    HashTable *headers;
    zend_string *body;
    HashTable *chunks;
    smart_str output = {0};
    zend_string *name;
    zend_ulong index;
    zval *values;
    zval *value;

    ZEND_PARSE_PARAMETERS_START(4, 4)
        Z_PARAM_LONG(status)
        Z_PARAM_ARRAY_HT(headers)
        Z_PARAM_STR(body)
        Z_PARAM_ARRAY_HT(chunks)
    ZEND_PARSE_PARAMETERS_END();

    if (status < 100 || status > 999 || zend_hash_num_elements(headers) > 65535
        || ZSTR_LEN(body) > UINT32_MAX) {
        RETURN_NULL();
    }
    smart_str_alloc(&output, 64 + ZSTR_LEN(body), 0);
    pam_append_u16(&output, (uint32_t) status);
    pam_append_u16(&output, zend_hash_num_elements(headers));
    ZEND_HASH_FOREACH_KEY_VAL(headers, index, name, values) {
        char numeric[MAX_LENGTH_OF_LONG + 1];
        const char *name_value;
        size_t name_length;
        ZVAL_DEREF(values);
        if (name != NULL) {
            name_value = ZSTR_VAL(name);
            name_length = ZSTR_LEN(name);
        } else {
            char *end = zend_print_ulong_to_buf(numeric + sizeof(numeric) - 1, index);
            numeric[sizeof(numeric) - 1] = '\0';
            name_value = end;
            name_length = (size_t) (numeric + sizeof(numeric) - 1 - end);
        }
        if (Z_TYPE_P(values) != IS_ARRAY || name_length > 65535
            || zend_hash_num_elements(Z_ARRVAL_P(values)) > 65535) {
            smart_str_free(&output);
            RETURN_NULL();
        }
        pam_append_u16(&output, (uint32_t) name_length);
        smart_str_appendl(&output, name_value, name_length);
        pam_append_u16(&output, zend_hash_num_elements(Z_ARRVAL_P(values)));
        ZEND_HASH_FOREACH_VAL(Z_ARRVAL_P(values), value) {
            ZVAL_DEREF(value);
            if (Z_TYPE_P(value) == IS_STRING && Z_STRLEN_P(value) <= UINT32_MAX) {
                pam_append_u32(&output, Z_STRLEN_P(value));
                smart_str_appendl(&output, Z_STRVAL_P(value), Z_STRLEN_P(value));
            } else if (Z_TYPE_P(value) == IS_LONG) {
                char buffer[MAX_LENGTH_OF_LONG + 1];
                int length = snprintf(buffer, sizeof(buffer), ZEND_LONG_FMT, Z_LVAL_P(value));
                pam_append_u32(&output, (size_t) length);
                smart_str_appendl(&output, buffer, (size_t) length);
            } else {
                smart_str_free(&output);
                RETURN_NULL();
            }
        } ZEND_HASH_FOREACH_END();
    } ZEND_HASH_FOREACH_END();
    pam_append_u32(&output, ZSTR_LEN(body));
    smart_str_append(&output, body);
    pam_append_u32(&output, zend_hash_num_elements(chunks));
    ZEND_HASH_FOREACH_VAL(chunks, value) {
        ZVAL_DEREF(value);
        if (Z_TYPE_P(value) != IS_STRING || Z_STRLEN_P(value) > UINT32_MAX) {
            smart_str_free(&output);
            RETURN_NULL();
        }
        pam_append_u32(&output, Z_STRLEN_P(value));
        smart_str_appendl(&output, Z_STRVAL_P(value), Z_STRLEN_P(value));
    } ZEND_HASH_FOREACH_END();

    RETURN_STR(smart_str_extract(&output));
}

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_MASK_EX(
    arginfo_pam_native_stream_fd,
    0,
    1,
    MAY_BE_LONG | MAY_BE_FALSE
)
    ZEND_ARG_INFO(0, stream)
ZEND_END_ARG_INFO()

static PHP_FUNCTION(pam_native_stream_fd)
{
    zval *stream_value;
    php_stream *stream;
    php_socket_t file_descriptor = -1;

    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_RESOURCE(stream_value)
    ZEND_PARSE_PARAMETERS_END();

    php_stream_from_zval_no_verify(stream, stream_value);
    if (stream == NULL || php_stream_cast(
        stream,
        PHP_STREAM_AS_FD_FOR_SELECT | PHP_STREAM_CAST_INTERNAL,
        (void *) &file_descriptor,
        1
    ) != SUCCESS) {
        RETURN_FALSE;
    }
    if (file_descriptor < 0) {
        RETURN_FALSE;
    }
    RETURN_LONG((zend_long) file_descriptor);
}

/*
 * Resets the SAPI response state between logical requests without PHP's
 * userland warnings: header_remove() keeps a raw `HTTP/` status line (set by
 * header('HTTP/1.1 200 OK'), as Symfony's Response::send() does), and PHP 8.5
 * warns when http_response_code() runs after one.
 */
static void pam_reset_sapi_response(int status_code)
{
    sapi_header_op(SAPI_HEADER_DELETE_ALL, NULL);
    if (SG(sapi_headers).http_status_line != NULL) {
        efree(SG(sapi_headers).http_status_line);
        SG(sapi_headers).http_status_line = NULL;
    }
    SG(headers_sent) = 0;
    SG(sapi_headers).http_response_code = status_code;
    SG(sapi_headers).send_default_content_type = 0;
}

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(
    arginfo_pam_native_reset_response,
    0,
    0,
    IS_VOID,
    0
)
    ZEND_ARG_TYPE_INFO_WITH_DEFAULT_VALUE(0, status_code, IS_LONG, 0, "200")
ZEND_END_ARG_INFO()

static PHP_FUNCTION(pam_native_reset_response)
{
    zend_long status_code = 200;

    ZEND_PARSE_PARAMETERS_START(0, 1)
        Z_PARAM_OPTIONAL
        Z_PARAM_LONG(status_code)
    ZEND_PARSE_PARAMETERS_END();

    if (status_code < 100 || status_code > 999) {
        zend_argument_value_error(1, "must be a valid HTTP status code");
        RETURN_THROWS();
    }
    pam_reset_sapi_response((int) status_code);
}

static volatile pam_cache_purge_handler pam_cache_purge_callback = NULL;

void pam_php_set_cache_purge_handler(pam_cache_purge_handler handler)
{
    pam_cache_purge_callback = handler;
}

static volatile int pam_worker_draining = 0;

void pam_php_set_draining(int draining)
{
    pam_worker_draining = draining != 0;
}

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_pam_draining, 0, 0, _IS_BOOL, 0)
ZEND_END_ARG_INFO()

/*
 * pam_draining(): bool
 * True once this worker started draining (SIGTERM, reload, recycle): it
 * finishes the requests it holds and takes no new work. Application readiness
 * endpoints answer 503 from then on.
 */
static PHP_FUNCTION(pam_draining)
{
    ZEND_PARSE_PARAMETERS_NONE();
    RETURN_BOOL(pam_worker_draining != 0);
}

static bool pam_cache_purge_append(smart_str *lines, zend_string *value, const char *prefix)
{
    if (ZSTR_LEN(value) == 0 || memchr(ZSTR_VAL(value), '\n', ZSTR_LEN(value)) != NULL
        || memchr(ZSTR_VAL(value), '\r', ZSTR_LEN(value)) != NULL) {
        return false;
    }
    smart_str_appends(lines, prefix);
    smart_str_append(lines, value);
    smart_str_appendc(lines, '\n');
    return true;
}

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_MASK_EX(
    arginfo_pam_cache_purge,
    0,
    0,
    MAY_BE_LONG | MAY_BE_FALSE
)
    ZEND_ARG_TYPE_INFO_WITH_DEFAULT_VALUE(0, tags, IS_ARRAY, 0, "[]")
    ZEND_ARG_TYPE_INFO_WITH_DEFAULT_VALUE(0, urls, IS_ARRAY, 0, "[]")
    ZEND_ARG_TYPE_INFO_WITH_DEFAULT_VALUE(0, all, _IS_BOOL, 0, "false")
ZEND_END_ARG_INFO()

/*
 * pam_cache_purge(array $tags = [], array $urls = [], bool $all = false): int|false
 * Purges the PAM edge page cache of this worker and publishes the purge to
 * sibling workers. Returns the entries purged here, or false when no page
 * cache runs in this process (callers then use the invalidation log).
 */
static PHP_FUNCTION(pam_cache_purge)
{
    HashTable *tags = NULL;
    HashTable *urls = NULL;
    bool all = false;
    smart_str lines = {0};
    zval *item;
    int64_t result;

    ZEND_PARSE_PARAMETERS_START(0, 3)
        Z_PARAM_OPTIONAL
        Z_PARAM_ARRAY_HT(tags)
        Z_PARAM_ARRAY_HT(urls)
        Z_PARAM_BOOL(all)
    ZEND_PARSE_PARAMETERS_END();

    if (all) {
        smart_str_appends(&lines, "*\n");
    }
    if (tags != NULL) {
        ZEND_HASH_FOREACH_VAL(tags, item) {
            if (Z_TYPE_P(item) != IS_STRING || !pam_cache_purge_append(&lines, Z_STR_P(item), "")) {
                smart_str_free(&lines);
                zend_argument_value_error(1, "must contain only non-empty single-line strings");
                RETURN_THROWS();
            }
        } ZEND_HASH_FOREACH_END();
    }
    if (urls != NULL) {
        ZEND_HASH_FOREACH_VAL(urls, item) {
            if (Z_TYPE_P(item) != IS_STRING || !pam_cache_purge_append(&lines, Z_STR_P(item), "!url ")) {
                smart_str_free(&lines);
                zend_argument_value_error(2, "must contain only non-empty single-line strings");
                RETURN_THROWS();
            }
        } ZEND_HASH_FOREACH_END();
    }
    if (pam_cache_purge_callback == NULL) {
        smart_str_free(&lines);
        RETURN_FALSE;
    }
    smart_str_0(&lines);
    result = pam_cache_purge_callback(
        lines.s == NULL ? "" : ZSTR_VAL(lines.s),
        lines.s == NULL ? 0 : ZSTR_LEN(lines.s)
    );
    smart_str_free(&lines);
    if (result == -1) {
        RETURN_FALSE;
    }
    if (result == -2) {
        zend_value_error("pam_cache_purge(): invalid cache tag or URL");
        RETURN_THROWS();
    }
    if (result < 0) {
        zend_throw_error(NULL, "pam_cache_purge(): the invalidation log could not be written");
        RETURN_THROWS();
    }
    RETURN_LONG((zend_long) result);
}

ZEND_BEGIN_ARG_WITH_RETURN_OBJ_TYPE_MASK_EX(
    arginfo_pam_native_utc_datetime,
    0,
    2,
    DateTimeImmutable,
    MAY_BE_NULL
)
    ZEND_ARG_TYPE_INFO(0, value, IS_STRING, 0)
    ZEND_ARG_OBJ_INFO(0, prototype, DateTimeImmutable, 0)
ZEND_END_ARG_INFO()

static int pam_digits(const char *text, int count)
{
    int value = 0;
    for (int index = 0; index < count; index++) {
        if (text[index] < '0' || text[index] > '9') {
            return -1;
        }
        value = value * 10 + (text[index] - '0');
    }
    return value;
}

/* Days from 1970-01-01 to a proleptic Gregorian date (Howard Hinnant's days_from_civil). */
static int64_t pam_days_from_civil(int64_t year, int64_t month, int64_t day)
{
    year -= month <= 2;
    int64_t era = (year >= 0 ? year : year - 399) / 400;
    int64_t year_of_era = year - era * 400;
    int64_t day_of_year = (153 * (month + (month > 2 ? -3 : 9)) + 2) / 5 + day - 1;
    int64_t day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    return era * 146097 + day_of_era - 719468;
}

/*
 * new DateTimeImmutable($value, UTC) for a database timestamp
 * "YYYY-MM-DD HH:MM:SS" (years 1-9999, a valid calendar date and time):
 * a clone of $prototype, an object constructed from a string of that same
 * shape in UTC, with the date, time and epoch seconds of $value. The parser
 * gives an identical object (same fields and flags); this skips its scanner
 * and time zone pass. Null for any other input, prototype or zone: the
 * caller constructs the object itself.
 */
static PHP_FUNCTION(pam_native_utc_datetime)
{
    zend_string *value;
    zval *prototype;
    static const int month_days[12] = {31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31};

    ZEND_PARSE_PARAMETERS_START(2, 2)
        Z_PARAM_STR(value)
        Z_PARAM_OBJECT_OF_CLASS(prototype, php_date_get_immutable_ce())
    ZEND_PARSE_PARAMETERS_END();

    zend_object *source = Z_OBJ_P(prototype);
    const timelib_time *base = php_date_obj_from_obj(source)->time;
    if (source->ce != php_date_get_immutable_ce() || base == NULL
        || base->zone_type != TIMELIB_ZONETYPE_ID || base->tz_info == NULL
        || base->tz_info->name == NULL || strcmp(base->tz_info->name, "UTC") != 0
        || !base->is_localtime || base->have_relative || base->us != 0
        || !base->have_time || !base->have_date
        || ZSTR_LEN(value) != 19) {
        RETURN_NULL();
    }
    const char *text = ZSTR_VAL(value);
    if (text[4] != '-' || text[7] != '-' || text[10] != ' ' || text[13] != ':' || text[16] != ':') {
        RETURN_NULL();
    }
    int year = pam_digits(text, 4);
    int month = pam_digits(text + 5, 2);
    int day = pam_digits(text + 8, 2);
    int hour = pam_digits(text + 11, 2);
    int minute = pam_digits(text + 14, 2);
    int second = pam_digits(text + 17, 2);
    if (year < 1 || month < 1 || month > 12 || day < 1 || hour < 0 || hour > 23
        || minute < 0 || minute > 59 || second < 0 || second > 59) {
        RETURN_NULL();
    }
    bool leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    if (day > month_days[month - 1] + (month == 2 && leap ? 1 : 0)) {
        RETURN_NULL();
    }

    zend_object *copy = source->handlers->clone_obj(source);
    if (copy == NULL) {
        RETURN_NULL();
    }
    timelib_time *time = php_date_obj_from_obj(copy)->time;
    if (time == NULL) {
        OBJ_RELEASE(copy);
        RETURN_NULL();
    }
    time->y = year;
    time->m = month;
    time->d = day;
    time->h = hour;
    time->i = minute;
    time->s = second;
    time->sse = pam_days_from_civil(year, month, day) * 86400 + hour * 3600 + minute * 60 + second;
    RETURN_OBJ(copy);
}

#include "pam_shared_php.inc"

static const zend_function_entry pam_native_functions[] = {
    PHP_FE(pam_cache_purge, arginfo_pam_cache_purge)
    PHP_FE(pam_draining, arginfo_pam_draining)
    PHP_FE(pam_native_abi_version, arginfo_pam_native_abi_version)
    PHP_FE(pam_native_fiber_stack_reuses, arginfo_pam_native_fiber_stack_reuses)
    PHP_FE(pam_native_stream_fd, arginfo_pam_native_stream_fd)
    PHP_FE(pam_native_reset_response, arginfo_pam_native_reset_response)
    PHP_FE(pam_native_encode_http_response, arginfo_pam_native_encode_http_response)
    PHP_FE(pam_native_utc_datetime, arginfo_pam_native_utc_datetime)
    PHP_FE_END
};

/*
 * Shared tables and task workers stay registered until the engine itself
 * shuts down: objects destructed and shutdown functions run during
 * php_request_shutdown() (an Octane WorkerStopping listener, a table's
 * destructor) may still call them, and their call sites cache the function.
 * They are the SAPI's additional functions, registered by module startup
 * and released by zend_shutdown() with the rest of the function table.
 * Registered after the request had started (as before), the executor's
 * shutdown discarded them without freeing them (valgrind: 14 definitely lost
 * blocks), and their request-interned names cannot be freed later.
 */
static const zend_function_entry pam_shared_functions[] = {
    PAM_SHARED_FUNCTION_ENTRIES
    PHP_FE_END
};

static void pam_unregister_native_functions(void)
{
    if (!pam_native_functions_registered) {
        return;
    }
    zend_unregister_functions(
        pam_native_functions,
        (int) ((sizeof(pam_native_functions) /
            sizeof(pam_native_functions[0])) - 1),
        NULL
    );
    pam_native_functions_registered = false;
}

uint32_t pam_native_abi_version(void)
{
    return PAM_NATIVE_ABI_VERSION;
}

static void pam_reset_runtime_cache(void)
{
    pam_runtime_class = NULL;
    pam_server_config_function = NULL;
    pam_runtime_info_function = NULL;
    pam_runtime_metrics_function = NULL;
    pam_runtime_diagnostics_function = NULL;
    pam_routes_info_function = NULL;
    pam_begin_http_dispatch_function = NULL;
    pam_resume_http_dispatch_function = NULL;
    pam_cancel_http_dispatch_function = NULL;
    pam_dispatch_ws_function = NULL;
}

#ifndef PHP_WIN32
#include <unistd.h>
#endif

/* The computer's IANA time zone: TZ, then the /etc/localtime link
 * (".../zoneinfo/America/Sao_Paulo"). Empty when unknown. */
static bool pam_system_timezone(char *zone, size_t size)
{
    const char *environment = getenv("TZ");
    const char *name = NULL;
    char link[512];
    if (environment != NULL && environment[0] != '\0') {
        name = environment[0] == ':' ? environment + 1 : environment;
    }
#ifndef PHP_WIN32
    if (name == NULL || name[0] == '\0') {
        ssize_t length = readlink("/etc/localtime", link, sizeof(link) - 1);
        if (length > 0) {
            link[length] = '\0';
            const char *marker = strstr(link, "zoneinfo/");
            name = marker == NULL ? NULL : marker + strlen("zoneinfo/");
        }
    }
#endif
    if (name == NULL || name[0] == '\0' || name[0] == '/' || strlen(name) >= size) {
        return false;
    }
    for (const char *c = name; *c != '\0'; ++c) {
        bool allowed = (*c >= 'A' && *c <= 'Z') || (*c >= 'a' && *c <= 'z') ||
            (*c >= '0' && *c <= '9') || *c == '/' || *c == '_' || *c == '-' || *c == '+';
        if (!allowed) {
            return false;
        }
    }
    strcpy(zone, name);
    return true;
}

/* Defaults php.ini can override: PHP ignores the system zone and falls back
 * to UTC, so date() was hours off wherever the user is not on UTC. */
static void pam_ini_defaults(HashTable *configuration_hash)
{
    char zone[128];
    zval value;
    if (!pam_system_timezone(zone, sizeof(zone))) {
        return;
    }
    ZVAL_NEW_STR(&value, zend_string_init(zone, strlen(zone), 1));
    zend_hash_str_update(configuration_hash, "date.timezone", sizeof("date.timezone") - 1, &value);
}

static int pam_embed_init(int argc, char **argv)
{
    static const char defaults[] =
        "html_errors=0\n"
        "register_argc_argv=1\n"
        "implicit_flush=1\n"
        "output_buffering=0\n"
        "max_execution_time=0\n"
        "max_input_time=-1\n";
    const char *overrides = getenv("PAM_INI_ENTRIES");
    size_t defaults_length = sizeof(defaults) - 1;
    size_t overrides_length = overrides == NULL ? 0 : strlen(overrides);

#ifdef SIGPIPE
    signal(SIGPIPE, SIG_IGN);
#endif
#ifdef ZTS
    php_tsrm_startup();
#ifdef PHP_WIN32
    ZEND_TSRMLS_CACHE_UPDATE();
#endif
#endif
    zend_signal_startup();
    sapi_startup(&php_embed_module);

    pam_ini_entries = malloc(defaults_length + overrides_length + 1);
    if (pam_ini_entries == NULL) {
        return FAILURE;
    }
    memcpy(pam_ini_entries, defaults, defaults_length);
    if (overrides_length > 0) {
        memcpy(pam_ini_entries + defaults_length, overrides, overrides_length);
    }
    pam_ini_entries[defaults_length + overrides_length] = '\0';
    php_embed_module.ini_entries = pam_ini_entries;
    php_embed_module.ini_defaults = pam_ini_defaults;
    /* `pam -n`: like the PHP CLI, skip php.ini discovery (the compiled-in
     * prefix path, PHPRC and the scan directory) while keeping -d entries. */
    const char *ignore_ini = getenv("PAM_INI_IGNORE");
    if (ignore_ini != NULL && ignore_ini[0] == '1' && ignore_ini[1] == '\0') {
        php_embed_module.php_ini_ignore = 1;
        php_embed_module.php_ini_ignore_cwd = 1;
    }
    if (argv != NULL && argc > 0) {
        php_embed_module.executable_location = argv[0];
    }

    if (php_embed_module.startup(&php_embed_module) == FAILURE) {
        php_embed_module.ini_entries = NULL;
        free(pam_ini_entries);
        pam_ini_entries = NULL;
        return FAILURE;
    }

    SG(options) |= SAPI_OPTION_NO_CHDIR;
    SG(request_info).argc = argc;
    SG(request_info).argv = argv;
    if (php_request_startup() == FAILURE) {
        php_module_shutdown();
        php_embed_module.ini_entries = NULL;
        free(pam_ini_entries);
        pam_ini_entries = NULL;
        return FAILURE;
    }
    SG(headers_sent) = 1;
    SG(request_info).no_headers = 1;
    php_register_variable("PHP_SELF", "-", NULL);
    return SUCCESS;
}

static zend_function *pam_runtime_function(const char *method)
{
    zend_function **cache = NULL;
    const char *normalized = NULL;
    size_t normalized_length = 0;

    if (strcmp(method, "serverConfig") == 0) {
        cache = &pam_server_config_function;
        normalized = "serverconfig";
    } else if (strcmp(method, "runtimeInfo") == 0) {
        cache = &pam_runtime_info_function;
        normalized = "runtimeinfo";
    } else if (strcmp(method, "runtimeMetrics") == 0) {
        cache = &pam_runtime_metrics_function;
        normalized = "runtimemetrics";
    } else if (strcmp(method, "runtimeDiagnostics") == 0) {
        cache = &pam_runtime_diagnostics_function;
        normalized = "runtimediagnostics";
    } else if (strcmp(method, "routesInfo") == 0) {
        cache = &pam_routes_info_function;
        normalized = "routesinfo";
    } else if (strcmp(method, "beginHttpDispatch") == 0) {
        cache = &pam_begin_http_dispatch_function;
        normalized = "beginhttpdispatch";
    } else if (strcmp(method, "resumeHttpDispatch") == 0) {
        cache = &pam_resume_http_dispatch_function;
        normalized = "resumehttpdispatch";
    } else if (strcmp(method, "cancelHttpDispatch") == 0) {
        cache = &pam_cancel_http_dispatch_function;
        normalized = "cancelhttpdispatch";
    } else if (strcmp(method, "dispatchWs") == 0) {
        cache = &pam_dispatch_ws_function;
        normalized = "dispatchws";
    } else {
        return NULL;
    }

    if (*cache != NULL) {
        return *cache;
    }
    if (pam_runtime_class == NULL) {
        zend_string *class_name = zend_string_init(
            "Pam\\Internal\\Runtime",
            sizeof("Pam\\Internal\\Runtime") - 1,
            0
        );
        pam_runtime_class = zend_lookup_class(class_name);
        zend_string_release(class_name);
        if (pam_runtime_class == NULL) {
            return NULL;
        }
    }

    normalized_length = strlen(normalized);
    *cache = zend_hash_str_find_ptr(
        &pam_runtime_class->function_table,
        normalized,
        normalized_length
    );
    return *cache;
}

static int pam_register_file_handles(void)
{
    php_stream *input;
    php_stream *output;
    php_stream *error;
    zend_constant constant;

    input = php_stream_open_wrapper("php://stdin", "rb", 0, NULL);
    output = php_stream_open_wrapper("php://stdout", "wb", 0, NULL);
    error = php_stream_open_wrapper("php://stderr", "wb", 0, NULL);
    if (input == NULL || output == NULL || error == NULL) {
        if (input != NULL) {
            php_stream_close(input);
        }
        if (output != NULL) {
            php_stream_close(output);
        }
        if (error != NULL) {
            php_stream_close(error);
        }
        return FAILURE;
    }

    input->flags |= PHP_STREAM_FLAG_NO_RSCR_DTOR_CLOSE;
    output->flags |= PHP_STREAM_FLAG_NO_RSCR_DTOR_CLOSE;
    error->flags |= PHP_STREAM_FLAG_NO_RSCR_DTOR_CLOSE;

    php_stream_to_zval(input, &constant.value);
    Z_CONSTANT_FLAGS(constant.value) = 0;
    constant.name = zend_string_init_interned("STDIN", sizeof("STDIN") - 1, 0);
    zend_register_constant(&constant);

    php_stream_to_zval(output, &constant.value);
    Z_CONSTANT_FLAGS(constant.value) = 0;
    constant.name = zend_string_init_interned("STDOUT", sizeof("STDOUT") - 1, 0);
    zend_register_constant(&constant);

    php_stream_to_zval(error, &constant.value);
    Z_CONSTANT_FLAGS(constant.value) = 0;
    constant.name = zend_string_init_interned("STDERR", sizeof("STDERR") - 1, 0);
    zend_register_constant(&constant);

    return SUCCESS;
}

static int pam_runtime_call(
    const char *method,
    uint32_t parameter_count,
    zval *parameters,
    zval *result
)
{
    zend_function *function = pam_runtime_function(method);
    if (function == NULL) {
        return FAILURE;
    }

    zend_call_known_function(
        function,
        NULL,
        pam_runtime_class,
        result,
        parameter_count,
        parameters,
        NULL
    );
    return Z_ISUNDEF_P(result) ? FAILURE : SUCCESS;
}

static int pam_copy_string_result(zval *result, char **output, size_t *output_length)
{
    char *copy;

    if (Z_TYPE_P(result) != IS_STRING) {
        return FAILURE;
    }

    copy = malloc(Z_STRLEN_P(result) + 1);
    if (copy == NULL) {
        return FAILURE;
    }

    memcpy(copy, Z_STRVAL_P(result), Z_STRLEN_P(result));
    copy[Z_STRLEN_P(result)] = '\0';
    *output = copy;
    *output_length = Z_STRLEN_P(result);

    return SUCCESS;
}

/*
 * DateTime::format() and DateTimeImmutable::format() with DATE_ATOM
 * ("Y-m-d\TH:i:sP", also DATE_RFC3339) for a time in UTC or at a fixed
 * offset: the bytes php_date.c's date_format() writes, without its seven
 * snprintf() calls. Every other format, time zone or range goes to PHP's own
 * handler. PAM_FAST_DATE_FORMAT=0 leaves the methods untouched.
 */
static zif_handler pam_php_date_format = NULL;

static void pam_write_2(char *output, int value)
{
    output[0] = (char) ('0' + value / 10);
    output[1] = (char) ('0' + value % 10);
}

static bool pam_format_atom(const timelib_time *time, char output[25])
{
    int offset;

    if (time == NULL || !time->is_localtime) {
        return false;
    }
    switch (time->zone_type) {
        case TIMELIB_ZONETYPE_OFFSET:
            offset = (int) time->z;
            break;
        case TIMELIB_ZONETYPE_ABBR:
            offset = (int) (time->z + (time->dst * 3600));
            break;
        case TIMELIB_ZONETYPE_ID:
            /* UTC has no transitions: its offset is always 0. */
            if (time->tz_info == NULL || time->tz_info->name == NULL
                || strcmp(time->tz_info->name, "UTC") != 0) {
                return false;
            }
            offset = 0;
            break;
        default:
            return false;
    }
    if (time->y < 0 || time->y > 9999
        || time->m < 0 || time->m > 99 || time->d < 0 || time->d > 99
        || time->h < 0 || time->h > 99 || time->i < 0 || time->i > 99
        || time->s < 0 || time->s > 99) {
        return false;
    }
    int hours = abs(offset / 3600);
    int minutes = abs((offset % 3600) / 60);
    if (hours > 99) {
        return false;
    }
    int year = (int) time->y;
    output[0] = (char) ('0' + year / 1000);
    output[1] = (char) ('0' + year / 100 % 10);
    output[2] = (char) ('0' + year / 10 % 10);
    output[3] = (char) ('0' + year % 10);
    output[4] = '-';
    pam_write_2(output + 5, (int) time->m);
    output[7] = '-';
    pam_write_2(output + 8, (int) time->d);
    output[10] = 'T';
    pam_write_2(output + 11, (int) time->h);
    output[13] = ':';
    pam_write_2(output + 14, (int) time->i);
    output[16] = ':';
    pam_write_2(output + 17, (int) time->s);
    output[19] = offset < 0 ? '-' : '+';
    pam_write_2(output + 20, hours);
    output[22] = ':';
    pam_write_2(output + 23, minutes);
    return true;
}

static ZEND_NAMED_FUNCTION(pam_date_format_method)
{
    if (ZEND_NUM_ARGS() == 1 && Z_TYPE(EX(This)) == IS_OBJECT) {
        zval *format = ZEND_CALL_ARG(execute_data, 1);
        if (Z_TYPE_P(format) == IS_STRING
            && zend_string_equals_literal(Z_STR_P(format), "Y-m-d\\TH:i:sP")) {
            php_date_obj *date = php_date_obj_from_obj(Z_OBJ(EX(This)));
            char output[25];
            if (pam_format_atom(date->time, output)) {
                RETURN_STRINGL(output, sizeof(output));
            }
        }
    }
    pam_php_date_format(INTERNAL_FUNCTION_PARAM_PASSTHRU);
}

static void pam_install_date_format(void)
{
    const char *setting = getenv("PAM_FAST_DATE_FORMAT");
    if (setting != NULL && strcmp(setting, "0") == 0) {
        return;
    }
    zend_class_entry *classes[] = {php_date_get_date_ce(), php_date_get_immutable_ce()};
    zend_function *methods[2];
    for (size_t index = 0; index < 2; index++) {
        methods[index] = classes[index] == NULL
            ? NULL
            : zend_hash_str_find_ptr(&classes[index]->function_table, "format", sizeof("format") - 1);
        if (methods[index] == NULL || methods[index]->type != ZEND_INTERNAL_FUNCTION) {
            return;
        }
    }
    /* Both methods are aliases of date_format(); anything else is left alone. */
    if (methods[0]->internal_function.handler != methods[1]->internal_function.handler
        || methods[0]->internal_function.handler == pam_date_format_method) {
        return;
    }
    pam_php_date_format = methods[0]->internal_function.handler;
    methods[0]->internal_function.handler = pam_date_format_method;
    methods[1]->internal_function.handler = pam_date_format_method;
}

int pam_php_init(int argc, char **argv)
{
    zend_result status;

    /*
     * Embed does not normally expose PHP_BINARY and identifies as a web SAPI.
     * Composer deliberately rejects PHAR execution in that mode, while Laravel
     * package providers use CLI identity to register Artisan commands. Pam opts
     * into CLI identity only for explicit CLI processes; HTTP applications
     * continue to report the truthful `embed` SAPI.
     */
    php_embed_module.executable_location = argc > 0 ? argv[0] : NULL;
    if (getenv("PAM_COMPOSER_MODE") != NULL || getenv("PAM_CLI_MODE") != NULL) {
        php_embed_module.name = "cli";
        php_embed_module.pretty_name = "Pam CLI";
    } else {
        php_embed_module.name = "embed";
        php_embed_module.pretty_name = "PHP Embedded Library";
    }
    pam_reset_runtime_cache();
    php_embed_module.additional_functions = pam_shared_functions;
    if (pam_embed_init(argc - 1, argv + 1) == FAILURE) {
        return PAM_INITIALIZATION_FAILURE;
    }

    pam_initialized = true;
    pam_set_php_binary(argc > 0 ? argv[0] : NULL);
    /* Like PHP registers a SAPI's additional functions: under the standard
     * module. Registered without one, opcache's optimizer dereferenced the
     * NULL module when it folded a literal function_exists('pam_native_…')
     * or is_callable() call and the worker crashed compiling that file. */
    EG(current_module) = zend_hash_str_find_ptr(&module_registry, "standard", sizeof("standard") - 1);
    status = zend_register_functions(
        NULL,
        pam_native_functions,
        NULL,
        MODULE_PERSISTENT
    );
    EG(current_module) = NULL;
    if (status == FAILURE) {
        php_embed_shutdown();
        pam_initialized = false;
        return PAM_INITIALIZATION_FAILURE;
    }
    pam_native_functions_registered = true;
    pam_install_date_format();
    if (pam_register_file_handles() == FAILURE) {
        pam_unregister_native_functions();
        php_embed_shutdown();
        pam_initialized = false;
        pam_reset_runtime_cache();
        return PAM_INITIALIZATION_FAILURE;
    }
    return PAM_SUCCESS;
}

int pam_php_eval(
    const char *source,
    size_t source_length,
    const char *source_name
)
{
    int exit_code = PAM_SUCCESS;

    if (!pam_initialized) {
        return PAM_INITIALIZATION_FAILURE;
    }

    zend_first_try {
        if (zend_eval_stringl(
            source,
            source_length,
            NULL,
            source_name
        ) == FAILURE) {
            exit_code = PAM_INITIALIZATION_FAILURE;
        }

        if (EG(exit_status) != PAM_SUCCESS) {
            exit_code = EG(exit_status);
        }
    } zend_catch {
        exit_code = EG(exit_status);
        if (exit_code == PAM_SUCCESS) {
            exit_code = PAM_INITIALIZATION_FAILURE;
        }
    } zend_end_try();

    return exit_code;
}

/*
 * php-cli runs the main script before anything else, so get_included_files()
 * starts with it. PAM preloads the Composer autoloader first; without this
 * the main script was listed after the autoloader's files, and tools that
 * drop the first entry as "the entry script" re-included it. PHPUnit's
 * process isolation (preserveGlobalState) required vendor/bin/phpunit inside
 * every child, which then ran the whole runner again.
 */
static void pam_list_primary_script_first(const char *filename)
{
    char resolved[MAXPATHLEN];
    HashTable reordered;
    zend_string *key;

    if (zend_hash_num_elements(&EG(included_files)) == 0 ||
        VCWD_REALPATH(filename, resolved) == NULL) {
        return;
    }
    zend_hash_init(&reordered, zend_hash_num_elements(&EG(included_files)) + 1, NULL, NULL, 0);
    zend_hash_str_add_empty_element(&reordered, resolved, strlen(resolved));
    ZEND_HASH_FOREACH_STR_KEY(&EG(included_files), key) {
        if (key != NULL) {
            zend_hash_add_empty_element(&reordered, key);
        }
    } ZEND_HASH_FOREACH_END();
    zend_hash_destroy(&EG(included_files));
    EG(included_files) = reordered;
}

int pam_php_execute_file(const char *filename, int primary)
{
    int exit_code = PAM_SUCCESS;

    if (!pam_initialized) {
        return PAM_INITIALIZATION_FAILURE;
    }

    zend_first_try {
        zend_file_handle script;
        if (primary) {
            pam_list_primary_script_first(filename);
        }
        zend_stream_init_filename(&script, filename);
        CG(skip_shebang) = 1;
        pam_reset_sapi_response(200);

        /*
         * php_execute_script() also returns false for a deliberate exit(0).
         * EG(exit_status) is the authoritative CLI-compatible status in that
         * case and for fatal/parse failures. Treating the boolean as an exit
         * code made successful PHPUnit and Pest runs appear to fail.
         */
        (void) php_execute_script(&script);
        exit_code = EG(exit_status);
    } zend_catch {
        exit_code = EG(exit_status);
    } zend_end_try();

    return exit_code;
}

int pam_php_server_config(char **output, size_t *output_length)
{
    int status = FAILURE;
    zval result;
    ZVAL_UNDEF(&result);

    zend_first_try {
        if (pam_runtime_call("serverConfig", 0, NULL, &result) == SUCCESS) {
            status = pam_copy_string_result(&result, output, output_length);
        }
    } zend_catch {
        status = FAILURE;
    } zend_end_try();

    if (!Z_ISUNDEF(result)) {
        zval_ptr_dtor(&result);
    }

    return status;
}

int pam_php_runtime_info(char **output, size_t *output_length)
{
    int status = FAILURE;
    zval result;
    ZVAL_UNDEF(&result);

    zend_first_try {
        if (pam_runtime_call("runtimeInfo", 0, NULL, &result) == SUCCESS) {
            status = pam_copy_string_result(&result, output, output_length);
        }
    } zend_catch {
        status = FAILURE;
    } zend_end_try();

    if (!Z_ISUNDEF(result)) {
        zval_ptr_dtor(&result);
    }

    return status;
}

int pam_php_runtime_metrics(char **output, size_t *output_length)
{
    int status = FAILURE;
    zval result;
    ZVAL_UNDEF(&result);

    zend_first_try {
        if (pam_runtime_call("runtimeMetrics", 0, NULL, &result) == SUCCESS) {
            status = pam_copy_string_result(&result, output, output_length);
        }
    } zend_catch {
        status = FAILURE;
    } zend_end_try();

    if (!Z_ISUNDEF(result)) {
        zval_ptr_dtor(&result);
    }

    return status;
}

int pam_php_runtime_diagnostics(char **output, size_t *output_length)
{
    int status = FAILURE;
    zval result;
    ZVAL_UNDEF(&result);

    zend_first_try {
        if (pam_runtime_call("runtimeDiagnostics", 0, NULL, &result) == SUCCESS) {
            status = pam_copy_string_result(&result, output, output_length);
        }
    } zend_catch {
        status = FAILURE;
    } zend_end_try();

    if (!Z_ISUNDEF(result)) {
        zval_ptr_dtor(&result);
    }
    return status;
}

int pam_php_routes_info(char **output, size_t *output_length)
{
    int status = FAILURE;
    zval result;
    ZVAL_UNDEF(&result);

    zend_first_try {
        if (pam_runtime_call("routesInfo", 0, NULL, &result) == SUCCESS) {
            status = pam_copy_string_result(&result, output, output_length);
        }
    } zend_catch {
        status = FAILURE;
    } zend_end_try();

    if (!Z_ISUNDEF(result)) {
        zval_ptr_dtor(&result);
    }

    return status;
}

/*
 * Runs Runtime::beginHttpDispatch with `parameters` while the SAPI request
 * body (php://input) points at `body`. Takes ownership of the parameters.
 */
static php_stream *pam_request_body_stream(zend_string *body)
{
#if PHP_VERSION_ID >= 80100
    /* A read-only memory stream over the body string PHP already holds as
     * the dispatch argument: php://input costs no second copy, which counts
     * against the worker's shared memory_limit. */
    return php_stream_memory_open(TEMP_STREAM_READONLY, body);
#else
    return php_stream_temp_open(
        TEMP_STREAM_DEFAULT,
        PHP_STREAM_MAX_MEM,
        ZSTR_VAL(body),
        ZSTR_LEN(body)
    );
#endif
}

static int pam_begin_http_dispatch(
    zval *parameters,
    uint32_t parameter_count,
    zend_string *body,
    char **output,
    size_t *output_length
)
{
    size_t body_length = ZSTR_LEN(body);
    int status = FAILURE;
    php_stream *previous_request_body;
    zval result;
    ZVAL_UNDEF(&result);

    /* php_embed considers headers sent after the bootstrap script finishes.
     * Pam dispatches many logical HTTP requests inside that single PHP
     * lifecycle, so each dispatch needs a fresh SAPI header bag. */
    pam_reset_sapi_response(200);

    previous_request_body = SG(request_info).request_body;
    SG(request_info).request_body = body_length == 0 ? NULL : pam_request_body_stream(body);

    if (body_length != 0 && SG(request_info).request_body == NULL) {
        SG(request_info).request_body = previous_request_body;
        for (uint32_t index = 0; index < parameter_count; index++) {
            zval_ptr_dtor(&parameters[index]);
        }
        return FAILURE;
    }

    zend_first_try {
        if (pam_runtime_call("beginHttpDispatch", parameter_count, parameters, &result) == SUCCESS) {
            status = pam_copy_string_result(&result, output, output_length);
        }
    } zend_catch {
        status = FAILURE;
    } zend_end_try();

    if (SG(request_info).request_body != NULL) {
        php_stream_close(SG(request_info).request_body);
    }
    SG(request_info).request_body = previous_request_body;

    for (uint32_t index = 0; index < parameter_count; index++) {
        zval_ptr_dtor(&parameters[index]);
    }
    if (!Z_ISUNDEF(result)) {
        zval_ptr_dtor(&result);
    }

    return status;
}

int pam_php_dispatch_http(
    const char *method,
    size_t method_length,
    const char *target,
    size_t target_length,
    const char *headers,
    size_t headers_length,
    const char *body,
    size_t body_length,
    const char *context,
    size_t context_length,
    const char *request_id,
    size_t request_id_length,
    char **output,
    size_t *output_length
)
{
    zval parameters[6];

    ZVAL_STRINGL(&parameters[0], method, method_length);
    ZVAL_STRINGL(&parameters[1], target, target_length);
    ZVAL_STRINGL(&parameters[2], headers, headers_length);
    ZVAL_STRINGL(&parameters[3], body, body_length);
    ZVAL_STRINGL(&parameters[4], context, context_length);
    ZVAL_STRINGL(&parameters[5], request_id, request_id_length);

    return pam_begin_http_dispatch(parameters, 6, Z_STR(parameters[3]), output, output_length);
}

typedef struct pam_header_reader {
    const unsigned char *cursor;
    const unsigned char *end;
} pam_header_reader;

static bool pam_header_read_u32(pam_header_reader *reader, uint32_t *value)
{
    if ((size_t) (reader->end - reader->cursor) < sizeof(uint32_t)) {
        return false;
    }
    memcpy(value, reader->cursor, sizeof(uint32_t));
    reader->cursor += sizeof(uint32_t);
    return true;
}

static bool pam_header_read_bytes(pam_header_reader *reader, const char **bytes, uint32_t *length)
{
    if (!pam_header_read_u32(reader, length)
        || (size_t) (reader->end - reader->cursor) < (size_t) *length) {
        return false;
    }
    *bytes = (const char *) reader->cursor;
    reader->cursor += *length;
    return true;
}

/*
 * Keys of the per-request arrays ($_SERVER, the request context, header
 * names) live for the whole process: built once, hash precomputed, flagged
 * permanent interned like PHP's own known strings, so adding an entry no
 * longer allocates, hashes and frees its key on every request.
 */
static zend_string *pam_permanent_key(const char *key, size_t key_length)
{
    zend_string *permanent = zend_string_init(key, key_length, 1);
    zend_string_hash_val(permanent);
    GC_ADD_FLAGS(permanent, IS_STR_INTERNED | IS_STR_PERMANENT);
    return permanent;
}

static void pam_hash_add_key(HashTable *table, zend_string *key, const char *value, size_t value_length)
{
    zval entry;
    ZVAL_STRINGL_FAST(&entry, value, value_length);
    zend_hash_update(table, key, &entry);
}

#define PAM_LITERAL_KEY(key) \
    ({ \
        static zend_string *pam_literal_key = NULL; \
        if (UNEXPECTED(pam_literal_key == NULL)) { \
            pam_literal_key = pam_permanent_key((key), sizeof(key) - 1); \
        } \
        pam_literal_key; \
    })

#define PAM_HASH_ADD_LITERAL(table, key, value, value_length) \
    do { \
        static zend_string *pam_literal_key = NULL; \
        if (UNEXPECTED(pam_literal_key == NULL)) { \
            pam_literal_key = pam_permanent_key((key), sizeof(key) - 1); \
        } \
        pam_hash_add_key((table), pam_literal_key, (value), (value_length)); \
    } while (0)

/* Header names seen by this process (lowercase) and their $_SERVER names. */
typedef struct {
    zend_string *name;
    zend_string *server_name;
} pam_header_key;

#define PAM_HEADER_KEY_CAPACITY 64
static pam_header_key pam_header_keys[PAM_HEADER_KEY_CAPACITY];
static size_t pam_header_key_count = 0;

/* $_SERVER name of a lowercase header: CONTENT_TYPE, CONTENT_LENGTH or HTTP_*. */
static zend_string *pam_server_header_name(const char *name, size_t length)
{
    if (length == sizeof("content-type") - 1 && memcmp(name, "content-type", length) == 0) {
        return zend_string_init("CONTENT_TYPE", sizeof("CONTENT_TYPE") - 1, 0);
    }
    if (length == sizeof("content-length") - 1 && memcmp(name, "content-length", length) == 0) {
        return zend_string_init("CONTENT_LENGTH", sizeof("CONTENT_LENGTH") - 1, 0);
    }
    zend_string *server_name = zend_string_alloc(length + 5, 0);
    char *output = ZSTR_VAL(server_name);
    memcpy(output, "HTTP_", 5);
    for (size_t index = 0; index < length; index++) {
        char character = name[index];
        output[index + 5] = character == '-' ? '_' : zend_toupper_ascii(character);
    }
    output[length + 5] = '\0';
    return server_name;
}

/* The cached keys of a header name; NULL once the cache is full. */
static const pam_header_key *pam_header_key_find(const char *name, size_t length)
{
    for (size_t index = 0; index < pam_header_key_count; index++) {
        const pam_header_key *key = &pam_header_keys[index];
        if (ZSTR_LEN(key->name) == length && memcmp(ZSTR_VAL(key->name), name, length) == 0) {
            return key;
        }
    }
    if (pam_header_key_count == PAM_HEADER_KEY_CAPACITY || length > 128) {
        return NULL;
    }
    zend_string *server_name = pam_server_header_name(name, length);
    pam_header_key *key = &pam_header_keys[pam_header_key_count++];
    key->name = pam_permanent_key(name, length);
    key->server_name = pam_permanent_key(ZSTR_VAL(server_name), ZSTR_LEN(server_name));
    zend_string_release(server_name);
    return key;
}

/*
 * Builds the header list (name => list<string>) and appends the header
 * entries of $_SERVER, exactly as Runtime::populateSuperglobals() does.
 */
static bool pam_build_request_headers(
    const char *encoded,
    size_t encoded_length,
    zval *headers,
    HashTable *server
)
{
    pam_header_reader reader = {
        (const unsigned char *) encoded,
        (const unsigned char *) encoded + encoded_length,
    };
    uint32_t name_count = 0;

    if (encoded_length == 0) {
        array_init(headers);
        return true;
    }
    if (!pam_header_read_u32(&reader, &name_count) || name_count > 65535) {
        return false;
    }
    array_init_size(headers, name_count);
    for (uint32_t name_index = 0; name_index < name_count; name_index++) {
        const char *name;
        uint32_t name_length;
        uint32_t value_count;
        zval values;
        smart_str joined = {0};
        zval joined_value;

        if (!pam_header_read_bytes(&reader, &name, &name_length)
            || !pam_header_read_u32(&reader, &value_count)
            || value_count > 65535) {
            return false;
        }
        array_init_size(&values, value_count);
        zend_hash_real_init_packed(Z_ARRVAL(values));
        for (uint32_t value_index = 0; value_index < value_count; value_index++) {
            const char *value;
            uint32_t value_length;
            zval entry;
            if (!pam_header_read_bytes(&reader, &value, &value_length)) {
                zval_ptr_dtor(&values);
                smart_str_free(&joined);
                return false;
            }
            ZVAL_STRINGL_FAST(&entry, value, value_length);
            zend_hash_next_index_insert_new(Z_ARRVAL(values), &entry);
            if (value_count > 1) {
                if (value_index > 0) {
                    smart_str_appendl(&joined, ", ", 2);
                }
                smart_str_appendl(&joined, value, value_length);
            }
        }

        if (value_count == 0) {
            ZVAL_EMPTY_STRING(&joined_value);
        } else if (value_count == 1) {
            ZVAL_COPY(&joined_value, zend_hash_index_find(Z_ARRVAL(values), 0));
        } else {
            ZVAL_STR(&joined_value, smart_str_extract(&joined));
        }
        const pam_header_key *key = pam_header_key_find(name, name_length);
        if (key != NULL) {
            zend_hash_update(server, key->server_name, &joined_value);
            zend_hash_update(Z_ARRVAL_P(headers), key->name, &values);
        } else {
            zend_string *server_name = pam_server_header_name(name, name_length);
            zend_hash_update(server, server_name, &joined_value);
            zend_string_release(server_name);
            zend_hash_str_update(Z_ARRVAL_P(headers), name, name_length, &values);
        }
    }
    return reader.cursor == reader.end;
}

int pam_php_dispatch_http_native(
    const char *method,
    size_t method_length,
    const char *target,
    size_t target_length,
    const char *headers,
    size_t headers_length,
    const char *body,
    size_t body_length,
    const pam_http_request_context *context,
    const char *request_id,
    size_t request_id_length,
    char **output,
    size_t *output_length
)
{
    zval parameters[7];
    HashTable *server;
    HashTable *context_table;
    php_url *url;
    zend_string *upper_method;
    const char *protocol_version = context->protocol_version_length == 0
        ? "1.1"
        : context->protocol_version;
    size_t protocol_version_length = context->protocol_version_length == 0
        ? 3
        : context->protocol_version_length;
    const char *scheme = context->scheme_length == 0 ? "http" : context->scheme;
    size_t scheme_length = context->scheme_length == 0 ? 4 : context->scheme_length;
    char server_protocol[32];
    int server_protocol_length;
    const char *public_request_id = context->public_request_id_length == 0
        ? request_id
        : context->public_request_id;
    size_t public_request_id_length = context->public_request_id_length == 0
        ? request_id_length
        : context->public_request_id_length;

    {
        zend_string *raw_method = zend_string_init_fast(method, method_length);
        upper_method = zend_string_toupper(raw_method);
        zend_string_release(raw_method);
    }

    /* $_SERVER, in Runtime::populateSuperglobals() order. */
    array_init_size(&parameters[6], 24);
    server = Z_ARRVAL(parameters[6]);
    {
        zval entry;
        ZVAL_STR_COPY(&entry, upper_method);
        zend_hash_update(server, PAM_LITERAL_KEY("REQUEST_METHOD"), &entry);
    }
    PAM_HASH_ADD_LITERAL(server, "REQUEST_URI", target, target_length);

    /* parse_url($target, PHP_URL_QUERY|PHP_URL_PATH) */
    url = php_url_parse_ex2(target, target_length, &(bool){false});
    if (url != NULL && url->query != NULL) {
        PAM_HASH_ADD_LITERAL(server, "QUERY_STRING", ZSTR_VAL(url->query), ZSTR_LEN(url->query));
    } else {
        PAM_HASH_ADD_LITERAL(server, "QUERY_STRING", "", 0);
    }
    {
        /* Not snprintf("%.*s"): PHP's snprintf runs strlen() on the
         * argument before applying the precision, and protocol_version is
         * not NUL terminated (an out-of-bounds read under ASan). */
        size_t version_length = protocol_version_length > 16 ? 16 : protocol_version_length;
        memcpy(server_protocol, "HTTP/", 5);
        memcpy(server_protocol + 5, protocol_version, version_length);
        server_protocol_length = (int) (5 + version_length);
    }
    PAM_HASH_ADD_LITERAL(server, "SERVER_PROTOCOL", server_protocol, (size_t) server_protocol_length);
    PAM_HASH_ADD_LITERAL(server, "REMOTE_ADDR", context->remote_address, context->remote_address_length);
    {
        zval entry;
        ZVAL_LONG(&entry, (zend_long) context->remote_port);
        zend_hash_update(server, PAM_LITERAL_KEY("REMOTE_PORT"), &entry);
    }
    PAM_HASH_ADD_LITERAL(server, "REQUEST_SCHEME", scheme, scheme_length);
    if (scheme_length == 5 && memcmp(scheme, "https", 5) == 0) {
        PAM_HASH_ADD_LITERAL(server, "HTTPS", "on", 2);
    } else {
        PAM_HASH_ADD_LITERAL(server, "HTTPS", "off", 3);
    }
    /* Placeholder keeps populateSuperglobals()' key order; the host header
     * (if any) overwrites it in place below. */
    PAM_HASH_ADD_LITERAL(server, "HTTP_HOST", "", 0);
    PAM_HASH_ADD_LITERAL(server, "PAM_REQUEST_ID", public_request_id, public_request_id_length);
    PAM_HASH_ADD_LITERAL(server, "PAM_TRACEPARENT", context->traceparent, context->traceparent_length);

    if (!pam_build_request_headers(headers, headers_length, &parameters[2], server)) {
        if (url != NULL) {
            php_url_free(url);
        }
        zend_string_release(upper_method);
        zval_ptr_dtor(&parameters[2]);
        zval_ptr_dtor(&parameters[6]);
        return FAILURE;
    }

    /* Request context: the keys Runtime::executeHttp() reads, plus the
     * already parsed path. multipart stays JSON (only uploads carry it). */
    array_init_size(&parameters[4], 8);
    context_table = Z_ARRVAL(parameters[4]);
    PAM_HASH_ADD_LITERAL(context_table, "protocolVersion", protocol_version, protocol_version_length);
    PAM_HASH_ADD_LITERAL(context_table, "remoteAddress", context->remote_address, context->remote_address_length);
    {
        zval entry;
        ZVAL_LONG(&entry, (zend_long) context->remote_port);
        zend_hash_update(context_table, PAM_LITERAL_KEY("remotePort"), &entry);
    }
    PAM_HASH_ADD_LITERAL(context_table, "scheme", scheme, scheme_length);
    PAM_HASH_ADD_LITERAL(context_table, "requestId", public_request_id, public_request_id_length);
    PAM_HASH_ADD_LITERAL(context_table, "traceparent", context->traceparent, context->traceparent_length);
    if (url != NULL && url->path != NULL && ZSTR_LEN(url->path) != 0
        && !zend_string_equals_literal(url->path, "0")) {
        PAM_HASH_ADD_LITERAL(context_table, "path", ZSTR_VAL(url->path), ZSTR_LEN(url->path));
    } else {
        PAM_HASH_ADD_LITERAL(context_table, "path", "/", 1);
    }
    if (context->multipart_json_length != 0) {
        PAM_HASH_ADD_LITERAL(
            context_table,
            "multipartJson",
            context->multipart_json,
            context->multipart_json_length
        );
    }
    if (url != NULL) {
        php_url_free(url);
    }

    ZVAL_STR(&parameters[0], upper_method);
    ZVAL_STRINGL_FAST(&parameters[1], target, target_length);
    ZVAL_STRINGL_FAST(&parameters[3], body, body_length);
    ZVAL_STRINGL_FAST(&parameters[5], request_id, request_id_length);

    return pam_begin_http_dispatch(parameters, 7, Z_STR(parameters[3]), output, output_length);
}

int pam_php_resume_http(
    const char *request_id,
    size_t request_id_length,
    const char *body,
    size_t body_length,
    const char *operation_result,
    size_t operation_result_length,
    char **output,
    size_t *output_length
)
{
    int status = FAILURE;
    php_stream *previous_request_body;
    zval parameters[2];
    zval result;
    ZVAL_UNDEF(&result);
    ZVAL_STRINGL(&parameters[0], request_id, request_id_length);
    ZVAL_STRINGL(&parameters[1], operation_result, operation_result_length);

    pam_reset_sapi_response(200);

    previous_request_body = SG(request_info).request_body;
    SG(request_info).request_body = body_length == 0
        ? NULL
        : php_stream_temp_open(
            TEMP_STREAM_DEFAULT,
            PHP_STREAM_MAX_MEM,
            body,
            body_length
        );

    if (body_length != 0 && SG(request_info).request_body == NULL) {
        SG(request_info).request_body = previous_request_body;
        zval_ptr_dtor(&parameters[0]);
        zval_ptr_dtor(&parameters[1]);
        return FAILURE;
    }

    zend_first_try {
        if (pam_runtime_call("resumeHttpDispatch", 2, parameters, &result) == SUCCESS) {
            status = pam_copy_string_result(&result, output, output_length);
        }
    } zend_catch {
        status = FAILURE;
    } zend_end_try();

    if (SG(request_info).request_body != NULL) {
        php_stream_close(SG(request_info).request_body);
    }
    SG(request_info).request_body = previous_request_body;
    zval_ptr_dtor(&parameters[0]);
    zval_ptr_dtor(&parameters[1]);
    if (!Z_ISUNDEF(result)) {
        zval_ptr_dtor(&result);
    }
    return status;
}

static void pam_uploaded_file_dtor(zval *element)
{
    zend_string_release_ex(Z_STR_P(element), 0);
}

/* Registers a temporary file the transport streamed a multipart upload into,
 * exactly as PHP's own rfc1867 handler does, so is_uploaded_file() and
 * move_uploaded_file() accept it. */
int pam_php_register_upload(const char *path, size_t path_length)
{
    zend_string *name;

    if (SG(rfc1867_uploaded_files) == NULL) {
        ALLOC_HASHTABLE(SG(rfc1867_uploaded_files));
        zend_hash_init(SG(rfc1867_uploaded_files), 8, NULL, pam_uploaded_file_dtor, 0);
    }
    name = zend_string_init(path, path_length, 0);
    if (zend_hash_add_ptr(SG(rfc1867_uploaded_files), name, name) == NULL) {
        zend_string_release_ex(name, 0);
        return FAILURE;
    }
    return SUCCESS;
}

/* Forgets an upload at the end of its request. Returns 1 when the file was
 * still registered (the application did not move it) and must be deleted. */
int pam_php_release_upload(const char *path, size_t path_length)
{
    if (SG(rfc1867_uploaded_files) == NULL) {
        return 0;
    }
    return zend_hash_str_del(SG(rfc1867_uploaded_files), path, path_length) == SUCCESS ? 1 : 0;
}

int pam_php_cancel_http(const char *request_id, size_t request_id_length)
{
    int status = FAILURE;
    zval parameter;
    zval result;
    ZVAL_UNDEF(&result);
    ZVAL_STRINGL(&parameter, request_id, request_id_length);

    zend_first_try {
        status = pam_runtime_call("cancelHttpDispatch", 1, &parameter, &result);
    } zend_catch {
        status = FAILURE;
    } zend_end_try();

    zval_ptr_dtor(&parameter);
    if (!Z_ISUNDEF(result)) {
        zval_ptr_dtor(&result);
    }
    return status;
}

int pam_php_dispatch_ws(
    int event_kind,
    const char *socket_id,
    size_t socket_id_length,
    const char *payload,
    size_t payload_length,
    char **output,
    size_t *output_length
)
{
    int status = FAILURE;
    zval parameters[3];
    zval result;
    ZVAL_UNDEF(&result);

    ZVAL_LONG(&parameters[0], event_kind);
    ZVAL_STRINGL(&parameters[1], socket_id, socket_id_length);
    ZVAL_STRINGL(&parameters[2], payload, payload_length);

    zend_first_try {
        if (pam_runtime_call("dispatchWs", 3, parameters, &result) == SUCCESS) {
            status = pam_copy_string_result(&result, output, output_length);
        }
    } zend_catch {
        status = FAILURE;
    } zend_end_try();

    for (size_t index = 0; index < 3; index++) {
        zval_ptr_dtor(&parameters[index]);
    }
    if (!Z_ISUNDEF(result)) {
        zval_ptr_dtor(&result);
    }

    return status;
}

void pam_php_string_free(char *value)
{
    free(value);
}

/*
 * A worker forked from a booted zygote inherits the zygote's PRNG states and
 * would draw the same mt_rand()/rand()/shuffle()/lcg_value()/uniqid() values
 * as every sibling. Clearing the "seeded" flags makes each process reseed
 * lazily from the CSPRNG, exactly like a fresh process. random_bytes() and
 * random_int() read the kernel CSPRNG and carry no state. setitimer() based
 * max_execution_time timers are not inherited by fork(2), so a configured
 * limit is re-armed here.
 */
void pam_php_after_fork(void)
{
    if (!pam_initialized) {
        return;
    }
#if PHP_VERSION_ID >= 80200
    RANDOM_G(mt19937_seeded) = false;
    RANDOM_G(combined_lcg_seeded) = false;
#endif
#if PHP_VERSION_ID >= 80400
    RANDOM_G(fallback_seed_state).initialized = false;
#endif
    if (EG(timeout_seconds) > 0) {
        zend_set_timeout(EG(timeout_seconds), 0);
    }
}

void pam_php_shutdown(void)
{
    if (!pam_initialized) {
        return;
    }

    /*
     * The last request may have been cut short by a fatal error (memory_limit)
     * before PAM reset its response state, leaving a raw `HTTP/` status line.
     * Shutdown functions (Laravel's, then Whoops' through Collision) handle
     * that fatal again and call http_response_code(), which PHP 8.5 turns into
     * a warning, and Whoops into a second, uncaught fatal. The response was
     * already answered by the server; start shutdown from a clean slate.
     */
    if (SG(sapi_started)) {
        pam_reset_sapi_response(200);
    }

    pam_unregister_native_functions();
    php_embed_shutdown();
    php_embed_module.ini_entries = NULL;
    free(pam_ini_entries);
    pam_ini_entries = NULL;
    pam_reset_runtime_cache();
    pam_initialized = false;
}
