<?php

declare(strict_types=1);

namespace Nimbus\Auth;

use Nimbus\Notifications\Notification;
use Nimbus\Notifications\NotificationDispatcher;

/**
 * E-mail verification by API: `send()` mails a single-use link (`{token}` in the verification
 * URL) and `verify()` spends the token and marks the address verified. Sent synchronously so
 * the token never enters a job payload.
 *
 * Each token is bound to the address it was mailed to (its subject carries a hash of the
 * address the user's `mail` route returned): after the user changes the address, an older link
 * no longer verifies the new one.
 */
final readonly class EmailVerification
{
    public const string PURPOSE = 'email-verification';

    /** @var (\Closure(AuthUser, string): Notification)|null */
    private ?\Closure $notification;

    /**
     * @param string $verifyUrl Link of the application's confirmation screen, with `{token}`.
     * @param (\Closure(AuthUser, string): Notification)|null $notification
     */
    public function __construct(
        private AuthUserProvider $users,
        private OneTimeTokens $tokens,
        private NotificationDispatcher $notifications,
        private string $verifyUrl,
        private int $ttlSeconds = 86_400,
        private int $throttleSeconds = 60,
        ?\Closure $notification = null,
    ) {
        if (!str_contains($verifyUrl, '{token}')) {
            throw new \InvalidArgumentException('The verification URL must contain the {token} placeholder.');
        }
        if ($ttlSeconds < 60 || $ttlSeconds > 604_800 || $throttleSeconds < 0) {
            throw new \InvalidArgumentException('E-mail verification TTL or throttle is invalid.');
        }
        $this->notification = $notification;
    }

    /** Mails a link unless the address is verified or a link was sent in the throttle window. */
    public function send(AuthUser $user): bool
    {
        if ($user->hasVerifiedEmail()) {
            return false;
        }
        $subject = self::subject($user);
        if ($this->tokens->createdWithin(self::PURPOSE, $subject, $this->throttleSeconds)) {
            return false;
        }
        $token = $this->tokens->create(self::PURPOSE, $subject, $this->ttlSeconds);
        $url = strtr($this->verifyUrl, ['{token}' => rawurlencode($token)]);
        $message = $this->notification === null
            ? new VerifyEmailNotification($url, intdiv($this->ttlSeconds, 60))
            : ($this->notification)($user, $url);
        $this->notifications->send($user, $message);
        return true;
    }

    /** Spends the token and marks the user's address verified; false for an invalid token. */
    public function verify(#[\SensitiveParameter] string $token): bool
    {
        $subject = $this->tokens->consume(self::PURPOSE, $token);
        $separator = $subject === null ? false : strrpos($subject, '|');
        if ($subject === null || $separator === false) {
            return false;
        }
        $user = $this->users->findById(substr($subject, 0, $separator));
        // The address changed since the link was sent: the token does not verify the new one.
        if ($user === null || !hash_equals(self::subject($user), $subject)) {
            return false;
        }
        if (!$user->hasVerifiedEmail()) {
            $this->users->markEmailAsVerified($user);
        }
        return true;
    }

    /** The token subject: the user's identifier and a hash of the address being verified. */
    private static function subject(AuthUser $user): string
    {
        $route = $user->routeNotificationFor('mail', new VerifyEmailNotification('', 0));
        if (is_array($route)) {
            $first = array_key_first($route);
            $route = is_string($first) ? $first : ($first === null ? '' : $route[$first]);
        }
        $address = is_string($route) ? strtolower(trim($route)) : '';
        return $user->authIdentifier() . '|' . substr(hash('sha256', $address), 0, 32);
    }

    public function users(): AuthUserProvider
    {
        return $this->users;
    }
}
