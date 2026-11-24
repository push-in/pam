<?php

declare(strict_types=1);

namespace Nimbus\Telemetry;

use Nimbus\Jobs\JobHandler;
use Nimbus\Jobs\JobOutcome;
use Nimbus\Metrics\Counter;
use Nimbus\Metrics\Histogram;
use OpenTelemetry\API\Trace\SpanKind;
use OpenTelemetry\API\Trace\StatusCode;
use OpenTelemetry\API\Trace\TracerInterface;

/**
 * Wraps a job handler with a consumer span (`job App\Jobs\SendInvoice`) and
 * the `nimbus_jobs_total` / `nimbus_job_duration_seconds` metrics, labelled by
 * job class and outcome (`complete`, `retry`, `dead_letter`, `failed`).
 *
 * @internal Built by {@see Telemetry::job()}.
 */
final readonly class ObservedJobHandler implements JobHandler
{
    /** @var \Closure(object): JobOutcome */
    private \Closure $handler;

    /** @param JobHandler|callable(object): JobOutcome $handler */
    public function __construct(
        JobHandler|callable $handler,
        private ?TracerInterface $tracer = null,
        private ?Counter $jobs = null,
        private ?Histogram $duration = null,
    ) {
        $this->handler = $handler instanceof JobHandler ? $handler->handle(...) : \Closure::fromCallable($handler);
    }

    public function handle(object $job): JobOutcome
    {
        $class = $job::class;
        // A job is its own trace: it runs long after, and apart from, the request that queued it.
        $span = $this->tracer?->spanBuilder('job ' . $class)
            ->setParent(false)
            ->setSpanKind(SpanKind::KIND_CONSUMER)
            ->setAttribute('messaging.operation.type', 'process')
            ->setAttribute('nimbus.job.class', $class)
            ->startSpan();
        $scope = $span?->activate();
        $startedAt = hrtime(true);
        $outcome = 'failed';
        try {
            $result = ($this->handler)($job);
            $outcome = match ($result) {
                JobOutcome::Complete => 'complete',
                JobOutcome::Retry => 'retry',
                JobOutcome::DeadLetter => 'dead_letter',
            };
            return $result;
        } catch (\Throwable $error) {
            $span?->recordException($error);
            $span?->setStatus(StatusCode::STATUS_ERROR);
            throw $error;
        } finally {
            $span?->setAttribute('nimbus.job.outcome', $outcome);
            $scope?->detach();
            $span?->end();
            $this->jobs?->inc(['job' => $class, 'outcome' => $outcome]);
            $this->duration?->observe((hrtime(true) - $startedAt) / 1e9, ['job' => $class]);
        }
    }
}
