<?php

declare(strict_types=1);

namespace Nimbus\Mail;

use Nimbus\Notifications\Mail\MailMessage;
use Symfony\Component\Mime\Address;
use Symfony\Component\Mime\Email;

/** Recipients chosen with `Mailer::to()`, waiting for the message. */
final readonly class PendingMail
{
    /** @param list<Address> $to */
    public function __construct(private Mailer $mailer, private array $to)
    {
    }

    public function send(Email|MailMessage $message): void
    {
        if ($message instanceof Email) {
            $message->to(...$this->to);
        }
        $this->mailer->send($message, $this->to);
    }
}
