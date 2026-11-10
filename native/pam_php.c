#include <sapi/embed/php_embed.h>
#include <main/php_memory_streams.h>
#include <main/php_network.h>
#if PHP_VERSION_ID >= 80200
#include <ext/random/php_random.h>
#endif
#include <Zend/zend_smart_str.h>

#include "pam.h"

#include <stdbool.h>
#include <signal.h>
#include <stdlib.h>
#include <string.h>

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

#include "pam_shared_php.inc"

static const zend_function_entry pam_native_functions[] = {
    PHP_FE(pam_cache_purge, arginfo_pam_cache_purge)
    PHP_FE(pam_native_abi_version, arginfo_pam_native_abi_version)
    PHP_FE(pam_native_stream_fd, arginfo_pam_native_stream_fd)
    PHP_FE(pam_native_reset_response, arginfo_pam_native_reset_response)
    PHP_FE_END
};

/*
 * Shared tables and task workers stay registered until the engine itself
 * shuts down: objects destructed and shutdown functions run during
 * php_request_shutdown() (an Octane WorkerStopping listener, a table's
 * destructor) may still call them, and their call sites cache the function.
 * zend_shutdown() releases them with the rest of the function table.
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

int pam_php_init(int argc, char **argv)
{
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
    if (pam_embed_init(argc - 1, argv + 1) == FAILURE) {
        return PAM_INITIALIZATION_FAILURE;
    }

    pam_initialized = true;
    pam_set_php_binary(argc > 0 ? argv[0] : NULL);
    if (zend_register_functions(
        NULL,
        pam_native_functions,
        NULL,
        MODULE_PERSISTENT
    ) == FAILURE) {
        php_embed_shutdown();
        pam_initialized = false;
        return PAM_INITIALIZATION_FAILURE;
    }
    pam_native_functions_registered = true;
    if (zend_register_functions(NULL, pam_shared_functions, NULL, MODULE_PERSISTENT) == FAILURE) {
        pam_unregister_native_functions();
        php_embed_shutdown();
        pam_initialized = false;
        return PAM_INITIALIZATION_FAILURE;
    }
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

int pam_php_execute_file(const char *filename)
{
    int exit_code = PAM_SUCCESS;

    if (!pam_initialized) {
        return PAM_INITIALIZATION_FAILURE;
    }

    zend_first_try {
        zend_file_handle script;
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
    int status = FAILURE;
    php_stream *previous_request_body;
    zval parameters[6];
    zval result;
    ZVAL_UNDEF(&result);

    ZVAL_STRINGL(&parameters[0], method, method_length);
    ZVAL_STRINGL(&parameters[1], target, target_length);
    ZVAL_STRINGL(&parameters[2], headers, headers_length);
    ZVAL_STRINGL(&parameters[3], body, body_length);
    ZVAL_STRINGL(&parameters[4], context, context_length);
    ZVAL_STRINGL(&parameters[5], request_id, request_id_length);

    /* php_embed considers headers sent after the bootstrap script finishes.
     * Pam dispatches many logical HTTP requests inside that single PHP
     * lifecycle, so each dispatch needs a fresh SAPI header bag. */
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
        for (size_t index = 0; index < 6; index++) {
            zval_ptr_dtor(&parameters[index]);
        }
        return FAILURE;
    }

    zend_first_try {
        if (pam_runtime_call("beginHttpDispatch", 6, parameters, &result) == SUCCESS) {
            status = pam_copy_string_result(&result, output, output_length);
        }
    } zend_catch {
        status = FAILURE;
    } zend_end_try();

    if (SG(request_info).request_body != NULL) {
        php_stream_close(SG(request_info).request_body);
    }
    SG(request_info).request_body = previous_request_body;

    for (size_t index = 0; index < 6; index++) {
        zval_ptr_dtor(&parameters[index]);
    }
    if (!Z_ISUNDEF(result)) {
        zval_ptr_dtor(&result);
    }

    return status;
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

    pam_unregister_native_functions();
    php_embed_shutdown();
    php_embed_module.ini_entries = NULL;
    free(pam_ini_entries);
    pam_ini_entries = NULL;
    pam_reset_runtime_cache();
    pam_initialized = false;
}
