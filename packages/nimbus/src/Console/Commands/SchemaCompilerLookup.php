<?php

declare(strict_types=1);

namespace Nimbus\Console\Commands;

use Nimbus\App;
use Nimbus\Database\ArcanoDatabaseProvider;
use Nimbus\Database\ArcanoSchemaCompiler;

/** @internal */
final class SchemaCompilerLookup
{
    /** @return array{ArcanoSchemaCompiler, ArcanoDatabaseProvider} */
    public static function find(App $app): array
    {
        $container = $app->container();
        $compiler = $container->has(ArcanoSchemaCompiler::class) ? $container->get(ArcanoSchemaCompiler::class) : null;
        $databases = $container->has(ArcanoDatabaseProvider::class) ? $container->get(ArcanoDatabaseProvider::class) : null;
        if (!$compiler instanceof ArcanoSchemaCompiler || !$databases instanceof ArcanoDatabaseProvider) {
            throw new \RuntimeException(
                'No schema compiler: register ArcanoServiceProvider with an ArcanoSchemaCompiler in bootstrap/app.php.',
            );
        }
        return [$compiler, $databases];
    }
}
