/*
 * Reader of the request header list the Rust transport encodes for PHP
 * (`encode_php_request_headers` in src/server.rs): a native-endian u32 name
 * count, then per name its length-prefixed bytes, a u32 value count and the
 * length-prefixed values. Kept free of Zend so the fuzz harness in
 * fuzz/native/ compiles exactly this code with sanitizers.
 */
#ifndef PAM_HEADER_READER_H
#define PAM_HEADER_READER_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

#define PAM_HEADER_MAX_COUNT 65535

typedef struct pam_header_reader {
    const unsigned char *cursor;
    const unsigned char *end;
} pam_header_reader;

static inline bool pam_header_read_u32(pam_header_reader *reader, uint32_t *value)
{
    if ((size_t) (reader->end - reader->cursor) < sizeof(uint32_t)) {
        return false;
    }
    memcpy(value, reader->cursor, sizeof(uint32_t));
    reader->cursor += sizeof(uint32_t);
    return true;
}

static inline bool pam_header_read_bytes(pam_header_reader *reader, const char **bytes, uint32_t *length)
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
 * Walks an encoded list with the same bounds pam_build_request_headers()
 * applies, calling visit() for every name and value (value NULL marks a
 * name). Returns false on any malformed or trailing byte.
 */
static inline bool pam_header_walk(
    const char *encoded,
    size_t encoded_length,
    void (*visit)(void *state, const char *name, uint32_t name_length, const char *value, uint32_t value_length),
    void *state
)
{
    pam_header_reader reader = {
        (const unsigned char *) encoded,
        (const unsigned char *) encoded + encoded_length,
    };
    uint32_t name_count = 0;

    if (encoded_length == 0) {
        return true;
    }
    if (!pam_header_read_u32(&reader, &name_count) || name_count > PAM_HEADER_MAX_COUNT) {
        return false;
    }
    for (uint32_t name_index = 0; name_index < name_count; name_index++) {
        const char *name;
        uint32_t name_length;
        uint32_t value_count;
        if (!pam_header_read_bytes(&reader, &name, &name_length)
            || !pam_header_read_u32(&reader, &value_count)
            || value_count > PAM_HEADER_MAX_COUNT) {
            return false;
        }
        visit(state, name, name_length, NULL, 0);
        for (uint32_t value_index = 0; value_index < value_count; value_index++) {
            const char *value;
            uint32_t value_length;
            if (!pam_header_read_bytes(&reader, &value, &value_length)) {
                return false;
            }
            visit(state, name, name_length, value, value_length);
        }
    }
    return reader.cursor == reader.end;
}

#endif
