#ifndef PAM_NATIVE_H
#define PAM_NATIVE_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define PAM_NATIVE_ABI_VERSION 1u

enum pam_native_status {
    PAM_NATIVE_SUCCESS = 0,
    PAM_NATIVE_FAILURE = 1,
    PAM_NATIVE_INITIALIZATION_FAILURE = 70,
};

uint32_t pam_native_abi_version(void);
int pam_php_init(int argc, char **argv);
int pam_php_eval(const char *source, size_t source_length, const char *source_name);
int pam_php_execute_file(const char *filename);
int pam_php_server_config(char **output, size_t *output_length);
int pam_php_runtime_info(char **output, size_t *output_length);
int pam_php_runtime_metrics(char **output, size_t *output_length);
int pam_php_runtime_diagnostics(char **output, size_t *output_length);
int pam_php_routes_info(char **output, size_t *output_length);
int pam_php_contracts_info(char **output, size_t *output_length);
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
);
int pam_php_resume_http(
    const char *request_id,
    size_t request_id_length,
    const char *body,
    size_t body_length,
    const char *operation_result,
    size_t operation_result_length,
    char **output,
    size_t *output_length
);
int pam_php_cancel_http(const char *request_id, size_t request_id_length);
int pam_php_dispatch_ws(
    int event_kind,
    const char *socket_id,
    size_t socket_id_length,
    const char *payload,
    size_t payload_length,
    char **output,
    size_t *output_length
);
void pam_php_string_free(char *value);
void pam_php_shutdown(void);

#ifdef __cplusplus
}
#endif

#endif
