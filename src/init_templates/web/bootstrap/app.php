<?php

declare(strict_types=1);

use App\Providers\AppServiceProvider;
use Nimbus\App;
use Nimbus\Database\ArcanoSchemaCompiler;
use Nimbus\Database\ArcanoServiceProvider;
use Nimbus\Database\DatabaseConfig;
use Nimbus\Http\CookieCodec;
use Nimbus\Middleware\SecurityHeadersMiddleware;
use Nimbus\Session\FileSessionStore;
use Nimbus\Session\MemorySessionStore;
use Nimbus\Session\SessionConfig;
use Nimbus\Session\SessionServiceProvider;
use Nimbus\Translation\TranslationServiceProvider;
use Nimbus\View\ViewServiceProvider;

$root = dirname(__DIR__);

// Values already in the environment (the process manager, phpunit.xml, CI) win over .env.
if (is_file($root . '/.env')) {
    foreach (file($root . '/.env', FILE_IGNORE_NEW_LINES | FILE_SKIP_EMPTY_LINES) ?: [] as $line) {
        $line = trim($line);
        if ($line === '' || str_starts_with($line, '#') || !str_contains($line, '=')) {
            continue;
        }
        [$name, $value] = array_map(trim(...), explode('=', $line, 2));
        if (getenv($name) === false) {
            putenv($name . '=' . trim($value, "\"'"));
        }
    }
}

$production = getenv('PAM_ENV') === 'production';
$sessions = getenv('SESSION_DRIVER') === 'memory'
    ? new MemorySessionStore()
    : new FileSessionStore($root . '/storage/sessions');

$app = new App(discoverPackages: false);
$app->provider(new ArcanoServiceProvider(
    DatabaseConfig::fromEnvironment(),
    new ArcanoSchemaCompiler($root . '/app/Models', $root . '/storage/cache/arcano-schema.php', refresh: !$production),
    migrations: $root . '/database/migrations',
));
$app->provider(new SessionServiceProvider(
    $sessions,
    new SessionConfig(secure: getenv('SESSION_SECURE_COOKIE') !== 'false'),
    cookies: CookieCodec::fromEnvironment(),
));
$app->provider(new TranslationServiceProvider(
    $root . '/lang',
    getenv('APP_LOCALE') ?: 'pt_BR',
    fallbackLocales: ['en'],
    supportedLocales: ['pt_BR', 'en'],
));
$app->provider(new ViewServiceProvider($root . '/resources/views', reload: !$production));
$app->provider(new AppServiceProvider());
$app->middleware(new SecurityHeadersMiddleware());
$app->routes($root . '/routes.php');

return $app;
