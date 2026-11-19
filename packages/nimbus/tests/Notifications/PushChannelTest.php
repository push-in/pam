<?php

declare(strict_types=1);

namespace Nimbus\Tests\Notifications;

use Nimbus\Notifications\Http\HttpResponse;
use Nimbus\Notifications\Http\HttpTransportException;
use Nimbus\Notifications\NotificationDeliveryException;
use Nimbus\Notifications\NotificationSender;
use Nimbus\Notifications\Push\ApnsProvider;
use Nimbus\Notifications\Push\FcmProvider;
use Nimbus\Notifications\Push\JsonWebToken;
use Nimbus\Notifications\Push\PushChannel;
use Nimbus\Notifications\Push\PushMessage;
use Nimbus\Notifications\Push\PushOutcome;
use Nimbus\Notifications\Push\PushTarget;
use Nimbus\Tests\Notifications\Fixtures\FakeHttpClient;
use Nimbus\Tests\Notifications\Fixtures\InvoicePaid;
use Nimbus\Tests\Notifications\Fixtures\User;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(PushChannel::class)]
#[CoversClass(PushMessage::class)]
#[CoversClass(PushTarget::class)]
#[CoversClass(FcmProvider::class)]
#[CoversClass(ApnsProvider::class)]
#[CoversClass(JsonWebToken::class)]
final class PushChannelTest extends TestCase
{
    private const string APNS_TOKEN = 'a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90';

    private static ?string $rsaKey = null;
    private static ?string $ecKey = null;

    public function testFcmExchangesAServiceAccountJwtOnceAndSendsV1Messages(): void
    {
        $http = new FakeHttpClient([
            new HttpResponse(200, [], '{"access_token":"ya29.token","expires_in":3599}'),
            new HttpResponse(200, [], '{"name":"projects/demo/messages/1"}'),
            new HttpResponse(200, [], '{"name":"projects/demo/messages/2"}'),
        ]);
        $now = 1_800_000_000;
        $fcm = new FcmProvider(self::serviceAccount(), $http, clock: static fn (): int => $now);
        $message = PushMessage::make('Hi', 'There')->data(['id' => 7, 'ok' => true])->badge(3)->collapseKey('inbox');

        self::assertSame(PushOutcome::Delivered, $fcm->send($message, 'token-1'));
        self::assertSame(PushOutcome::Delivered, $fcm->send($message, 'token-2'));

        self::assertCount(3, $http->requests);
        $oauth = $http->requests[0];
        self::assertSame('https://oauth2.googleapis.com/token', $oauth->url);
        parse_str($oauth->body, $form);
        self::assertSame('urn:ietf:params:oauth:grant-type:jwt-bearer', $form['grant_type']);
        self::assertIsString($form['assertion']);
        [$header, $claims, $signature] = explode('.', $form['assertion']);
        self::assertSame(1, openssl_verify("{$header}.{$claims}", self::base64UrlDecode($signature), self::publicKey(self::rsaKey()), OPENSSL_ALGO_SHA256));
        $claims = json_decode(self::base64UrlDecode($claims), true);
        self::assertIsArray($claims);
        self::assertSame('push@demo.iam.gserviceaccount.com', $claims['iss']);
        self::assertSame('https://www.googleapis.com/auth/firebase.messaging', $claims['scope']);
        self::assertSame($now + 3_600, $claims['exp']);

        $send = $http->requests[1];
        self::assertSame('https://fcm.googleapis.com/v1/projects/demo/messages:send', $send->url);
        self::assertSame('Bearer ya29.token', $send->headers['Authorization']);
        self::assertSame([
            'message' => [
                'token' => 'token-1',
                'notification' => ['title' => 'Hi', 'body' => 'There'],
                'data' => ['id' => '7', 'ok' => 'true'],
                'android' => ['priority' => 'HIGH', 'collapse_key' => 'inbox'],
                'apns' => ['payload' => ['aps' => ['badge' => 3]]],
            ],
        ], json_decode($send->body, true));
        self::assertSame('Bearer ya29.token', $http->requests[2]->headers['Authorization']);
    }

    public function testFcmMapsUnregisteredTokensAndRetryableErrors(): void
    {
        $http = new FakeHttpClient([
            new HttpResponse(200, [], '{"access_token":"t","expires_in":3599}'),
            new HttpResponse(404, [], '{"error":{"status":"NOT_FOUND","details":[{"errorCode":"UNREGISTERED"}]}}'),
            new HttpResponse(503, [], '{"error":{"status":"UNAVAILABLE"}}'),
            new HttpResponse(400, [], '{"error":{"status":"INVALID_ARGUMENT"}}'),
        ]);
        $fcm = new FcmProvider(self::serviceAccount(), $http);
        $message = PushMessage::make('a', 'b');

        self::assertSame(PushOutcome::InvalidToken, $fcm->send($message, 'gone'));
        foreach ([true, false] as $retryable) {
            try {
                $fcm->send($message, 'x');
                self::fail('Expected a delivery failure.');
            } catch (NotificationDeliveryException $failure) {
                self::assertSame($retryable, $failure->retryable);
            }
        }
    }

    public function testApnsSignsAnEs256ProviderTokenAndPostsOverHttp2(): void
    {
        $http = new FakeHttpClient([new HttpResponse(200), new HttpResponse(200)]);
        $now = 1_800_000_000;
        $apns = new ApnsProvider('TEAM123456', 'KEY1234567', self::ecKey(), 'com.example.app', false, $http, clock: static function () use (&$now): int {
            return $now;
        });
        $message = PushMessage::make('Hi', 'There')->sound()->ttl(60)->collapseKey('c')->data(['deep' => 'link']);

        self::assertSame(PushOutcome::Delivered, $apns->send($message, self::APNS_TOKEN));
        $now += 60;
        $apns->send($message, self::APNS_TOKEN);

        $request = $http->requests[0];
        self::assertTrue($request->http2);
        self::assertSame('https://api.sandbox.push.apple.com/3/device/' . self::APNS_TOKEN, $request->url);
        self::assertSame('com.example.app', $request->headers['apns-topic']);
        self::assertSame('alert', $request->headers['apns-push-type']);
        self::assertSame('10', $request->headers['apns-priority']);
        self::assertSame((string) ($now - 60 + 60), $request->headers['apns-expiration']);
        self::assertSame('c', $request->headers['apns-collapse-id']);
        self::assertSame(
            ['aps' => ['alert' => ['title' => 'Hi', 'body' => 'There'], 'sound' => 'default'], 'deep' => 'link'],
            json_decode($request->body, true),
        );
        self::assertSame($request->headers['authorization'], $http->requests[1]->headers['authorization'], 'token reused');
        $jwt = substr($request->headers['authorization'], 7);
        [$header, $claims, $signature] = explode('.', $jwt);
        self::assertSame(['alg' => 'ES256', 'typ' => 'JWT', 'kid' => 'KEY1234567'], json_decode(self::base64UrlDecode($header), true));
        self::assertSame(['iss' => 'TEAM123456', 'iat' => 1_800_000_000], json_decode(self::base64UrlDecode($claims), true));
        $raw = self::base64UrlDecode($signature);
        self::assertSame(64, strlen($raw));
        self::assertSame(1, openssl_verify("{$header}.{$claims}", self::rawToDer($raw), self::publicKey(self::ecKey()), OPENSSL_ALGO_SHA256));
    }

    public function testApnsMapsInvalidTokensAndExpiredProviderTokens(): void
    {
        $http = new FakeHttpClient([
            new HttpResponse(410, [], '{"reason":"Unregistered"}'),
            new HttpResponse(400, [], '{"reason":"BadDeviceToken"}'),
            new HttpResponse(403, [], '{"reason":"ExpiredProviderToken"}'),
            new HttpResponse(400, [], '{"reason":"PayloadTooLarge"}'),
        ]);
        $apns = new ApnsProvider('TEAM123456', 'KEY1234567', self::ecKey(), 'com.example.app', true, $http);
        $message = PushMessage::make('a')->silent();

        $outcomes = [];
        foreach ([self::APNS_TOKEN, self::APNS_TOKEN, 'not-hex'] as $token) {
            $outcomes[] = $apns->send($message, $token);
        }
        self::assertSame(array_fill(0, 3, PushOutcome::InvalidToken), $outcomes);
        foreach ([true, false] as $retryable) {
            try {
                $apns->send($message, self::APNS_TOKEN);
                self::fail('Expected a delivery failure.');
            } catch (NotificationDeliveryException $failure) {
                self::assertSame($retryable, $failure->retryable);
            }
        }
        self::assertSame('background', $http->requests[0]->headers['apns-push-type']);
        self::assertSame('5', $http->requests[0]->headers['apns-priority']);
        self::assertSame(['aps' => ['content-available' => 1]], json_decode($http->requests[0]->body, true));
    }

    public function testTheChannelFansOutToEveryDeviceAndReportsInvalidTokens(): void
    {
        $fcmHttp = new FakeHttpClient([
            new HttpResponse(200, [], '{"access_token":"t","expires_in":3599}'),
            new HttpResponse(200),
            new HttpResponse(404, [], '{"error":{"details":[{"errorCode":"UNREGISTERED"}]}}'),
        ]);
        $apnsHttp = new FakeHttpClient([new HttpTransportException('timeout')]);
        $invalid = [];
        $channel = new PushChannel(
            new FcmProvider(self::serviceAccount(), $fcmHttp),
            new ApnsProvider('TEAM123456', 'KEY1234567', self::ecKey(), 'com.example.app', true, $apnsHttp),
            static function (PushTarget $target) use (&$invalid): void {
                $invalid[] = $target->toString();
            },
        );
        $user = new User(1, devices: ['fcm:android-1', 'apns:' . self::APNS_TOKEN, 'fcm:android-old']);

        try {
            (new NotificationSender(['push' => $channel]))->send($user, new InvoicePaid(1, ['push']));
            self::fail('Expected the APNs timeout to fail the delivery.');
        } catch (NotificationDeliveryException $failure) {
            self::assertTrue($failure->retryable);
        }

        self::assertSame(['fcm:android-old'], $invalid);
        self::assertCount(3, $fcmHttp->requests);
        self::assertCount(1, $apnsHttp->requests);
    }

    public function testUsersWithoutDevicesAreSkipped(): void
    {
        $channel = new PushChannel(new FcmProvider(self::serviceAccount(), new FakeHttpClient()));

        self::assertNull($channel->prepare(new User(1), new InvoicePaid(1, ['push']), 'id'));
        $this->expectException(\InvalidArgumentException::class);
        PushTarget::parse('gcm:abc');
    }

    /** @return array<string, string> */
    private static function serviceAccount(): array
    {
        return [
            'type' => 'service_account',
            'project_id' => 'demo',
            'client_email' => 'push@demo.iam.gserviceaccount.com',
            'private_key' => self::rsaKey(),
            'token_uri' => 'https://oauth2.googleapis.com/token',
        ];
    }

    private static function rsaKey(): string
    {
        return self::$rsaKey ??= self::export(['private_key_type' => OPENSSL_KEYTYPE_RSA, 'private_key_bits' => 2048]);
    }

    private static function ecKey(): string
    {
        return self::$ecKey ??= self::export(['private_key_type' => OPENSSL_KEYTYPE_EC, 'curve_name' => 'prime256v1']);
    }

    /** @param array<string, int|string> $options */
    private static function export(array $options): string
    {
        $key = openssl_pkey_new($options);
        self::assertNotFalse($key);
        self::assertTrue(openssl_pkey_export($key, $pem));
        self::assertIsString($pem);
        return $pem;
    }

    private static function publicKey(string $privatePem): string
    {
        $key = openssl_pkey_get_private($privatePem);
        self::assertNotFalse($key);
        $details = openssl_pkey_get_details($key);
        self::assertIsArray($details);
        self::assertIsString($details['key']);
        return $details['key'];
    }

    private static function base64UrlDecode(string $value): string
    {
        return (string) base64_decode(strtr($value, '-_', '+/'), true);
    }

    private static function rawToDer(string $raw): string
    {
        $integer = static function (string $bytes): string {
            $bytes = ltrim($bytes, "\0");
            if ($bytes === '' || ord($bytes[0]) & 0x80) {
                $bytes = "\0" . $bytes;
            }
            return "\x02" . chr(strlen($bytes)) . $bytes;
        };
        $sequence = $integer(substr($raw, 0, 32)) . $integer(substr($raw, 32));
        return "\x30" . chr(strlen($sequence)) . $sequence;
    }
}
