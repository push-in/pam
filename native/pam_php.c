#include <sapi/embed/php_embed.h>
#include <main/php_memory_streams.h>
#include <main/php_network.h>

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
static char *pam_ini_entries = NULL;
static zend_class_entry *pam_runtime_class = NULL;
static zend_function *pam_server_config_function = NULL;
static zend_function *pam_runtime_info_function = NULL;
static zend_function *pam_runtime_metrics_function = NULL;
static zend_function *pam_runtime_diagnostics_function = NULL;
static zend_function *pam_routes_info_function = NULL;
static zend_function *pam_contracts_info_function = NULL;
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

static const zend_function_entry pam_native_functions[] = {
    PHP_FE(pam_native_abi_version, arginfo_pam_native_abi_version)
    PHP_FE(pam_native_stream_fd, arginfo_pam_native_stream_fd)
    PHP_FE_END
};

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
    pam_contracts_info_function = NULL;
    pam_begin_http_dispatch_function = NULL;
    pam_resume_http_dispatch_function = NULL;
    pam_cancel_http_dispatch_function = NULL;
    pam_dispatch_ws_function = NULL;
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
    } else if (strcmp(method, "contractsInfo") == 0) {
        cache = &pam_contracts_info_function;
        normalized = "contractsinfo";
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
    if (pam_register_file_handles() == FAILURE) {
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
        sapi_header_op(SAPI_HEADER_DELETE_ALL, NULL);
        SG(headers_sent) = 0;
        SG(sapi_headers).http_response_code = 200;
        SG(sapi_headers).send_default_content_type = 0;

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

int pam_php_contracts_info(char **output, size_t *output_length)
{
    int status = FAILURE;
    zval result;
    ZVAL_UNDEF(&result);

    zend_first_try {
        if (pam_runtime_call("contractsInfo", 0, NULL, &result) == SUCCESS) {
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
    sapi_header_op(SAPI_HEADER_DELETE_ALL, NULL);
    SG(headers_sent) = 0;
    SG(sapi_headers).http_response_code = 200;
    SG(sapi_headers).send_default_content_type = 0;

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

    sapi_header_op(SAPI_HEADER_DELETE_ALL, NULL);
    SG(headers_sent) = 0;
    SG(sapi_headers).http_response_code = 200;
    SG(sapi_headers).send_default_content_type = 0;

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

void pam_php_shutdown(void)
{
    if (!pam_initialized) {
        return;
    }

    php_embed_shutdown();
    php_embed_module.ini_entries = NULL;
    free(pam_ini_entries);
    pam_ini_entries = NULL;
    pam_reset_runtime_cache();
    pam_initialized = false;
}
