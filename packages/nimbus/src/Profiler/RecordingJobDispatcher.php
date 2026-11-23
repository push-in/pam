<?php

declare(strict_types=1);

namespace Nimbus\Profiler;

use Nimbus\Jobs\JobDispatcher;
use Nimbus\Jobs\JobEnvelope;

/**
 * Records dispatched jobs in the debug panel, then dispatches them.
 *
 * @internal Created through DebugPanel.
 */
final readonly class RecordingJobDispatcher implements JobDispatcher
{
    public function __construct(private JobDispatcher $inner, private DebugRecorder $recorder)
    {
    }

    public function dispatch(object $job, int $maximumAttempts = 3, int $delaySeconds = 0): JobEnvelope
    {
        $envelope = $this->inner->dispatch($job, $maximumAttempts, $delaySeconds);
        $this->recorder->job('dispatched', $job, [
            'id' => $envelope->id,
            'attempts' => $maximumAttempts,
            'delay_seconds' => $delaySeconds,
        ]);
        return $envelope;
    }
}
