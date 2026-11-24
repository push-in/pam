<?php

declare(strict_types=1);

namespace Nimbus\Telemetry;

use OpenTelemetry\API\Trace\SpanKind;
use OpenTelemetry\API\Trace\TracerInterface;

/**
 * A client span per database query, back-dated to when the query started.
 * The statement is recorded as the driver logged it, with placeholders: bound
 * values never reach the span. Queries outside a traced request or job are not
 * recorded, so boot-time and background noise does not create lone traces.
 *
 * @internal Installed by {@see Telemetry::install()}.
 */
final readonly class QueryTracer
{
    public function __construct(private TracerInterface $tracer, private int $maximumStatementBytes = 2048)
    {
    }

    public function record(string $sql, float $elapsedMilliseconds): void
    {
        $parent = \OpenTelemetry\API\Trace\Span::getCurrent()->getContext();
        if (!$parent->isValid()) {
            return;
        }
        $now = (int) (microtime(true) * 1e9);
        $operation = strtoupper((string) strtok(ltrim($sql), " \t\r\n("));
        $span = $this->tracer->spanBuilder($operation === '' ? 'db.query' : $operation)
            ->setSpanKind(SpanKind::KIND_CLIENT)
            ->setStartTimestamp($now - (int) ($elapsedMilliseconds * 1e6))
            ->setAttribute('db.query.text', strlen($sql) > $this->maximumStatementBytes
                ? substr($sql, 0, $this->maximumStatementBytes)
                : $sql)
            ->setAttribute('db.operation.name', $operation)
            ->startSpan();
        $span->end($now);
    }
}
