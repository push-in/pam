<?php

declare(strict_types=1);

namespace Pam\Api\Tests;

use Pam\App;
use Pam\Api\Testing\TestClient;
use PHPUnit\Framework\TestCase;

final class UpgradeFromV1Test extends TestCase
{
    public function testPublishedV1ApplicationRunsUnchangedOnV2(): void
    {
        $fixture = __DIR__ . '/fixtures/v1.0.2';
        $source = self::sourceEvidence($fixture . '/source.json');

        self::assertSame(1, $source['schemaVersion']);
        self::assertSame('pushinbr/pam-http', $source['package']);
        self::assertSame('v1.0.2', $source['tag']);
        self::assertSame(
            '2b99c19782ff3c88e9b7484aa13a5960088fcccb',
            $source['sourceCommit'],
        );

        $app = require $fixture . '/application.php';
        self::assertInstanceOf(App::class, $app);

        (new TestClient($app))
            ->get('/users/legacy%20consumer', ['origin' => 'https://app.example'])
            ->assertSuccessful()
            ->assertHeader('access-control-allow-origin', 'https://app.example')
            ->assertHeader('x-content-type-options', 'nosniff')
            ->assertJson(['id' => 'legacy consumer']);
        self::addToAssertionCount(4);
    }

    /**
     * @return array{
     *     schemaVersion: int,
     *     package: string,
     *     tag: string,
     *     sourceCommit: string
     * }
     */
    private static function sourceEvidence(string $path): array
    {
        $source = json_decode(
            (string) file_get_contents($path),
            true,
            16,
            JSON_THROW_ON_ERROR,
        );
        if (
            !is_array($source)
            || !isset($source['schemaVersion'])
            || !is_int($source['schemaVersion'])
            || !isset($source['package'])
            || !is_string($source['package'])
            || !isset($source['tag'])
            || !is_string($source['tag'])
            || !isset($source['sourceCommit'])
            || !is_string($source['sourceCommit'])
        ) {
            throw new \UnexpectedValueException('PAM API v1 upgrade source evidence is invalid.');
        }

        return [
            'schemaVersion' => $source['schemaVersion'],
            'package' => $source['package'],
            'tag' => $source['tag'],
            'sourceCommit' => $source['sourceCommit'],
        ];
    }
}
