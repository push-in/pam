<?php

declare(strict_types=1);

$directory = sys_get_temp_dir().'/pam-benchmark-'.bin2hex(random_bytes(8));
if (!mkdir($directory, 0700)) {
    throw new RuntimeException('Unable to create benchmark test directory.');
}

$round = static fn (float $rps, int $p95, int $errors = 0): array => [
    'rps' => $rps,
    'latency' => [
        'p50_us' => intdiv($p95, 2),
        'p95_us' => $p95,
        'p99_us' => $p95 * 2,
    ],
    'errors' => $errors,
];

try {
    file_put_contents(
        $directory.'/pam.round-1.json',
        json_encode($round(750, 1_500), JSON_THROW_ON_ERROR),
    );
    file_put_contents(
        $directory.'/node-http.round-1.json',
        json_encode($round(1_000, 1_000), JSON_THROW_ON_ERROR),
    );
    $command = escapeshellarg(PHP_BINARY).' '
        .escapeshellarg(__DIR__.'/aggregate.php').' '
        .escapeshellarg($directory);
    exec($command, $output, $status);
    if ($status !== 0) {
        throw new RuntimeException('Aggregate command unexpectedly failed.');
    }
    $report = json_decode(
        (string) file_get_contents($directory.'/report.json'),
        true,
        flags: JSON_THROW_ON_ERROR,
    );
    if (($report['gate']['passed'] ?? null) !== true
        || ($report['comparison']['pam_to_node_rps_ratio'] ?? null) !== 0.75
        || ($report['comparison']['pam_to_node_p95_ratio'] ?? null) !== 1.5) {
        throw new RuntimeException('Aggregate comparison contract changed.');
    }
} finally {
    foreach (glob($directory.'/*') ?: [] as $path) {
        unlink($path);
    }
    rmdir($directory);
}

fwrite(STDOUT, "Benchmark aggregate contract passed.\n");
