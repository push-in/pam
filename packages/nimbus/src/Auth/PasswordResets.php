<?php

declare(strict_types=1);

namespace Nimbus\Auth;

use Nimbus\Notifications\Notification;
use Nimbus\Notifications\NotificationDispatcher;
use Nimbus\Security\Hash;

/**
 * Password reset by API: `sendResetLink()` mails a single-use link (the reset URL with the
 * token in `{token}`), `reset()` spends the token and stores the new hash. The e-mail is sent
 * synchronously so the token never enters a job payload; unknown addresses and throttled
 * requests are silent, so the response does not reveal which e-mails exist.
 */
final readonly class PasswordResets
{
    public const string PURPOSE = 'password-reset';

    /** @var (\Closure(AuthUser, string): Notification)|null */
    private ?\Closure $notification;

    /** @var (\Closure(AuthUser): void)|null */
    private ?\Closure $afterReset;

    /**
     * @param string $resetUrl Link of the application's reset screen, with `{token}` (and optionally `{email}`).
     * @param int $throttleSeconds Minimum interval between two links to the same user.
     * @param (\Closure(AuthUser, string): Notification)|null $notification Builds the message from the user and the link.
     * @param (\Closure(AuthUser): void)|null $afterReset Runs after a reset, e.g. to revoke the user's tokens.
     */
    public function __construct(
        private AuthUserProvider $users,
        private OneTimeTokens $tokens,
        private NotificationDispatcher $notifications,
        private string $resetUrl,
        private int $ttlSeconds = 3_600,
        private int $throttleSeconds = 60,
        private int $minimumPasswordLength = 8,
        ?\Closure $notification = null,
        ?\Closure $afterReset = null,
    ) {
        if (!str_contains($resetUrl, '{token}')) {
            throw new \InvalidArgumentException('The reset URL must contain the {token} placeholder.');
        }
        if ($ttlSeconds < 60 || $ttlSeconds > 86_400 || $throttleSeconds < 0 || $minimumPasswordLength < 8) {
            throw new \InvalidArgumentException('Password reset TTL, throttle or minimum length is invalid.');
        }
        $this->notification = $notification;
        $this->afterReset = $afterReset;
    }

    /** Mails a reset link when the address belongs to a user and no link was sent in the throttle window. */
    public function sendResetLink(string $email): bool
    {
        $email = trim($email);
        if ($email === '' || strlen($email) > 254) {
            return false;
        }
        $user = $this->users->findByEmail($email);
        if ($user === null) {
            return false;
        }
        $subject = $user->authIdentifier();
        if ($this->tokens->createdWithin(self::PURPOSE, $subject, $this->throttleSeconds)) {
            return false;
        }
        $token = $this->tokens->create(self::PURPOSE, $subject, $this->ttlSeconds);
        $url = strtr($this->resetUrl, ['{token}' => rawurlencode($token), '{email}' => rawurlencode($email)]);
        $message = $this->notification === null
            ? new ResetPasswordNotification($url, intdiv($this->ttlSeconds, 60))
            : ($this->notification)($user, $url);
        $this->notifications->send($user, $message);
        return true;
    }

    /**
     * Spends the token and sets the password.
     *
     * @throws \InvalidArgumentException When the password is too short or too long.
     * @return bool False when the token is invalid, expired or already used.
     */
    public function reset(#[\SensitiveParameter] string $token, #[\SensitiveParameter] string $password): bool
    {
        $this->assertPassword($password);
        $subject = $this->tokens->consume(self::PURPOSE, $token);
        $user = $subject === null ? null : $this->users->findById($subject);
        if ($user === null) {
            return false;
        }
        $this->users->updatePassword($user, Hash::make($password));
        if ($this->afterReset !== null) {
            ($this->afterReset)($user);
        }
        return true;
    }

    public function assertPassword(#[\SensitiveParameter] string $password): void
    {
        $length = mb_strlen($password);
        if ($length < $this->minimumPasswordLength || strlen($password) > 1_024) {
            throw new \InvalidArgumentException("The password must have at least {$this->minimumPasswordLength} characters.");
        }
    }
}
