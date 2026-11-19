<?php

declare(strict_types=1);

namespace Nimbus\Notifications\Push;

use Nimbus\Notifications\Http\HttpClient;
use Nimbus\Notifications\Http\HttpClients;
use Nimbus\Notifications\Http\HttpRequest;
use Nimbus\Notifications\Http\HttpResponse;
use Nimbus\Notifications\Http\HttpTransportException;
use Nimbus\Notifications\NotificationDeliveryException;

/**
 * Firebase Cloud Messaging HTTP v1. A service account signs an RS256 assertion exchanged
 * for an OAuth2 access token, cached by this instance (one per worker) until shortly before expiry.
 */
final class FcmProvider implements PushProvider
{
    private const string SCOPE = 'https://www.googleapis.com/auth/firebase.messaging';
    private const string ENDPOINT = 'https://fcm.googleapis.com/v1/projects/%s/messages:send';

    private readonly string $projectId;
    private readonly string $clientEmail;
    private readonly string $privateKey;
    private readonly string $tokenUri;
    private readonly HttpClient $http;
    /** @var \Closure(): int */
    private readonly \Closure $clock;
    private ?string $accessToken = null;
    private int $accessTokenExpiresAt = 0;

    /**
     * @param array<array-key, mixed> $serviceAccount Decoded service account JSON.
     * @param (\Closure(): int)|null $clock
     */
    public function __construct(
        #[\SensitiveParameter] array $serviceAccount,
        ?HttpClient $http = null,
        private readonly float $timeout = 10.0,
        ?\Closure $clock = null,
    ) {
        foreach (['project_id', 'client_email', 'private_key'] as $key) {
            if (!is_string($serviceAccount[$key] ?? null) || $serviceAccount[$key] === '') {
                throw new \InvalidArgumentException("The FCM service account is missing {$key}.");
            }
        }
        $tokenUri = $serviceAccount['token_uri'] ?? null;
        /** @var array{project_id: string, client_email: string, private_key: string} $serviceAccount */
        $this->projectId = $serviceAccount['project_id'];
        $this->clientEmail = $serviceAccount['client_email'];
        $this->privateKey = $serviceAccount['private_key'];
        $this->tokenUri = is_string($tokenUri) && str_starts_with($tokenUri, 'https://')
            ? $tokenUri
            : 'https://oauth2.googleapis.com/token';
        $this->http = $http ?? HttpClients::detect();
        $this->clock = $clock ?? time(...);
    }

    public static function fromServiceAccountFile(string $path, ?HttpClient $http = null): self
    {
        $json = is_readable($path) ? file_get_contents($path) : false;
        if ($json === false) {
            throw new \RuntimeException('The FCM service account file is not readable.');
        }
        return self::fromServiceAccountJson($json, $http);
    }

    public static function fromServiceAccountJson(#[\SensitiveParameter] string $json, ?HttpClient $http = null): self
    {
        $decoded = json_decode($json, true, 16, JSON_THROW_ON_ERROR);
        if (!is_array($decoded)) {
            throw new \InvalidArgumentException('The FCM service account JSON must be an object.');
        }
        return new self($decoded, $http);
    }

    public function send(PushMessage $message, string $token): PushOutcome
    {
        $body = json_encode(
            ['message' => ['token' => $token] + $message->toFcm()],
            JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE,
        );
        $response = $this->post(new HttpRequest('POST', sprintf(self::ENDPOINT, rawurlencode($this->projectId)), [
            'Authorization' => 'Bearer ' . $this->accessToken(),
            'Content-Type' => 'application/json; charset=utf-8',
        ], $body, $this->timeout));
        if ($response->successful()) {
            return PushOutcome::Delivered;
        }
        $code = self::errorCode($response);
        if ($response->status === 404 || $code === 'UNREGISTERED') {
            return PushOutcome::InvalidToken;
        }
        if ($response->status === 401) {
            $this->accessToken = null;
        }
        $retryable = in_array($response->status, [401, 408, 429], true) || $response->status >= 500;
        throw new NotificationDeliveryException("FCM answered HTTP {$response->status} ({$code}).", $retryable);
    }

    private function accessToken(): string
    {
        $now = ($this->clock)();
        if ($this->accessToken !== null && $now < $this->accessTokenExpiresAt) {
            return $this->accessToken;
        }
        $assertion = JsonWebToken::rs256([
            'iss' => $this->clientEmail,
            'scope' => self::SCOPE,
            'aud' => $this->tokenUri,
            'iat' => $now,
            'exp' => $now + 3_600,
        ], $this->privateKey);
        $response = $this->post(new HttpRequest('POST', $this->tokenUri, [
            'Content-Type' => 'application/x-www-form-urlencoded',
        ], http_build_query([
            'grant_type' => 'urn:ietf:params:oauth:grant-type:jwt-bearer',
            'assertion' => $assertion,
        ]), $this->timeout));
        $json = $response->json();
        $token = $json['access_token'] ?? null;
        if (!$response->successful() || !is_string($token) || $token === '') {
            throw new NotificationDeliveryException(
                "Google OAuth2 answered HTTP {$response->status}.",
                $response->status === 429 || $response->status >= 500,
            );
        }
        $expiresIn = is_int($json['expires_in'] ?? null) ? $json['expires_in'] : 3_600;
        $this->accessToken = $token;
        $this->accessTokenExpiresAt = $now + max(60, $expiresIn - 300);
        return $token;
    }

    private function post(HttpRequest $request): HttpResponse
    {
        try {
            return $this->http->send($request);
        } catch (HttpTransportException $failure) {
            throw new NotificationDeliveryException('FCM request failed: ' . $failure->getMessage(), true, $failure);
        }
    }

    private static function errorCode(HttpResponse $response): string
    {
        $error = $response->json()['error'] ?? null;
        if (!is_array($error)) {
            return 'UNKNOWN';
        }
        foreach (is_array($error['details'] ?? null) ? $error['details'] : [] as $detail) {
            if (is_array($detail) && is_string($detail['errorCode'] ?? null)) {
                return $detail['errorCode'];
            }
        }
        return is_string($error['status'] ?? null) ? $error['status'] : 'UNKNOWN';
    }
}
