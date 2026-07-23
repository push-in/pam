<?php

declare(strict_types=1);

namespace Composer\Autoload {
    final class ClassLoader
    {
    }
}

namespace {
    spl_autoload_register(static function (string $class): void {
        $prefix = 'Fixture\\';
        if (!str_starts_with($class, $prefix)) {
            return;
        }

        $relative = substr($class, strlen($prefix));
        require dirname(__DIR__) . '/src/' . str_replace('\\', '/', $relative) . '.php';
    });

    return new \Composer\Autoload\ClassLoader();
}
