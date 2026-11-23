<?php

declare(strict_types=1);

// Protocol errors on the dispatch entry points must answer 500 instead of
// throwing: an exception that escapes to the C boundary has no PHP frame to
// land in, becomes a fatal error and recycles the worker (audit A1).
$decode = new ReflectionMethod(\Pam\Internal\Runtime::class, 'decodeHttpResponse');
$status = static fn (string $encoded): int => $decode->invoke(null, $encoded)['status'];

$results = [
    'empty-id' => $status(\Pam\Internal\Runtime::beginHttpDispatch('GET', '/', [], '', [], '', [])),
    'unknown-resume' => $status(\Pam\Internal\Runtime::resumeHttpDispatch('missing')),
];
\Pam\Internal\Runtime::cancelHttpDispatch('missing');
echo json_encode($results), "\n";
