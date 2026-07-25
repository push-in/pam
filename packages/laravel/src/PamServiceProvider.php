<?php

declare(strict_types=1);

namespace Pam\Laravel;

use Illuminate\Cache\Events\CacheHit;
use Illuminate\Cache\Events\CacheMissed;
use Illuminate\Cache\Events\KeyForgotten;
use Illuminate\Cache\Events\KeyWritten;
use Illuminate\Console\Events\CommandFinished;
use Illuminate\Console\Events\CommandStarting;
use Illuminate\Contracts\Debug\ExceptionHandler;
use Illuminate\Foundation\Application;
use Illuminate\Database\Events\QueryExecuted;
use Illuminate\Http\Client\Events\ConnectionFailed;
use Illuminate\Http\Client\Events\ResponseReceived;
use Illuminate\Queue\Events\JobFailed;
use Illuminate\Queue\Events\JobProcessed;
use Illuminate\Queue\Events\JobProcessing;
use Illuminate\Routing\Router;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Event;
use Illuminate\Support\Facades\Route;
use Illuminate\Support\ServiceProvider;
use Pam\Laravel\Console\AutoscaleCommand;
use Pam\Laravel\Console\CapacityCommand;
use Pam\Laravel\Console\CheckProductionCommand;
use Pam\Laravel\Console\CompatibilityCommand;
use Pam\Laravel\Console\DeployCommand;
use Pam\Laravel\Console\ForgeScriptCommand;
use Pam\Laravel\Console\HealthCommand;
use Pam\Laravel\Console\InstallCommand;
use Pam\Laravel\Console\LeaksCommand;
use Pam\Laravel\Console\McpCommand;
use Pam\Laravel\Console\MigrateFromOctaneCommand;
use Pam\Laravel\Console\NightwatchCommand;
use Pam\Laravel\Console\RemoteCommand;
use Pam\Laravel\Console\SupervisorCommand;
use Pam\Laravel\Contracts\TelemetryExporter;
use Pam\Laravel\Enums\SpanKind;
use Pam\Laravel\Enums\SpanStatus;
use Pam\Laravel\Http\Middleware\GuardPersistentState;
use Pam\Laravel\Http\Middleware\ObserveRequest;
use Pam\Laravel\Enums\JobEventType;
use Pam\Laravel\Services\AtomicDeployer;
use Pam\Laravel\Services\Autoscaler;
use Pam\Laravel\Services\AutoscaleMetricsClient;
use Pam\Laravel\Services\HealthReporter;
use Pam\Laravel\Services\LifecycleManager;
use Pam\Laravel\Services\McpServer;
use Pam\Laravel\Services\NightwatchIntegration;
use Pam\Laravel\Services\ObservabilityRegistry;
use Pam\Laravel\Services\OtlpHttpExporter;
use Pam\Laravel\Services\ProcessSupervisor;
use Pam\Laravel\Services\ProductionChecker;
use Pam\Laravel\Services\RemoteControlClient;
use Pam\Laravel\Services\StateGuard;
use Pam\Laravel\Services\TelemetryManager;
use Pam\Laravel\Support\ConfigValue;
use Throwable;

final class PamServiceProvider extends ServiceProvider
{
    public function register(): void
    {
        $this->mergeConfigFrom(__DIR__.'/../config/pam.php', 'pam');
        foreach ([
            LifecycleManager::class,
            ObservabilityRegistry::class,
            StateGuard::class,
            TelemetryManager::class,
        ] as $service) {
            $this->app->singleton($service);
        }
        $this->app->singleton(TelemetryExporter::class, OtlpHttpExporter::class);
        $this->app->singleton(ProductionChecker::class);
        $this->app->singleton(HealthReporter::class);
        $this->app->singleton(ProcessSupervisor::class);
        $this->app->singleton(AtomicDeployer::class);
        $this->app->singleton(RemoteControlClient::class);
        $this->app->singleton(NightwatchIntegration::class);
        $this->app->singleton(Autoscaler::class);
        $this->app->singleton(AutoscaleMetricsClient::class);
        $this->app->singleton(McpServer::class);
    }

    public function boot(Router $router): void
    {
        if ($this->app->runningInConsole()) {
            $this->commands([
                InstallCommand::class,
                CheckProductionCommand::class,
                CompatibilityCommand::class,
                HealthCommand::class,
                LeaksCommand::class,
                MigrateFromOctaneCommand::class,
                CapacityCommand::class,
                SupervisorCommand::class,
                DeployCommand::class,
                RemoteCommand::class,
                NightwatchCommand::class,
                AutoscaleCommand::class,
                McpCommand::class,
                ForgeScriptCommand::class,
            ]);
            $this->publishes([__DIR__.'/../config/pam.php' => config_path('pam.php')], 'pam-config');
            $this->publishes([
                __DIR__.'/../stubs/pam.processes.json' => base_path('pam.processes.json'),
                __DIR__.'/../stubs/docker-compose.pam.yml' => base_path('docker-compose.pam.yml'),
                __DIR__.'/../stubs/pam.service' => base_path('deploy/pam.service'),
                __DIR__.'/../stubs/kubernetes.yaml' => base_path('deploy/kubernetes.yaml'),
                __DIR__.'/../stubs/forge-deploy.sh' => base_path('deploy/forge-deploy.sh'),
            ], 'pam-operations');
        }

        if (ConfigValue::bool('pam.state_guard.enabled', true)) {
            $router->pushMiddlewareToGroup('web', GuardPersistentState::class);
            $router->pushMiddlewareToGroup('api', GuardPersistentState::class);
        }
        $observabilityEnabled = ConfigValue::bool('pam.observability.enabled', true);
        $telemetryEnabled = ConfigValue::bool('pam.telemetry.enabled');
        if ($observabilityEnabled || $telemetryEnabled) {
            $router->pushMiddlewareToGroup('web', ObserveRequest::class);
            $router->pushMiddlewareToGroup('api', ObserveRequest::class);
            DB::listen(function (QueryExecuted $query) use ($observabilityEnabled, $telemetryEnabled): void {
                if ($observabilityEnabled) {
                    $this->app->make(ObservabilityRegistry::class)->query($query->sql, $query->time);
                }
                if ($telemetryEnabled) {
                    $this->app->make(TelemetryManager::class)->child(
                        'db.query',
                        SpanKind::Client,
                        (int) round($query->time * 1_000_000),
                        [
                            'db.system.name' => $query->connectionName,
                            'db.query.summary' => $this->querySummary($query->sql),
                        ],
                    );
                }
            });
        }
        if ($telemetryEnabled) {
            $this->registerTelemetryEvents();
        }
        $this->registerHealthRoutes();
    }

    private function registerTelemetryEvents(): void
    {
        Event::listen(JobProcessing::class, function (JobProcessing $event): void {
            $name = $event->job->resolveName();
            $this->app->make(ObservabilityRegistry::class)->job(JobEventType::Processing, $name);
            $this->app->make(TelemetryManager::class)->startRoot('job '.$name, SpanKind::Consumer, [
                'messaging.system' => (string) $event->connectionName,
                'messaging.operation.name' => 'process',
                'messaging.destination.name' => $event->job->getQueue(),
            ]);
        });
        Event::listen(JobProcessed::class, function (JobProcessed $event): void {
            $this->app->make(ObservabilityRegistry::class)->job(JobEventType::Processed, $event->job->resolveName());
            $telemetry = $this->app->make(TelemetryManager::class);
            $telemetry->finishRoot(SpanStatus::Ok);
            $telemetry->flush();
        });
        Event::listen(JobFailed::class, function (JobFailed $event): void {
            $this->app->make(ObservabilityRegistry::class)->job(JobEventType::Failed, $event->job->resolveName());
            $telemetry = $this->app->make(TelemetryManager::class);
            $telemetry->finishRoot(SpanStatus::Error, ['exception.type' => $event->exception::class], $event->exception->getMessage());
            $telemetry->flush();
        });
        foreach ([
            CacheHit::class => 'hit',
            CacheMissed::class => 'miss',
            KeyWritten::class => 'write',
            KeyForgotten::class => 'forget',
        ] as $eventClass => $operation) {
            Event::listen($eventClass, function (object $event) use ($operation): void {
                /** @var CacheHit|CacheMissed|KeyWritten|KeyForgotten $event */
                $this->app->make(TelemetryManager::class)->child('cache '.$operation, SpanKind::Internal, 0, [
                    'cache.operation.name' => $operation,
                    'cache.store.name' => (string) $event->storeName,
                    'cache.key_hash' => hash('sha256', (string) $event->key),
                ]);
            });
        }
        Event::listen(ResponseReceived::class, function (ResponseReceived $event): void {
            $stats = $event->response->handlerStats();
            $duration = (int) round((float) ($stats['total_time'] ?? 0) * 1_000_000_000);
            $this->app->make(TelemetryManager::class)->child('http.client', SpanKind::Client, $duration, [
                'http.request.method' => $event->request->method(),
                'server.address' => (string) parse_url($event->request->url(), PHP_URL_HOST),
                'http.response.status_code' => $event->response->status(),
            ], $event->response->serverError() ? SpanStatus::Error : SpanStatus::Ok);
        });
        Event::listen(ConnectionFailed::class, function (ConnectionFailed $event): void {
            $this->app->make(TelemetryManager::class)->child('http.client', SpanKind::Client, 0, [
                'http.request.method' => $event->request->method(),
                'server.address' => (string) parse_url($event->request->url(), PHP_URL_HOST),
                'error.type' => $event->exception::class,
            ], SpanStatus::Error);
        });
        Event::listen(CommandStarting::class, function (CommandStarting $event): void {
            if ($event->command !== 'pam:mcp') {
                $this->app->make(TelemetryManager::class)->startRoot('command '.$event->command, SpanKind::Internal, [
                    'command.name' => $event->command,
                ]);
            }
        });
        Event::listen(CommandFinished::class, function (CommandFinished $event): void {
            if ($event->command !== 'pam:mcp') {
                $telemetry = $this->app->make(TelemetryManager::class);
                $telemetry->finishRoot($event->exitCode === 0 ? SpanStatus::Ok : SpanStatus::Error, [
                    'command.exit_code' => $event->exitCode,
                ]);
                $telemetry->flush();
            }
        });
        try {
            $handler = $this->app->make(ExceptionHandler::class);
            if (method_exists($handler, 'reportable')) {
                $handler->reportable(function (Throwable $exception): void {
                    $this->app->make(TelemetryManager::class)->child('exception', SpanKind::Internal, 0, [
                        'exception.type' => $exception::class,
                    ], SpanStatus::Error);
                });
            }
        } catch (Throwable) {
            // Custom exception handlers may not expose Laravel's reportable callbacks.
        }
    }

    private function querySummary(string $sql): string
    {
        $normalized = preg_replace(["/'[^']*'/", '/\b\d+\b/'], ['?', '?'], $sql) ?: $sql;
        $summary = preg_replace('/\s+/', ' ', trim($normalized)) ?: 'query';

        return mb_substr($summary, 0, 160);
    }

    private function registerHealthRoutes(): void
    {
        if (!ConfigValue::bool('pam.health.enabled', true)
            || ($this->app instanceof Application && $this->app->routesAreCached())) {
            return;
        }
        Route::get(ConfigValue::string('pam.health.path', '/__pam/health'), function (HealthReporter $reporter) {
            $report = $reporter->report();
            return response()->json($report, $report['ok'] ? 200 : 503);
        })->name('pam.health');
        Route::get(ConfigValue::string('pam.health.metrics_path', '/__pam/metrics'), function (ObservabilityRegistry $registry) {
            $token = ConfigValue::string('pam.health.token');
            if ($token !== '' && !hash_equals($token, (string) request()->bearerToken())) {
                abort(403);
            }
            return response()->json($registry->snapshot());
        })->name('pam.metrics');
    }
}
