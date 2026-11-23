<?php

declare(strict_types=1);

namespace Nimbus\Auth;

use Nimbus\Jobs\SerializableJob;

/**
 * The queued half of `PasswordResets::requestResetLink()`: carries only the address, and the
 * job worker looks the user up, creates the token and sends the e-mail. Register it in the
 * application's JobCodec and run it with an InvokableJobHandler over the container, which
 * injects the PasswordResets.
 */
final readonly class SendPasswordResetLink implements SerializableJob
{
    public function __construct(public string $email)
    {
    }

    public static function jobName(): string
    {
        return 'nimbus.auth.password-reset-link';
    }

    public function toJobPayload(): array
    {
        return ['email' => $this->email];
    }

    public static function fromJobPayload(array $payload): self
    {
        $email = $payload['email'] ?? null;
        if (!is_string($email) || $email === '' || strlen($email) > 254) {
            throw new \InvalidArgumentException('The password reset job needs an e-mail address.');
        }
        return new self($email);
    }

    public function handle(PasswordResets $passwords): void
    {
        $passwords->sendResetLink($this->email);
    }
}
