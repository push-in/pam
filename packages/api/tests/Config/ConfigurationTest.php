<?php

declare(strict_types=1);

namespace Pam\Api\Tests\Config;

use Pam\Api\Config\ConfigDefinition;
use Pam\Api\Config\ConfigType;
use Pam\Api\Config\Configuration;
use Pam\Api\Config\ConfigurationException;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(ConfigDefinition::class)]
#[CoversClass(ConfigType::class)]
#[CoversClass(Configuration::class)]
#[CoversClass(ConfigurationException::class)]
final class ConfigurationTest extends TestCase
{
    public function testItParsesTypedValuesAndRedactsSecrets(): void
    {
        $configuration = Configuration::fromArray([
            new ConfigDefinition('app.port', 'APP_PORT', ConfigType::Integer),
            new ConfigDefinition('app.debug', 'APP_DEBUG', ConfigType::Boolean),
            new ConfigDefinition('auth.secret', 'AUTH_SECRET', sensitive: true),
            new ConfigDefinition('app.ratio', 'APP_RATIO', ConfigType::Float, required: false, default: 0.5),
        ], [
            'APP_PORT' => '3000',
            'APP_DEBUG' => 'false',
            'AUTH_SECRET' => 'never-print-this',
        ]);

        self::assertSame(3000, $configuration->integer('app.port'));
        self::assertFalse($configuration->boolean('app.debug'));
        self::assertSame(0.5, $configuration->float('app.ratio'));
        self::assertSame('[REDACTED]', $configuration->redacted()['auth.secret']);
        self::assertStringNotContainsString('never-print-this', json_encode($configuration->redacted(), JSON_THROW_ON_ERROR));
    }

    public function testItReportsAllInvalidVariablesWithoutTheirValues(): void
    {
        try {
            Configuration::fromArray([
                new ConfigDefinition('app.port', 'APP_PORT', ConfigType::Integer),
                new ConfigDefinition('auth.secret', 'AUTH_SECRET', sensitive: true),
            ], ['APP_PORT' => 'not-a-number', 'AUTH_SECRET' => '']);
            self::fail('Invalid configuration should fail during boot.');
        } catch (ConfigurationException $error) {
            self::assertCount(2, $error->errors);
            self::assertStringContainsString('APP_PORT', $error->getMessage());
            self::assertStringContainsString('AUTH_SECRET', $error->getMessage());
            self::assertStringNotContainsString('not-a-number', $error->getMessage());
        }
    }

    public function testDefaultsMustMatchDeclaredTypes(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        new ConfigDefinition('app.port', 'APP_PORT', ConfigType::Integer, required: false, default: '3000');
    }
}
