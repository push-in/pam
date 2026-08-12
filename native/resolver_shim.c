/*
 * PHP's DNS implementation references underscored resolver symbols that
 * newer glibc linkers no longer match to their versioned compatibility
 * aliases. Bridge those names to the public resolver API for static SDKs.
 */
#include <resolv.h>

int __dn_expand(
    const unsigned char *message,
    const unsigned char *end,
    const unsigned char *compressed,
    char *expanded,
    int length
)
{
    return dn_expand(message, end, compressed, expanded, length);
}

int __dn_skipname(const unsigned char *compressed, const unsigned char *end)
{
    return dn_skipname(compressed, end);
}

int __res_nsearch(
    res_state state,
    const char *domain,
    int class,
    int type,
    unsigned char *answer,
    int answer_length
)
{
    return res_nsearch(state, domain, class, type, answer, answer_length);
}
