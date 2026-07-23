<?php

declare(strict_types=1);

$target = $argv[1] ?? null;
if (!is_string($target) || $target === '') {
    fwrite(STDERR, "pam: Composer bootstrap target is missing.\n");
    exit(64);
}

$signature = @file_get_contents('https://composer.github.io/installer.sig');
$installer = @file_get_contents('https://getcomposer.org/installer');
if (!is_string($signature) || !is_string($installer)) {
    fwrite(STDERR, "pam: unable to download the verified Composer installer.\n");
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

$setup = dirname($target) . '/composer-setup.php';
if (file_put_contents($setup, $installer, LOCK_EX) === false) {
    fwrite(STDERR, "pam: unable to write the Composer installer cache.\n");
    exit(73);
}

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
