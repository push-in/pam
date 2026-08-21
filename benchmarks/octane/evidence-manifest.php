<?php

declare(strict_types=1);

enum EvidenceSuite: int
{
    case Comparison = 1;
    case Matrix = 2;
    case Soak = 3;
    case Overload = 4;
}

$directory = isset($argv[1]) ? rtrim($argv[1], '/') : '';
$suiteValue = filter_var($argv[2] ?? null, FILTER_VALIDATE_INT);
$verify = ($argv[3] ?? '') === '--verify';

if ($directory === '' || !is_dir($directory) || $suiteValue === false) {
    fwrite(STDERR, "usage: php evidence-manifest.php <results-directory> <suite-id: 1|2|3|4> [--verify]\n");
    exit(64);
}

try {
    $suite = EvidenceSuite::from($suiteValue);
} catch (ValueError) {
    fwrite(STDERR, "evidence suite id must be 1 (comparison), 2 (matrix), 3 (soak), or 4 (overload)\n");
    exit(64);
}

$manifestPath = $directory.'/evidence-manifest.json';
$artifactFiles = static function (string $root): array {
    $allowedExtensions = ['csv', 'json', 'log', 'txt'];
    $files = [];
    $iterator = new RecursiveIteratorIterator(
        new RecursiveDirectoryIterator($root, FilesystemIterator::SKIP_DOTS),
    );
    foreach ($iterator as $file) {
        if (!$file instanceof SplFileInfo || !$file->isFile()) {
            continue;
        }
        $path = $file->getPathname();
        if (basename($path) === 'evidence-manifest.json') {
            continue;
        }
        if (!in_array(strtolower($file->getExtension()), $allowedExtensions, true)) {
            continue;
        }
        $relative = str_replace('\\', '/', substr($path, strlen($root) + 1));
        $files[$relative] = $path;
    }
    ksort($files, SORT_STRING);

    return $files;
};
$describeArtifacts = static function (array $files): array {
    $artifacts = [];
    foreach ($files as $relative => $path) {
        $hash = hash_file('sha256', $path);
        if (!is_string($hash)) {
            throw new RuntimeException("cannot hash evidence artifact {$relative}");
        }
        $bytes = filesize($path);
        if (!is_int($bytes)) {
            throw new RuntimeException("cannot measure evidence artifact {$relative}");
        }
        $artifacts[] = [
            'path' => $relative,
            'bytes' => $bytes,
            'sha256' => $hash,
        ];
    }

    return $artifacts;
};

if ($verify) {
    if (!is_file($manifestPath)) {
        fwrite(STDERR, "evidence manifest is missing: {$manifestPath}\n");
        exit(1);
    }
    $manifest = json_decode(
        (string) file_get_contents($manifestPath),
        true,
        flags: JSON_THROW_ON_ERROR,
    );
    if (($manifest['schema_version'] ?? null) !== 1
        || ($manifest['suite_id'] ?? null) !== $suite->value
        || !is_array($manifest['artifacts'] ?? null)) {
        fwrite(STDERR, "evidence manifest schema or suite does not match\n");
        exit(1);
    }
    $actual = $describeArtifacts($artifactFiles($directory));
    if ($actual !== $manifest['artifacts']) {
        fwrite(STDERR, "evidence artifacts do not match their manifest\n");
        exit(1);
    }
    fwrite(STDOUT, "Verified ".count($actual)." evidence artifacts.\n");
    exit(0);
}

$metadataPath = $directory.'/metadata.json';
$metadata = is_file($metadataPath)
    ? json_decode((string) file_get_contents($metadataPath), true, flags: JSON_THROW_ON_ERROR)
    : [];
$reportPath = match ($suite) {
    EvidenceSuite::Comparison => $directory.'/report.json',
    EvidenceSuite::Matrix => $directory.'/matrix-report.json',
    EvidenceSuite::Soak => $directory.'/soak-report.json',
    EvidenceSuite::Overload => $directory.'/overload-report.json',
};
$report = is_file($reportPath)
    ? json_decode((string) file_get_contents($reportPath), true, flags: JSON_THROW_ON_ERROR)
    : [];
$manifest = [
    'schema_version' => 1,
    'suite_id' => $suite->value,
    'generated_at' => gmdate(DATE_ATOM),
    'source' => $metadata['source'] ?? null,
    'parameters' => $metadata['parameters'] ?? null,
    'gates' => match ($suite) {
        EvidenceSuite::Comparison => [
            'measurement' => $report['measurement_gate']['passed'] ?? false,
            'dynamic' => ($report['dynamic_gate']['passed_frankenphp'] ?? false)
                && ($report['dynamic_gate']['p99_passed'] ?? false)
                && ($report['dynamic_gate']['zero_errors'] ?? false),
        ],
        EvidenceSuite::Matrix => $report['gates'] ?? null,
        EvidenceSuite::Soak => ['soak' => $report['passed'] ?? false],
        EvidenceSuite::Overload => ['overload' => $report['passed'] ?? false],
    },
    'artifacts' => $describeArtifacts($artifactFiles($directory)),
];
$encoded = json_encode(
    $manifest,
    JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR,
)."\n";
if (file_put_contents($manifestPath, $encoded) === false) {
    throw new RuntimeException("cannot write {$manifestPath}");
}
fwrite(STDOUT, $encoded);
