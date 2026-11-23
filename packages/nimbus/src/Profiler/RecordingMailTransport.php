<?php

declare(strict_types=1);

namespace Nimbus\Profiler;

use Symfony\Component\Mailer\Envelope;
use Symfony\Component\Mailer\SentMessage;
use Symfony\Component\Mailer\Transport\TransportInterface;
use Symfony\Component\Mime\Address;
use Symfony\Component\Mime\Email;
use Symfony\Component\Mime\RawMessage;

/**
 * Records sent mails in the debug panel, then sends them through the inner transport.
 *
 * @internal Created through DebugPanel.
 */
final readonly class RecordingMailTransport implements TransportInterface
{
    public function __construct(private TransportInterface $inner, private DebugRecorder $recorder)
    {
    }

    public function send(RawMessage $message, ?Envelope $envelope = null): ?SentMessage
    {
        $sent = $this->inner->send($message, $envelope);
        if ($message instanceof Email) {
            $body = $message->getTextBody();
            $this->recorder->mail(
                $message->getSubject() ?? '',
                array_values(array_map(static fn (Address $address): string => $address->toString(), $message->getTo())),
                is_string($body) ? $body : null,
            );
        }
        return $sent;
    }

    public function __toString(): string
    {
        return (string) $this->inner;
    }
}
