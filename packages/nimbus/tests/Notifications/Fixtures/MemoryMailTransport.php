<?php

declare(strict_types=1);

namespace Nimbus\Tests\Notifications\Fixtures;

use Symfony\Component\Mailer\Envelope;
use Symfony\Component\Mailer\Exception\TransportException;
use Symfony\Component\Mailer\SentMessage;
use Symfony\Component\Mailer\Transport\TransportInterface;
use Symfony\Component\Mime\Email;
use Symfony\Component\Mime\RawMessage;

final class MemoryMailTransport implements TransportInterface
{
    /** @var list<Email> */
    public array $sent = [];
    public bool $fail = false;

    public function send(RawMessage $message, ?Envelope $envelope = null): SentMessage
    {
        if ($this->fail) {
            throw new TransportException('SMTP is down.');
        }
        if ($message instanceof Email) {
            $this->sent[] = $message;
        }
        return new SentMessage($message, $envelope ?? Envelope::create($message));
    }

    public function __toString(): string
    {
        return 'memory://';
    }
}
