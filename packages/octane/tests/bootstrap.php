<?php

declare(strict_types=1);

require __DIR__.'/../vendor/autoload.php';
if (!class_exists(\Pam\Http\Request::class, false)) {
    require_once __DIR__.'/../../../runtime/bootstrap.php';
    require_once __DIR__.'/../../../runtime/async.php';
    require_once __DIR__.'/../../../runtime/native.php';
    require_once __DIR__.'/../../../runtime/laravel.php';
}

$_SERVER['PHP_SELF'] ??= 'artisan';
