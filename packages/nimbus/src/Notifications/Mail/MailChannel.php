<?php

declare(strict_types=1);

namespace Nimbus\Notifications\Mail;

use Nimbus\Notifications\Notifiable;
use Nimbus\Notifications\Notification;
use Nimbus\Notifications\NotificationChannel;
use Nimbus\Notifications\NotificationDeliveryException;
use Symfony\Component\Mailer\Exception\TransportExceptionInterface;
use Symfony\Component\Mailer\Transport;
use Symfony\Component\Mailer\Transport\TransportInterface;
use Symfony\Component\Mime\Address;
use Symfony\Component\Mime\Email;
use Symfony\Component\Mime\Exception\ExceptionInterface as MimeException;

/**
 * E-mail through symfony/mailer. The route is an address, a list of addresses or an
 * `address => name` map.
 */
final readonly class MailChannel implements NotificationChannel
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

    /** Builds the transport from a DSN such as `smtp://user:pass@host:587` or `null://null`. */
    public static function fromDsn(#[\SensitiveParameter] string $dsn, string $fromAddress, string $fromName = ''): self
    {
        self::assertInstalled();
        return new self(Transport::fromDsn($dsn), $fromAddress, $fromName);
    }

    /**
     * Reads `MAILER_DSN` (or `MAIL_MAILER` as a DSN or `smtp`, `sendmail`, `null`, with
     * `MAIL_HOST`, `MAIL_PORT`, `MAIL_USERNAME`, `MAIL_PASSWORD`) and `MAIL_FROM_ADDRESS`, `MAIL_FROM_NAME`.
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
        return self::fromDsn(self::dsn($environment), $from, $environment['MAIL_FROM_NAME'] ?? '');
    }

    /** @param array<string, string> $environment */
    public static function dsn(array $environment): string
    {
        $dsn = $environment['MAILER_DSN'] ?? '';
        if ($dsn !== '') {
            return $dsn;
        }
        $mailer = $environment['MAIL_MAILER'] ?? '';
        if (str_contains($mailer, '://')) {
            return $mailer;
        }
        return match ($mailer) {
            'smtp' => sprintf(
                'smtp://%s%s:%s',
                ($environment['MAIL_USERNAME'] ?? '') === '' ? '' : rawurlencode($environment['MAIL_USERNAME'])
                    . ':' . rawurlencode($environment['MAIL_PASSWORD'] ?? '') . '@',
                $environment['MAIL_HOST'] ?? '127.0.0.1',
                $environment['MAIL_PORT'] ?? '587',
            ),
            'sendmail' => 'sendmail://default',
            'null', 'array', 'log' => 'null://null',
            default => throw new \RuntimeException('Set MAILER_DSN or MAIL_MAILER (smtp, sendmail, null or a DSN).'),
        };
    }

    public function prepare(Notifiable $notifiable, Notification $notification, string $id): ?array
    {
        $to = self::recipients($notifiable->routeNotificationFor('mail', $notification));
        if ($to === []) {
            return null;
        }
        return ['id' => $id, 'to' => $to] + $notification->toMail($notifiable)->toArray();
    }

    public function deliver(array $payload): void
    {
        self::assertInstalled();
        try {
            $email = $this->email($payload);
        } catch (MimeException $invalid) {
            throw NotificationDeliveryException::permanent('Mail message is invalid: ' . $invalid->getMessage(), $invalid);
        }
        try {
            $this->transport->send($email);
        } catch (TransportExceptionInterface $failure) {
            throw new NotificationDeliveryException('Mail delivery failed: ' . $failure->getMessage(), true, $failure);
        }
    }

    /** @param array<string, mixed> $payload */
    private function email(array $payload): Email
    {
        $email = (new Email())
            ->from(self::from($payload['from'] ?? null) ?? new Address($this->fromAddress, $this->fromName))
            ->subject(self::string($payload, 'subject'))
            ->text(self::string($payload, 'text'))
            ->html(self::string($payload, 'html'));
        foreach (self::addresses($payload['to'] ?? []) as $address) {
            $email->addTo($address);
        }
        foreach (self::addresses($payload['cc'] ?? []) as $address) {
            $email->addCc($address);
        }
        foreach (self::addresses($payload['bcc'] ?? []) as $address) {
            $email->addBcc($address);
        }
        foreach (self::addresses($payload['reply_to'] ?? []) as $address) {
            $email->addReplyTo($address);
        }
        if (is_string($payload['id'] ?? null)) {
            $email->getHeaders()->addTextHeader('X-Nimbus-Notification', $payload['id']);
        }
        return $email;
    }

    /** @return list<array{address: string, name: string}> */
    private static function recipients(mixed $route): array
    {
        if ($route === null || $route === '') {
            return [];
        }
        if (is_string($route)) {
            return [['address' => $route, 'name' => '']];
        }
        if (!is_array($route)) {
            throw new \InvalidArgumentException('The mail route must be an address or a list of addresses.');
        }
        $recipients = [];
        foreach ($route as $key => $value) {
            if (is_int($key) && is_string($value)) {
                $recipients[] = ['address' => $value, 'name' => ''];
            } elseif (is_string($key) && is_string($value)) {
                $recipients[] = ['address' => $key, 'name' => $value];
            } else {
                throw new \InvalidArgumentException('The mail route must contain address strings.');
            }
        }
        return $recipients;
    }

    /** @return list<Address> */
    private static function addresses(mixed $value): array
    {
        $addresses = [];
        foreach (is_array($value) ? $value : [] as $item) {
            if (is_string($item)) {
                $addresses[] = new Address($item);
            } elseif (is_array($item) && is_string($item['address'] ?? null)) {
                $addresses[] = new Address($item['address'], is_string($item['name'] ?? null) ? $item['name'] : '');
            }
        }
        return $addresses;
    }

    private static function from(mixed $value): ?Address
    {
        return self::addresses(is_array($value) ? [$value] : [])[0] ?? null;
    }

    /** @param array<string, mixed> $payload */
    private static function string(array $payload, string $key): string
    {
        $value = $payload[$key] ?? '';
        return is_string($value) ? $value : '';
    }

    private static function assertInstalled(): void
    {
        if (!interface_exists(TransportInterface::class) || !class_exists(Email::class)) {
            throw new \LogicException('The mail channel requires symfony/mailer and symfony/mime: composer require symfony/mailer.');
        }
    }
}
