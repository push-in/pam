<?php

declare(strict_types=1);

namespace Nimbus\Notifications\Webhook;

/**
 * `Nimbus-Signature: t=<unix time>,v1=<hex HMAC-SHA256 of "t.body">`, the format of
 * Stripe-style webhooks. Receivers verify it with `verify()`.
 */
final class WebhookSignature
{
    public const string HEADER = 'Nimbus-Signature';

    private function __construct()
    {
    }

    public static function sign(#[\SensitiveParameter] string $secret, string $body, int $timestamp): string
    {
        return 't=' . $timestamp . ',v1=' . hash_hmac('sha256', $timestamp . '.' . $body, $secret);
    }

    /** Checks the signature and that its timestamp is within the tolerance window. */
    public static function verify(
        string $header,
        string $body,
        #[\SensitiveParameter] string $secret,
        int $toleranceSeconds = 300,
        ?int $now = null,
    ): bool {
        $timestamp = null;
        $signatures = [];
        foreach (explode(',', $header, 16) as $part) {
            [$key, $value] = array_pad(explode('=', trim($part), 2), 2, '');
            if ($key === 't' && ctype_digit($value)) {
                $timestamp = (int) $value;
            } elseif ($key === 'v1' && $value !== '') {
                $signatures[] = $value;
            }
        }
        if ($timestamp === null || $signatures === [] || abs(($now ?? time()) - $timestamp) > $toleranceSeconds) {
            return false;
        }
        $expected = hash_hmac('sha256', $timestamp . '.' . $body, $secret);
        foreach ($signatures as $signature) {
            if (hash_equals($expected, $signature)) {
                return true;
            }
        }
        return false;
    }
}
