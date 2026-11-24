<?php

declare(strict_types=1);

namespace Nimbus\Profiler;

use Nimbus\Jobs\JobHandler;
use Nimbus\Jobs\JobOutcome;

/**
 * Records each job a worker runs (outcome, duration, failure) in the debug panel.
 *
 * @internal Created through DebugPanel.
 */
final readonly class RecordingJobHandler implements JobHandler
{
    public function __construct(private JobHandler $inner, private DebugRecorder $recorder)
    {
    }

    public function handle(object $job): JobOutcome
    {
        $started = hrtime(true);
        try {
            $outcome = $this->inner->handle($job);
        } catch (\Throwable $failure) {
            $this->recorder->exception($failure);
            $this->recorder->job('failed', $job, ['duration_ms' => self::since($started), 'exception' => $failure::class]);
            throw $failure;
        }
        $this->recorder->job('processed', $job, ['duration_ms' => self::since($started), 'outcome' => $outcome->name]);
        return $outcome;
    }

    private static function since(int $started): float
    {
        return round((hrtime(true) - $started) / 1_000_000, 3);
    }
}
