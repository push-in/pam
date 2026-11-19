<?php

declare(strict_types=1);

$autoloaders = [
    dirname(__DIR__) . '/vendor/autoload.php',
    dirname(__DIR__, 2) . '/http/vendor/autoload.php',
    dirname(__DIR__, 2) . '/api/vendor/autoload.php',
];

foreach ($autoloaders as $autoloader) {
    if (is_file($autoloader)) {
        require $autoloader;
        break;
    }
}

spl_autoload_register(static function (string $class): void {
    $directories = [
        'App\\' => '/app/',
        'Database\\Factories\\' => '/database/factories/',
        'Database\\Seeders\\' => '/database/seeders/',
    ];
    foreach ($directories as $prefix => $directory) {
        if (!str_starts_with($class, $prefix)) {
            continue;
        }
        $path = dirname(__DIR__) . $directory . str_replace('\\', '/', substr($class, strlen($prefix))) . '.php';
        if (is_file($path)) {
            require $path;
        }
        return;
    }
});
