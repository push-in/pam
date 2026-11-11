<?php

declare(strict_types=1);

require __DIR__.'/../vendor/autoload.php';
if (!class_exists(\Pam\Http\Request::class, false)) {
    // Outside PAM: the runtime's PHP sources from the monorepo
    // (packages/octane) or from tests/tools/fetch-pam-runtime.sh.
    $runtime = getenv('PAM_RUNTIME_DIR');
    if (!is_string($runtime) || $runtime === '') {
        $runtime = is_file(__DIR__.'/../../../runtime/bootstrap.php')
            ? __DIR__.'/../../../runtime'
            : __DIR__.'/../.pam-runtime';
    }
    foreach (['bootstrap', 'async', 'native', 'shared', 'laravel'] as $file) {
        require_once "{$runtime}/{$file}.php";
    }
}

$_SERVER['PHP_SELF'] ??= 'artisan';
