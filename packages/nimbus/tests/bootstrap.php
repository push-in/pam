<?php

declare(strict_types=1);

// Inside the PAM runtime (pam vendor/bin/phpunit) the real classes load on demand.
if (!function_exists('pam_native_abi_version') && !class_exists(\Pam\Http\Request::class)) {
    require_once __DIR__ . '/runtime.php';
}
