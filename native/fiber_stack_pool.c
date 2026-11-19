/*
 * Per-process pool of PHP Fiber C stacks (Linux).
 *
 * PHP's zend_fiber_stack_allocate() maps a fresh stack for every Fiber
 * (mmap + madvise(MADV_NOHUGEPAGE) + prctl(PR_SET_VMA_ANON_NAME) + mprotect
 * of the guard page) and zend_fiber_stack_free() unmaps it: five syscalls
 * plus the page faults of touching new stack pages, for every request Pam
 * dispatches (each request runs in its own Fiber). Pam links the distro's
 * libphp.so and does not compile PHP, so zend_fibers.c cannot be patched.
 *
 * Instead the executable defines mmap/munmap/mprotect/madvise/prctl, which
 * the dynamic linker binds libphp.so's (and libphp.a's) calls to. Every call
 * goes straight to the kernel except Fiber stacks, recognised by their exact
 * mmap fingerprint (NULL hint, PROT_READ|PROT_WRITE,
 * MAP_PRIVATE|MAP_ANONYMOUS|MAP_STACK, fd -1, offset 0; no other mmap in
 * libphp uses MAP_STACK, and glibc's own thread stacks use its internal
 * mmap). On munmap a tracked stack goes back to a small pool instead of the
 * kernel; the next Fiber stack mmap of the same length gets it back with
 * its guard page, name and huge page advice already in place, so the three
 * configuration calls PHP makes on it are answered without a syscall.
 *
 * Only C stack memory is reused: no Fiber object, VM stack or PHP state
 * survives (zend_fiber_stack and the Fiber's VM stack are reallocated by
 * PHP as usual). Stacks are not zeroed, like any C stack. The pool holds at
 * most PAM_FIBER_STACK_POOL_CAPACITY stacks (default 8, 0 disables);
 * PAM_FIBER_STACK_POOL=0 turns the whole mechanism off.
 */
#define _GNU_SOURCE

#include <errno.h>
#include <pthread.h>
#include <stdarg.h>
#include <stdatomic.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <sys/prctl.h>
#include <sys/syscall.h>
#include <unistd.h>

#ifndef PR_SET_VMA
#define PR_SET_VMA 0x53564d41
#endif
#ifndef PR_SET_VMA_ANON_NAME
#define PR_SET_VMA_ANON_NAME 0
#endif

#define PAM_FIBER_POOL_MAX 64
#define PAM_FIBER_TRACK_MAX 256

enum {
    PAM_PENDING_MADVISE = 1,
    PAM_PENDING_NAME = 2,
    PAM_PENDING_GUARD = 4,
};

typedef struct pam_fiber_region {
    void *base;
    size_t length;
    unsigned pending;
} pam_fiber_region;

static atomic_flag pam_fiber_lock = ATOMIC_FLAG_INIT;
/* -1 unknown, 0 disabled, otherwise the pool capacity. */
static int pam_fiber_capacity = -1;
static pam_fiber_region pam_fiber_pool[PAM_FIBER_POOL_MAX];
static int pam_fiber_pool_count = 0;
/* Live stacks handed to PHP (fresh or reused). */
static pam_fiber_region pam_fiber_live[PAM_FIBER_TRACK_MAX];
static atomic_int pam_fiber_live_count = 0;
static atomic_ulong pam_fiber_reuses = 0;

static void pam_fiber_acquire(void)
{
    while (atomic_flag_test_and_set_explicit(&pam_fiber_lock, memory_order_acquire)) {
    }
}

static void pam_fiber_release(void)
{
    atomic_flag_clear_explicit(&pam_fiber_lock, memory_order_release);
}

static int pam_fiber_configured_capacity(void)
{
    if (pam_fiber_capacity < 0) {
        const char *enabled = getenv("PAM_FIBER_STACK_POOL");
        const char *capacity = getenv("PAM_FIBER_STACK_POOL_CAPACITY");
        int value = 8;
        if (capacity != NULL && *capacity != '\0') {
            value = atoi(capacity);
        }
        if (enabled != NULL && enabled[0] == '0' && enabled[1] == '\0') {
            value = 0;
        }
        if (value < 0) {
            value = 0;
        }
        if (value > PAM_FIBER_POOL_MAX) {
            value = PAM_FIBER_POOL_MAX;
        }
        pam_fiber_capacity = value;
    }
    return pam_fiber_capacity;
}

static void pam_fiber_track(void *base, size_t length, unsigned pending)
{
    for (int index = 0; index < PAM_FIBER_TRACK_MAX; index++) {
        if (pam_fiber_live[index].base == NULL) {
            pam_fiber_live[index].base = base;
            pam_fiber_live[index].length = length;
            pam_fiber_live[index].pending = pending;
            atomic_fetch_add_explicit(&pam_fiber_live_count, 1, memory_order_relaxed);
            return;
        }
    }
    /* Table full: the stack stays untracked and is unmapped normally. */
}

static pam_fiber_region *pam_fiber_find(void *base)
{
    for (int index = 0; index < PAM_FIBER_TRACK_MAX; index++) {
        if (pam_fiber_live[index].base == base) {
            return &pam_fiber_live[index];
        }
    }
    return NULL;
}

/* Answers one configuration call PHP makes on a reused stack. */
static bool pam_fiber_consume_pending(void *base, size_t length, unsigned flag)
{
    bool consumed = false;
    if (atomic_load_explicit(&pam_fiber_live_count, memory_order_relaxed) == 0) {
        return false;
    }
    pam_fiber_acquire();
    pam_fiber_region *region = pam_fiber_find(base);
    if (region != NULL && (region->pending & flag) != 0
        && (flag == PAM_PENDING_GUARD || region->length == length)) {
        region->pending &= ~flag;
        consumed = true;
    }
    pam_fiber_release();
    return consumed;
}

static bool pam_fiber_stack_fingerprint(void *address, size_t length, int protection, int flags, int descriptor, off_t offset)
{
    return address == NULL
        && length != 0
        && protection == (PROT_READ | PROT_WRITE)
        && flags == (MAP_PRIVATE | MAP_ANONYMOUS | MAP_STACK)
        && descriptor == -1
        && offset == 0;
}

void *mmap(void *address, size_t length, int protection, int flags, int descriptor, off_t offset)
{
    if (pam_fiber_stack_fingerprint(address, length, protection, flags, descriptor, offset)
        && pam_fiber_configured_capacity() > 0) {
        void *reused = NULL;
        pam_fiber_acquire();
        for (int index = pam_fiber_pool_count - 1; index >= 0; index--) {
            if (pam_fiber_pool[index].length == length) {
                reused = pam_fiber_pool[index].base;
                pam_fiber_pool[index] = pam_fiber_pool[pam_fiber_pool_count - 1];
                pam_fiber_pool_count--;
                break;
            }
        }
        if (reused != NULL) {
            pam_fiber_track(reused, length, PAM_PENDING_MADVISE | PAM_PENDING_NAME | PAM_PENDING_GUARD);
            pam_fiber_release();
            atomic_fetch_add_explicit(&pam_fiber_reuses, 1, memory_order_relaxed);
            return reused;
        }
        pam_fiber_release();
        void *mapped = (void *) syscall(SYS_mmap, address, length, protection, flags, descriptor, offset);
        if (mapped != MAP_FAILED) {
            pam_fiber_acquire();
            pam_fiber_track(mapped, length, 0);
            pam_fiber_release();
        }
        return mapped;
    }
    return (void *) syscall(SYS_mmap, address, length, protection, flags, descriptor, offset);
}

int munmap(void *address, size_t length)
{
    if (atomic_load_explicit(&pam_fiber_live_count, memory_order_relaxed) != 0) {
        pam_fiber_acquire();
        pam_fiber_region *region = pam_fiber_find(address);
        if (region != NULL && region->length == length) {
            /* A stack whose configuration calls did not all happen (PHP
             * failed between mmap and mprotect) is not pooled. */
            bool poolable = region->pending == 0
                && pam_fiber_pool_count < pam_fiber_configured_capacity();
            region->base = NULL;
            region->length = 0;
            region->pending = 0;
            atomic_fetch_sub_explicit(&pam_fiber_live_count, 1, memory_order_relaxed);
            if (poolable) {
                pam_fiber_pool[pam_fiber_pool_count].base = address;
                pam_fiber_pool[pam_fiber_pool_count].length = length;
                pam_fiber_pool[pam_fiber_pool_count].pending = 0;
                pam_fiber_pool_count++;
                pam_fiber_release();
                return 0;
            }
        } else if (region != NULL) {
            /* Partial or larger unmap of a tracked stack: stop tracking. */
            region->base = NULL;
            region->length = 0;
            region->pending = 0;
            atomic_fetch_sub_explicit(&pam_fiber_live_count, 1, memory_order_relaxed);
        }
        pam_fiber_release();
    }
    return (int) syscall(SYS_munmap, address, length);
}

int mprotect(void *address, size_t length, int protection)
{
    if (protection == PROT_NONE && pam_fiber_consume_pending(address, length, PAM_PENDING_GUARD)) {
        return 0;
    }
    return (int) syscall(SYS_mprotect, address, length, protection);
}

int madvise(void *address, size_t length, int advice)
{
#ifdef MADV_NOHUGEPAGE
    if (advice == MADV_NOHUGEPAGE && pam_fiber_consume_pending(address, length, PAM_PENDING_MADVISE)) {
        return 0;
    }
#endif
    return (int) syscall(SYS_madvise, address, length, advice);
}

int prctl(int option, ...)
{
    va_list arguments;
    va_start(arguments, option);
    unsigned long second = va_arg(arguments, unsigned long);
    unsigned long third = va_arg(arguments, unsigned long);
    unsigned long fourth = va_arg(arguments, unsigned long);
    unsigned long fifth = va_arg(arguments, unsigned long);
    va_end(arguments);

    if (option == PR_SET_VMA && second == PR_SET_VMA_ANON_NAME
        && pam_fiber_consume_pending((void *) third, (size_t) fourth, PAM_PENDING_NAME)) {
        return 0;
    }
    return (int) syscall(SYS_prctl, option, second, third, fourth, fifth);
}

/* A fork while another thread holds the lock must not leave it held in
 * the child (Pam forks workers from a zygote). */
static void pam_fiber_fork_release(void)
{
    pam_fiber_release();
}

__attribute__((constructor)) static void pam_fiber_register_fork_handlers(void)
{
    (void) pthread_atfork(pam_fiber_acquire, pam_fiber_fork_release, pam_fiber_fork_release);
}

/* Referenced from pam_php.c so the linker keeps this object. */
unsigned long pam_fiber_stack_pool_reuses(void)
{
    return atomic_load_explicit(&pam_fiber_reuses, memory_order_relaxed);
}
