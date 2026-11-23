<?php

declare(strict_types=1);

// Runs a Fiber to completion so its C stack goes back to the pool, then
// checks the next Fiber (which reuses that stack): its stack must still have
// the guard page below it, and overflowing it must end in PHP's stack limit
// error, never run into neighbouring memory (audit M16).
(new Fiber(static fn (): int => 1))->start();

function pam_recurse(int $depth): int
{
    // array_map re-enters the VM from C, so every level grows the C stack.
    return array_map(static fn (int $value): int => pam_recurse($depth + 1), [$depth])[0];
}

/** Whether the stack holding $address starts right above a PROT_NONE page. */
function pam_guarded(): bool
{
    $previous = null;
    foreach (file('/proc/self/maps') ?: [] as $line) {
        if (!preg_match('/^([0-9a-f]+)-([0-9a-f]+) (\S{4}) .*\[anon:zend_fiber_stack\]/', $line, $match)) {
            $previous = null;
            continue;
        }
        if ($match[3] === 'rw-p' && $previous !== null && $previous[3] === '---p' && $previous[2] === $match[1]) {
            return true;
        }
        $previous = $match;
    }
    return false;
}

$fiber = new Fiber(static function (): void {
    echo 'reuses=', pam_native_fiber_stack_reuses(), ' guarded=', pam_guarded() ? 1 : 0, "\n";
    try {
        pam_recurse(0);
    } catch (Error $error) {
        echo str_contains($error->getMessage(), 'Maximum call stack size') ? "stopped\n" : $error->getMessage();
    }
});
$fiber->start();
