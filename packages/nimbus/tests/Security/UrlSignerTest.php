<?php

declare(strict_types=1);

namespace Nimbus\Tests\Security;

use Nimbus\App;
use Nimbus\Http\ProblemCode;
use Nimbus\Middleware\ValidateSignatureMiddleware;
use Nimbus\Security\UrlSignatureState;
use Nimbus\Security\UrlSigner;
use Nimbus\Testing\TestClient;
use Pam\Http\Request;
use Pam\Http\Response;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(UrlSigner::class)]
#[CoversClass(ValidateSignatureMiddleware::class)]
final class UrlSignerTest extends TestCase
{
    public function testSignedUrlsVerifyAndExpire(): void
    {
        $signer = new UrlSigner(str_repeat('k', 32));
        [$path, $query] = self::split($signer->sign('/files', ['id' => 7], 1_000));

        self::assertSame('/files', $path);
        self::assertSame(UrlSignatureState::Valid, $signer->check($path, $query, 999));
        self::assertSame(UrlSignatureState::Expired, $signer->check($path, $query, 1_001));
        self::assertTrue($signer->verify(...self::split($signer->sign('/files', ['id' => 7]))));
    }

    public function testTamperedUrlsAreRejected(): void
    {
        $signer = new UrlSigner(str_repeat('k', 32));
        [$path, $query] = self::split($signer->temporary('/files', 60, ['id' => 7]));

        self::assertSame(UrlSignatureState::Invalid, $signer->check('/other', $query));
        self::assertSame(UrlSignatureState::Invalid, $signer->check($path, ['id' => '8'] + $query));
        self::assertSame(UrlSignatureState::Invalid, $signer->check($path, ['expires' => (string) (time() + 9_999)] + $query));
        self::assertSame(UrlSignatureState::Invalid, $signer->check($path, ['extra' => '1'] + $query));
        self::assertSame(UrlSignatureState::Invalid, $signer->check($path, ['id' => '7']));
        self::assertSame(UrlSignatureState::Invalid, (new UrlSigner(str_repeat('x', 32)))->check($path, $query));
    }

    public function testShortKeysAreRejected(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        new UrlSigner('short');
    }

    public function testSignedRoutesAnswerProblemDetails(): void
    {
        $signer = new UrlSigner(str_repeat('k', 32));
        $app = new App(discoverPackages: false);
        $app->container()->instance(UrlSigner::class, $signer);
        $app->get('/reports', static fn (Request $request, Response $response): Response => $response->json(['ok' => true]))
            ->signed();
        $client = new TestClient($app);

        [$path, $query] = self::split($signer->temporary('/reports', 60));
        $client->request('GET', $path, $query)->assertStatus(200)->assertJsonPath('ok', true);
        $client->request('GET', $path, ['signature' => 'forged'] + $query)
            ->assertStatus(403)
            ->assertJsonPath('code', ProblemCode::InvalidSignature->value)
            ->assertJsonPath('title', 'The URL signature is invalid.');
        [$path, $query] = self::split($signer->sign('/reports', [], time() - 1));
        $client->request('GET', $path, $query)
            ->assertStatus(403)
            ->assertJsonPath('title', 'The signed URL has expired.');
    }

    /** @return array{string, array<string, mixed>} */
    private static function split(string $url): array
    {
        $path = parse_url($url, PHP_URL_PATH);
        parse_str((string) parse_url($url, PHP_URL_QUERY), $query);
        self::assertIsString($path);
        /** @var array<string, mixed> $query */
        return [$path, $query];
    }
}
