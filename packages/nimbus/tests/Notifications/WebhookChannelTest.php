<?php

declare(strict_types=1);

namespace Nimbus\Tests\Notifications;

use Nimbus\Notifications\Http\HttpResponse;
use Nimbus\Notifications\Http\HttpTransportException;
use Nimbus\Notifications\NotificationDeliveryException;
use Nimbus\Notifications\NotificationSender;
use Nimbus\Notifications\Webhook\WebhookChannel;
use Nimbus\Notifications\Webhook\WebhookMessage;
use Nimbus\Notifications\Webhook\WebhookSignature;
use Nimbus\Tests\Notifications\Fixtures\FakeHttpClient;
use Nimbus\Tests\Notifications\Fixtures\InvoicePaid;
use Nimbus\Tests\Notifications\Fixtures\User;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\Attributes\DataProvider;
use PHPUnit\Framework\TestCase;

#[CoversClass(WebhookChannel::class)]
#[CoversClass(WebhookMessage::class)]
#[CoversClass(WebhookSignature::class)]
final class WebhookChannelTest extends TestCase
{
    public function testItPostsSignedJson(): void
    {
        $http = new FakeHttpClient([new HttpResponse(204)]);
        $channel = new WebhookChannel('whsec_test', $http, timeout: 3.0, clock: static fn (): int => 1_800_000_000);

        (new NotificationSender(['webhook' => $channel]))
            ->send(new User(1, webhookUrl: 'https://hooks.example.test/in'), new InvoicePaid(990, ['webhook']));

        $request = $http->requests[0];
        self::assertSame('POST', $request->method);
        self::assertSame('https://hooks.example.test/in', $request->url);
        self::assertSame(3.0, $request->timeout);
        self::assertSame('invoice.paid', $request->headers['Nimbus-Event']);
        $body = json_decode($request->body, true);
        self::assertIsArray($body);
        self::assertSame($body['id'], $request->headers['Nimbus-Delivery']);
        self::assertSame('invoice.paid', $body['event']);
        self::assertSame('2027-01-15T08:00:00Z', $body['created_at']);
        self::assertSame(['amount_in_cents' => 990], $body['data']);
        $signature = $request->headers[WebhookSignature::HEADER];
        self::assertStringStartsWith('t=1800000000,v1=', $signature);
        self::assertTrue(WebhookSignature::verify($signature, $request->body, 'whsec_test', now: 1_800_000_100));
        self::assertFalse(WebhookSignature::verify($signature, $request->body, 'other', now: 1_800_000_100));
        self::assertFalse(WebhookSignature::verify($signature, $request->body . ' ', 'whsec_test', now: 1_800_000_100));
        self::assertFalse(WebhookSignature::verify($signature, $request->body, 'whsec_test', now: 1_800_001_000));
    }

    /** @return iterable<string, array{HttpResponse|HttpTransportException, bool}> */
    public static function failures(): iterable
    {
        yield 'timeout' => [new HttpTransportException('timeout'), true];
        yield '429' => [new HttpResponse(429), true];
        yield '503' => [new HttpResponse(503), true];
        yield '410' => [new HttpResponse(410), false];
        yield '302' => [new HttpResponse(302), false];
    }

    #[DataProvider('failures')]
    public function testFailuresAreClassifiedForTheQueue(HttpResponse|HttpTransportException $response, bool $retryable): void
    {
        $channel = new WebhookChannel('secret', new FakeHttpClient([$response]));

        try {
            $channel->deliver(['url' => 'https://hooks.example.test', 'id' => 'i', 'event' => 'e', 'headers' => []]);
            self::fail('Expected a delivery failure.');
        } catch (NotificationDeliveryException $failure) {
            self::assertSame($retryable, $failure->retryable);
        }
    }

    public function testPayloadsCarryNoSecretAndPerUrlSecretsSign(): void
    {
        $http = new FakeHttpClient();
        $channel = new WebhookChannel('', $http, secretFor: static fn (string $url): string => 'secret-for-' . parse_url($url, PHP_URL_HOST));
        $payload = $channel->prepare(new User(1, webhookUrl: 'https://a.example.test/x'), new InvoicePaid(1, ['webhook']), 'id-1');

        self::assertIsArray($payload);
        self::assertStringNotContainsString('secret', (string) json_encode($payload));
        $channel->deliver($payload);
        self::assertTrue(WebhookSignature::verify(
            $http->requests[0]->headers[WebhookSignature::HEADER],
            $http->requests[0]->body,
            'secret-for-a.example.test',
        ));
        self::assertNull($channel->prepare(new User(2), new InvoicePaid(1, ['webhook']), 'id-2'));
    }
}
