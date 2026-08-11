<?php

declare(strict_types=1);

use Illuminate\Foundation\Application;
use Illuminate\Foundation\Configuration\Exceptions;
use Illuminate\Foundation\Configuration\Middleware;
use Illuminate\Contracts\Config\Repository as ConfigRepository;
use Illuminate\Support\Facades\Route;
use Illuminate\Support\Facades\DB;
use Laravel\Octane\OctaneServiceProvider;
use Pam\Octane\PamOctaneServiceProvider;

return Application::configure(basePath: dirname(__DIR__))
    ->withProviders([
        OctaneServiceProvider::class,
        PamOctaneServiceProvider::class,
    ])
    ->withRouting(
        using: static function (): void {
            Route::get('/api/ping', static fn (): array => ['message' => 'pong']);

            Route::get('/api/cached', static fn () => response()
                ->json(['message' => 'cached'])
                ->header('Cache-Control', 'public, max-age=30'));

            Route::get('/api/slow', static function (): array {
                usleep(50_000);

                return ['message' => 'slow'];
            });

            Route::get('/api/runtime', static function (): array {
                $opcache = function_exists('opcache_get_status') ? opcache_get_status(false) : false;

                return [
                    'php_version' => PHP_VERSION,
                    'zts' => defined('PHP_ZTS') ? constant('PHP_ZTS') === 1 : null,
                    'debug' => defined('PHP_DEBUG') ? constant('PHP_DEBUG') === 1 : null,
                    'opcache' => is_array($opcache),
                    'jit_enabled' => ($opcache['jit']['enabled'] ?? false) === true,
                    'jit_kind' => $opcache['jit']['kind'] ?? null,
                    'jit_buffer_size' => $opcache['jit']['buffer_size'] ?? null,
                    'sapi' => PHP_SAPI,
                ];
            });

            Route::get('/api/large-json', static fn (): array => [
                'items' => array_map(
                    static fn (int $id): array => [
                        'id' => $id,
                        'name' => "benchmark-item-{$id}",
                        'enabled' => $id % 2 === 0,
                    ],
                    range(1, 100),
                ),
            ]);

            Route::get('/api/blade', static fn () => view('benchmark', [
                'title' => 'PAM Octane',
                'items' => range(1, 40),
            ]));

            Route::get('/api/database', static function (): array {
                return [
                    'count' => DB::table('benchmark_items')->count(),
                    'items' => DB::table('benchmark_items')->orderBy('id')->limit(10)->get(),
                ];
            });

            Route::get('/state', static function (): array {
                $request = request();
                $request->attributes->set('private-marker', 'request-only');

                return [
                    'boot_id' => app('pam-octane.fixture-boot-id'),
                    'path' => $request->path(),
                ];
            });

            Route::get('/isolation', static fn (): array => [
                'leaked' => request()->attributes->has('private-marker'),
            ]);
        },
    )
    ->withMiddleware(static function (Middleware $middleware): void {
        // The fixture intentionally keeps the HTTP stack minimal.
    })
    ->withExceptions(static function (Exceptions $exceptions): void {
        // Use Laravel's default exception pipeline.
    })
    ->booting(static function (Application $app): void {
        $app->instance('pam-octane.fixture-boot-id', bin2hex(random_bytes(8)));
        $config = $app->make(ConfigRepository::class);
        $config->set('database.default', 'sqlite');
        $config->set('database.connections.sqlite', [
            'driver' => 'sqlite',
            'database' => storage_path('benchmark.sqlite'),
            'prefix' => '',
            'foreign_key_constraints' => true,
        ]);
    })
    ->create();
