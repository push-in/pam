<?php

declare(strict_types=1);

namespace Nimbus\Mail;

use Nimbus\Notifications\Mail\MailChannel;
use Nimbus\Notifications\Mail\MailMessage;
use Symfony\Component\Mailer\Exception\TransportExceptionInterface;
use Symfony\Component\Mailer\Transport;
use Symfony\Component\Mailer\Transport\TransportInterface;
use Symfony\Component\Mime\Address;
use Symfony\Component\Mime\Email;

/**
 * Sends e-mail outside notifications through symfony/mailer: a `MailMessage` (the same
 * builder notifications use) or a Symfony `Email`.
 *
 * ```php
 * $mailer->to('ana@example.com')->send(MailMessage::make()->subject('Welcome')->line('Hi!'));
 * ```
 *
 * Bind one in the container; `Mail::fake()` in tests replaces it with one over a `MailFake`.
 */
final readonly class Mailer
{
    public function __construct(
        private TransportInterface $transport,
        private string $fromAddress,
        private string $fromName = '',
    ) {
        if ($fromAddress === '') {
            throw new \InvalidArgumentException('The mail from address is required.');
        }
    }

    /**
     * Reads the same variables as `MailChannel::fromEnvironment()`.
     *
     * @param array<string, string>|null $environment Defaults to getenv().
     */
    public static function fromEnvironment(?array $environment = null): self
    {
        $environment ??= getenv();
        $from = $environment['MAIL_FROM_ADDRESS'] ?? '';
        if ($from === '') {
            throw new \RuntimeException('MAIL_FROM_ADDRESS must be set.');
        }
        return new self(Transport::fromDsn(MailChannel::dsn($environment)), $from, $environment['MAIL_FROM_NAME'] ?? '');
    }

    public function transport(): TransportInterface
    {
        return $this->transport;
    }

    /** The same mailer over another transport, such as a recording or fake one. */
    public function withTransport(TransportInterface $transport): self
    {
        return new self($transport, $this->fromAddress, $this->fromName);
    }

    /** The notification channel over this mailer's transport and sender. */
    public function channel(): MailChannel
    {
        return new MailChannel($this->transport, $this->fromAddress, $this->fromName);
    }

    /** @param string|list<string>|array<string, string> $addresses An address, a list or an `address => name` map. */
    public function to(string|array $addresses): PendingMail
    {
        return new PendingMail($this, self::addresses($addresses));
    }

    /**
     * Sends an Email as is (adding the default sender when it has none), or a MailMessage
     * to the given recipients.
     *
     * @param list<Address> $to
     * @throws TransportExceptionInterface
     */
    public function send(Email|MailMessage $message, array $to = []): void
    {
        $email = $message instanceof Email ? $message : $this->email($message, $to);
        if ($email->getFrom() === []) {
            $email->from(new Address($this->fromAddress, $this->fromName));
        }
        $this->transport->send($email);
    }

    /** @param list<Address> $to */
    private function email(MailMessage $message, array $to): Email
    {
        if ($to === []) {
            throw new \InvalidArgumentException('A mail message needs at least one recipient: use to().');
        }
        $payload = $message->toArray();
        $email = (new Email())
            ->subject($message->getSubject())
            ->text($message->renderText())
            ->html($message->renderHtml())
            ->to(...$to);
        if ($payload['from'] !== null) {
            $email->from(new Address($payload['from']['address'], $payload['from']['name']));
        }
        if ($payload['cc'] !== []) {
            $email->cc(...$payload['cc']);
        }
        if ($payload['bcc'] !== []) {
            $email->bcc(...$payload['bcc']);
        }
        if ($payload['reply_to'] !== []) {
            $email->replyTo(...$payload['reply_to']);
        }
        return $email;
    }

    /**
     * @param string|list<string>|array<string, string> $addresses
     * @return list<Address>
     */
    private static function addresses(string|array $addresses): array
    {
        if (is_string($addresses)) {
            return [new Address($addresses)];
        }
        $list = [];
        foreach ($addresses as $key => $value) {
            $list[] = is_string($key) ? new Address($key, $value) : new Address($value);
        }
        return $list;
    }
}
