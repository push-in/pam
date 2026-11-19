<?php

declare(strict_types=1);

namespace Nimbus\Notifications\Webhook;

use Nimbus\Notifications\Http\HttpClient;
use Nimbus\Notifications\Http\HttpClients;
use Nimbus\Notifications\Http\HttpRequest;
use Nimbus\Notifications\Http\HttpTransportException;
use Nimbus\Notifications\Notifiable;
use Nimbus\Notifications\Notification;
use Nimbus\Notifications\NotificationChannel;
use Nimbus\Notifications\NotificationDeliveryException;

/**
 * Signed JSON POSTs to the URL routed by the notifiable. Timeouts, 408, 429 and 5xx are
 * retryable (the job queue retries them with `sendLater()`); other 4xx are permanent.
 * The secret never enters the job payload.
 */
final readonly class WebhookChannel implements NotificationChannel
{
    private HttpClient $http;
    /** @var \Closure(): int */
    private \Closure $clock;

    /**
     * @param (\Closure(string): string)|null $secretFor Per-URL secret; defaults to `$secret`.
     * @param (\Closure(): int)|null $clock
     */
    public function __construct(
        #[\SensitiveParameter] private string $secret,
        ?HttpClient $http = null,
        private float $timeout = 10.0,
        private ?\Closure $secretFor = null,
        ?\Closure $clock = null,
    ) {
        if ($secret === '' && $secretFor === null) {
            throw new \InvalidArgumentException('Webhook signing secret is required.');
        }
        $this->http = $http ?? HttpClients::detect();
        $this->clock = $clock ?? time(...);
    }

    public function prepare(Notifiable $notifiable, Notification $notification, string $id): ?array
    {
        $url = $notifiable->routeNotificationFor('webhook', $notification);
        if ($url === null || $url === '' || $url === []) {
            return null;
        }
        if (!is_string($url) || !in_array(parse_url($url, PHP_URL_SCHEME), ['http', 'https'], true)) {
            throw new \InvalidArgumentException('The webhook route must be an http(s) URL.');
        }
        $message = $notification->toWebhook($notifiable);
        $message = is_array($message) ? WebhookMessage::make($message)->toArray() : $message->toArray();
        return [
            'url' => $url,
            'id' => $id,
            'event' => $message['event'] ?? $notification->type(),
            'created_at' => gmdate('Y-m-d\TH:i:s\Z', ($this->clock)()),
            'data' => $message['data'],
            'headers' => $message['headers'],
        ];
    }

    public function deliver(array $payload): void
    {
        $url = $payload['url'] ?? null;
        $id = $payload['id'] ?? null;
        $event = $payload['event'] ?? null;
        $headers = $payload['headers'] ?? [];
        if (!is_string($url) || !is_string($id) || !is_string($event) || !is_array($headers)) {
            throw NotificationDeliveryException::permanent('Webhook payload is invalid.');
        }
        $body = json_encode([
            'id' => $id,
            'event' => $event,
            'created_at' => $payload['created_at'] ?? null,
            'data' => $payload['data'] ?? [],
        ], JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE);
        $secret = $this->secretFor === null ? $this->secret : ($this->secretFor)($url);
        $request = [];
        foreach ($headers as $name => $value) {
            if (is_string($name) && is_string($value)) {
                $request[$name] = $value;
            }
        }
        $request = [
            'Content-Type' => 'application/json',
            'User-Agent' => 'Nimbus-Webhook/1',
            'Nimbus-Event' => $event,
            'Nimbus-Delivery' => $id,
            WebhookSignature::HEADER => WebhookSignature::sign($secret, $body, ($this->clock)()),
        ] + $request;
        try {
            $response = $this->http->send(new HttpRequest('POST', $url, $request, $body, $this->timeout));
        } catch (HttpTransportException $failure) {
            throw new NotificationDeliveryException('Webhook delivery failed: ' . $failure->getMessage(), true, $failure);
        }
        if ($response->successful()) {
            return;
        }
        $retryable = $response->status === 408 || $response->status === 429 || $response->status >= 500;
        throw new NotificationDeliveryException("Webhook endpoint answered HTTP {$response->status}.", $retryable);
    }
}
