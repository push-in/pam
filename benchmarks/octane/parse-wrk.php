<?php

declare(strict_types=1);

if ($argc !== 4) {
    fwrite(STDERR, "Usage: parse-wrk.php <runtime> <round> <output>\n");
    exit(64);
}

[$script, $runtime, $round, $path] = $argv;
$contents = file_get_contents($path);
if (!is_string($contents)) {
    throw new RuntimeException("Cannot read wrk output: {$path}");
}

$number = static function (string $pattern) use ($contents): float {
    if (preg_match($pattern, $contents, $matches) !== 1) {
        throw new RuntimeException("Missing benchmark field matching {$pattern}");
    }

    return (float) $matches[1];
};

$latencyMicros = static function (string $label) use ($contents): int {
    if (preg_match('/^\s*'.preg_quote($label, '/').'\s+([0-9.]+)(us|ms|s)$/mi', $contents, $matches) !== 1) {
        throw new RuntimeException("Missing {$label} latency");
    }

    $multiplier = match ($matches[2]) {
        'us' => 1,
        'ms' => 1_000,
        's' => 1_000_000,
    };

    return (int) round((float) $matches[1] * $multiplier);
};

$errors = 0;
if (preg_match('/Socket errors: connect (\d+), read (\d+), write (\d+), timeout (\d+)/', $contents, $matches) === 1) {
    $errors = array_sum(array_map('intval', array_slice($matches, 1)));
}
if (preg_match('/Non-2xx or 3xx responses: (\d+)/', $contents, $matches) === 1) {
    $errors += (int) $matches[1];
}

$result = [
    'schema_version' => 1,
    'runtime' => $runtime,
    'round' => (int) $round,
    'rps' => $number('/Requests\/sec:\s+([0-9.]+)/'),
    'latency' => [
        'p50_us' => $latencyMicros('50%'),
        'p75_us' => $latencyMicros('75%'),
        'p90_us' => $latencyMicros('90%'),
        'p99_us' => $latencyMicros('99%'),
    ],
    'errors' => $errors,
];

file_put_contents(
    preg_replace('/\.txt$/', '.json', $path) ?: $path.'.json',
    json_encode($result, JSON_PRETTY_PRINT | JSON_THROW_ON_ERROR)."\n",
);
