<?php

declare(strict_types=1);

namespace Nimbus\Notifications\Push;

use Nimbus\Notifications\Http\CurlHttpClient;
use Nimbus\Notifications\Http\HttpClient;
use Nimbus\Notifications\Http\HttpRequest;
use Nimbus\Notifications\Http\HttpResponse;
use Nimbus\Notifications\Http\HttpTransportException;
use Nimbus\Notifications\NotificationDeliveryException;

/**
 * Apple Push Notification service with token authentication: an ES256 JWT signed by the
 * `.p8` key, reused for 50 minutes, over HTTP/2 to api.push.apple.com.
 */
final class ApnsProvider implements PushProvider
{
    private const int TOKEN_LIFETIME_SECONDS = 3_000;

    private readonly HttpClient $http;
    /** @var \Closure(): int */
    private readonly \Closure $clock;
    private ?string $providerToken = null;
    private int $providerTokenIssuedAt = 0;

    /** @param (\Closure(): int)|null $clock */
    public function __construct(
        private readonly string $teamId,
        private readonly string $keyId,
        #[\SensitiveParameter] private readonly string $privateKey,
        private readonly string $topic,
        private readonly bool $production = true,
        ?HttpClient $http = null,
        private readonly float $timeout = 10.0,
        ?\Closure $clock = null,
    ) {
        if ($teamId === '' || $keyId === '' || $topic === '') {
            throw new \InvalidArgumentException('APNs team id, key id and topic (bundle id) are required.');
        }
        $this->http = $http ?? new CurlHttpClient();
        $this->clock = $clock ?? time(...);
    }

    public static function fromKeyFile(
        string $teamId,
        string $keyId,
        string $path,
        string $topic,
        bool $production = true,
        ?HttpClient $http = null,
    ): self {
        $key = is_readable($path) ? file_get_contents($path) : false;
        if ($key === false) {
            throw new \RuntimeException('The APNs key file is not readable.');
        }
        return new self($teamId, $keyId, $key, $topic, $production, $http);
    }

    public function send(PushMessage $message, string $token): PushOutcome
    {
        if (preg_match('/^[0-9a-fA-F]{32,200}$/D', $token) !== 1) {
            return PushOutcome::InvalidToken;
        }
        $headers = [
            'authorization' => 'bearer ' . $this->providerToken(),
            'apns-topic' => $this->topic,
            'apns-push-type' => $message->isSilent() ? 'background' : 'alert',
            'apns-priority' => $message->isHighPriority() ? '10' : '5',
            'content-type' => 'application/json',
        ];
        if ($message->ttlSeconds() !== null) {
            $headers['apns-expiration'] = (string) (($this->clock)() + $message->ttlSeconds());
        }
        if ($message->getCollapseKey() !== null) {
            $headers['apns-collapse-id'] = substr($message->getCollapseKey(), 0, 64);
        }
        $host = $this->production ? 'https://api.push.apple.com' : 'https://api.sandbox.push.apple.com';
        $body = json_encode($message->toApns(), JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE);
        try {
            $response = $this->http->send(new HttpRequest(
                'POST',
                $host . '/3/device/' . $token,
                $headers,
                $body,
                $this->timeout,
                http2: true,
            ));
        } catch (HttpTransportException $failure) {
            throw new NotificationDeliveryException('APNs request failed: ' . $failure->getMessage(), true, $failure);
        }
        return $this->outcome($response);
    }

    private function outcome(HttpResponse $response): PushOutcome
    {
        if ($response->successful()) {
            return PushOutcome::Delivered;
        }
        $reason = $response->json()['reason'] ?? null;
        $reason = is_string($reason) ? $reason : 'Unknown';
        if ($response->status === 410 || in_array($reason, ['BadDeviceToken', 'DeviceTokenNotForTopic', 'Unregistered'], true)) {
            return PushOutcome::InvalidToken;
        }
        if (in_array($reason, ['ExpiredProviderToken', 'InvalidProviderToken'], true)) {
            $this->providerToken = null;
        }
        $retryable = in_array($response->status, [403, 429], true) || $response->status >= 500;
        throw new NotificationDeliveryException("APNs answered HTTP {$response->status} ({$reason}).", $retryable);
    }

    private function providerToken(): string
    {
        $now = ($this->clock)();
        if ($this->providerToken !== null && $now - $this->providerTokenIssuedAt < self::TOKEN_LIFETIME_SECONDS) {
            return $this->providerToken;
        }
        $this->providerToken = JsonWebToken::es256(['iss' => $this->teamId, 'iat' => $now], $this->privateKey, ['kid' => $this->keyId]);
        $this->providerTokenIssuedAt = $now;
        return $this->providerToken;
    }
}
