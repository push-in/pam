<?php

declare(strict_types=1);

namespace Pam\Contracts\Transport;

final class TransportWorker
{
    /**
     * @param callable(TransportMessage): MessageResult $handler
     */
    public static function run(
        TransportProviderInterface $provider,
        callable $handler,
        TransportContext $context,
        int $maximumMessages = 0,
        int $waitMilliseconds = 1_000,
    ): int {
        if ($maximumMessages < 0) {
            throw new \InvalidArgumentException('Maximum transport messages cannot be negative.');
        }
        if ($waitMilliseconds < 1 || $waitMilliseconds > 60_000) {
            throw new \InvalidArgumentException('Transport wait must be between 1 and 60,000 milliseconds.');
        }
        $descriptor = $provider->descriptor();
        if (!$descriptor->supports(TransportCapability::Consume)) {
            throw new \LogicException("Transport {$descriptor->id} does not support consumption.");
        }

        $processed = 0;
        $context->record(TransportEventCode::Starting, ['transportId' => $descriptor->id]);
        try {
            $provider->start($context);
            $context->record(TransportEventCode::Ready, ['transportId' => $descriptor->id]);
            while (!$context->isCancelled()
                && ($maximumMessages === 0 || $processed < $maximumMessages)
            ) {
                $remaining = $maximumMessages === 0
                    ? $descriptor->maxBatchSize
                    : min($descriptor->maxBatchSize, $maximumMessages - $processed);
                $received = 0;
                foreach ($provider->receive($remaining, $waitMilliseconds) as $message) {
                    if (++$received > $remaining) {
                        throw new \OverflowException('Transport provider exceeded the requested batch size.');
                    }
                    if (strlen($message->payload) > $descriptor->maxPayloadBytes) {
                        throw new \LengthException('Transport message exceeds the declared payload limit.');
                    }
                    $context->record(TransportEventCode::MessageReceived, [
                        'bytes' => strlen($message->payload),
                        'attempt' => $message->attempt,
                    ]);
                    try {
                        $result = $handler($message);
                    } catch (\Throwable $error) {
                        $context->record(TransportEventCode::Failed, [
                            'exception' => $error::class,
                        ]);
                        $result = $descriptor->supports(TransportCapability::DelayedRetry)
                            ? MessageResult::retry()
                            : MessageResult::reject();
                    }
                    if ($result->disposition === MessageDisposition::Retry
                        && !$descriptor->supports(TransportCapability::DelayedRetry)
                    ) {
                        throw new \LogicException("Transport {$descriptor->id} does not support delayed retries.");
                    }
                    $provider->acknowledge(
                        $message,
                        $result->disposition,
                        $result->retryAfterMilliseconds,
                    );
                    $event = match ($result->disposition) {
                        MessageDisposition::Acknowledge => TransportEventCode::MessageAcknowledged,
                        MessageDisposition::Retry => TransportEventCode::RetryScheduled,
                        MessageDisposition::Reject => TransportEventCode::MessageRejected,
                    };
                    $context->record($event, ['attempt' => $message->attempt]);
                    ++$processed;
                    if ($context->isCancelled()
                        || ($maximumMessages !== 0 && $processed >= $maximumMessages)
                    ) {
                        break;
                    }
                }
            }
        } finally {
            try {
                $provider->stop();
            } finally {
                $context->record(TransportEventCode::Stopped, ['processed' => $processed]);
            }
        }
        return $processed;
    }

    private function __construct()
    {
    }
}
