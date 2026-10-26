<?php

declare(strict_types=1);

$target = $argv[1] ?? null;
if (!is_string($target) || $target === '') {
    fwrite(STDERR, "pam: Composer bootstrap target is missing.\n");
    exit(64);
}

$signaturePath = $argv[2] ?? null;
$installerPath = $argv[3] ?? null;
if (!is_string($signaturePath) || !is_string($installerPath)) {
    fwrite(STDERR, "pam: Composer bootstrap inputs are missing.\n");
    exit(64);
}

$signature = @file_get_contents($signaturePath);
$installer = @file_get_contents($installerPath);
if (!is_string($signature) || !is_string($installer)) {
    fwrite(STDERR, "pam: unable to read the downloaded Composer installer.\n");
    exit(69);
}

$signature = strtolower(trim($signature));
if (preg_match('/^[a-f0-9]{96}$/D', $signature) !== 1) {
    fwrite(STDERR, "pam: Composer returned an invalid installer signature.\n");
    exit(70);
}

$actual = hash('sha384', $installer);
if (!hash_equals($signature, $actual)) {
    fwrite(STDERR, "pam: Composer installer signature verification failed.\n");
    exit(70);
}

$setup = $installerPath;

try {
    $argv = [
        $setup,
        '--install-dir=' . dirname($target),
        '--filename=' . basename($target),
        '--quiet',
        '--2',
    ];
    $_SERVER['argv'] = $argv;
    $_SERVER['argc'] = count($argv);
    require $setup;
} finally {
    @unlink($setup);
}
