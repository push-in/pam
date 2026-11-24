<?php

declare(strict_types=1);

namespace Nimbus\Tests\PHPStan;

use Nimbus\PHPStan\ArcanoManagerReturnTypeExtension;
use Nimbus\PHPStan\ContainerGetReturnTypeExtension;
use Nimbus\PHPStan\FormRequestReturnTypeExtension;
use PHPStan\Testing\TypeInferenceTestCase;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\Attributes\DataProvider;

#[CoversClass(ContainerGetReturnTypeExtension::class)]
#[CoversClass(ArcanoManagerReturnTypeExtension::class)]
#[CoversClass(FormRequestReturnTypeExtension::class)]
final class NimbusExtensionTypesTest extends TypeInferenceTestCase
{
    /** @return iterable<mixed> */
    public static function dataFileAsserts(): iterable
    {
        yield from self::gatherAssertTypes(__DIR__ . '/data/types.php');
    }

    #[DataProvider('dataFileAsserts')]
    public function testFileAsserts(string $assertType, string $file, mixed ...$args): void
    {
        $this->assertFileAsserts($assertType, $file, ...$args);
    }

    public static function getAdditionalConfigFiles(): array
    {
        return [dirname(__DIR__, 2) . '/phpstan/extension.neon'];
    }
}
