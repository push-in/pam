/* Fuzz harness side of the C header reader (native/pam_header_reader.h). */
#include "../../native/pam_header_reader.h"

typedef struct {
    uint32_t names;
    uint32_t values;
    uint64_t bytes;
} pam_fuzz_header_counts;

static void pam_fuzz_count(void *state, const char *name, uint32_t name_length, const char *value, uint32_t value_length)
{
    pam_fuzz_header_counts *counts = state;
    volatile unsigned char sink = 0;
    /* Touch every byte so ASan sees any read past the buffer. */
    for (uint32_t index = 0; index < name_length; index++) {
        sink ^= (unsigned char) name[index];
    }
    if (value == NULL) {
        counts->names++;
        counts->bytes += name_length;
        return;
    }
    for (uint32_t index = 0; index < value_length; index++) {
        sink ^= (unsigned char) value[index];
    }
    (void) sink;
    counts->values++;
    counts->bytes += value_length;
}

/* Returns 1 for a well formed list and fills the counts. */
int pam_fuzz_walk_headers(const char *encoded, size_t length, uint32_t *names, uint32_t *values, uint64_t *bytes)
{
    pam_fuzz_header_counts counts = {0, 0, 0};
    int valid = pam_header_walk(encoded, length, pam_fuzz_count, &counts) ? 1 : 0;
    *names = counts.names;
    *values = counts.values;
    *bytes = counts.bytes;
    return valid;
}
