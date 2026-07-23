<?php

declare(strict_types=1);

namespace Pam\Observability {
    enum SpanStatus: int
    {
        case Unset = 1;
        case Ok = 2;
        case Error = 3;
    }

    final class Telemetry
    {
        private static ?bool $openTelemetryAvailable = null;

        private const REQUEST_ID = 'pam.telemetry.request_id';
        private const REQUEST_SCOPE = 'pam.telemetry.request_scope';
        private const TRACEPARENT = 'pam.telemetry.traceparent';

        public static function beginRequest(string $requestId, string $traceparent): void
        {
            \Pam\Async\FiberContext::set(self::REQUEST_ID, $requestId);
            \Pam\Async\FiberContext::set(self::TRACEPARENT, $traceparent);
            self::$openTelemetryAvailable ??= class_exists(
                \OpenTelemetry\API\Trace\Propagation\TraceContextPropagator::class,
            );
            if (self::$openTelemetryAvailable) {
                $scope = \OpenTelemetry\API\Trace\Propagation\TraceContextPropagator::getInstance()
                    ->extract(
                        ['traceparent' => $traceparent],
                        context: \OpenTelemetry\Context\Context::getRoot(),
                    )
                    ->activate();
                \Pam\Async\FiberContext::set(self::REQUEST_SCOPE, $scope);
            }
        }

        public static function endRequest(): void
        {
            $scope = \Pam\Async\FiberContext::get(self::REQUEST_SCOPE);
            if (is_object($scope) && method_exists($scope, 'detach')) {
                $scope->detach();
            }
            \Pam\Async\FiberContext::remove(self::REQUEST_SCOPE);
            \Pam\Async\FiberContext::remove(self::REQUEST_ID);
            \Pam\Async\FiberContext::remove(self::TRACEPARENT);
        }

        /** @param array<string, mixed> $context */
        public static function log(string $level, string $message, array $context = []): void
        {
            $replace = [];
            foreach ($context as $key => $value) {
                if (is_scalar($value) || $value === null || $value instanceof \Stringable) {
                    $replace['{' . $key . '}'] = (string) $value;
                }
            }
            error_log(json_encode([
                'timestamp' => (int) floor(microtime(true) * 1000),
                'level' => strtolower($level),
                'message' => strtr($message, $replace),
                'context' => $context,
                'requestId' => \Pam\Async\FiberContext::get(self::REQUEST_ID),
                'traceparent' => \Pam\Async\FiberContext::get(self::TRACEPARENT),
                'worker' => getenv('PAM_WORKER_ID') ?: 'standalone',
            ], JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE));
        }

        /** @param array<string, scalar|null> $attributes */
        public static function span(string $name, callable $operation, array $attributes = []): mixed
        {
            if ($name === '') {
                throw new \InvalidArgumentException('Span name cannot be empty.');
            }
            $started = microtime(true);
            $externalSpan = null;
            $scope = null;
            try {
                if (class_exists(\OpenTelemetry\API\Globals::class)) {
                    $tracer = \OpenTelemetry\API\Globals::tracerProvider()->getTracer('pam');
                    $externalSpan = $tracer->spanBuilder($name)->startSpan();
                    foreach ($attributes as $key => $value) {
                        if ($key === '') {
                            throw new \InvalidArgumentException('Span attribute names cannot be empty.');
                        }
                        $externalSpan->setAttribute($key, $value);
                    }
                    $scope = $externalSpan->activate();
                }
                $result = $operation();
                $externalSpan?->setStatus(\OpenTelemetry\API\Trace\StatusCode::STATUS_OK);
                return $result;
            } catch (\Throwable $error) {
                $externalSpan?->recordException($error);
                $externalSpan?->setStatus(
                    \OpenTelemetry\API\Trace\StatusCode::STATUS_ERROR,
                    $error->getMessage(),
                );
                throw $error;
            } finally {
                $scope?->detach();
                $externalSpan?->end();
                self::log('debug', 'span {name}', [
                    'name' => $name,
                    'durationMs' => (microtime(true) - $started) * 1000,
                    'attributes' => $attributes,
                ]);
            }
        }
    }

    if (class_exists(\Psr\Log\AbstractLogger::class)) {
        final class Logger extends \Psr\Log\AbstractLogger
        {
            public function log($level, string|\Stringable $message, array $context = []): void
            {
                $level = is_string($level) || is_numeric($level) ? (string) $level : 'info';
                Telemetry::log($level, (string) $message, $context);
            }
        }
    }
}
