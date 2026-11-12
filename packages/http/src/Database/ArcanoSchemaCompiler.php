<?php

declare(strict_types=1);

namespace Pam\Api\Database;

use Arcano\Annotated\Embeddings;
use Arcano\Annotated\Entities;
use Arcano\Annotated\Locator\TokenizerEmbeddingLocator;
use Arcano\Annotated\Locator\TokenizerEntityLocator;
use Arcano\Annotated\MergeColumns;
use Arcano\Annotated\MergeIndexes;
use Arcano\Annotated\TableInheritance;
use Arcano\Database\DatabaseProviderInterface;
use Arcano\Schema\Compiler;
use Arcano\Schema\Generator\ForeignKeys;
use Arcano\Schema\Generator\GenerateModifiers;
use Arcano\Schema\Generator\GenerateRelations;
use Arcano\Schema\Generator\GenerateTypecast;
use Arcano\Schema\Generator\RenderModifiers;
use Arcano\Schema\Generator\RenderRelations;
use Arcano\Schema\Generator\RenderTables;
use Arcano\Schema\Generator\ResetTables;
use Arcano\Schema\Generator\ValidateEntities;
use Arcano\Schema\GeneratorInterface;
use Arcano\Schema\Registry;
use Spiral\Attributes\AttributeReader;
use Spiral\Tokenizer\Config\TokenizerConfig;
use Spiral\Tokenizer\Tokenizer;

/**
 * Builds the Arcano schema from #[Entity] classes and keeps it in a PHP cache file.
 *
 * With $refresh the cache is rebuilt whenever a PHP file in the entity directories is
 * newer than it; production deployments compile once and pass $refresh = false.
 */
final readonly class ArcanoSchemaCompiler
{
    /** @var non-empty-list<string> */
    private array $directories;

    /** @param non-empty-list<string>|string $directories */
    public function __construct(
        array|string $directories,
        private ?string $cacheFile = null,
        private bool $refresh = true,
    ) {
        $directories = is_string($directories) ? [$directories] : $directories;
        foreach ($directories as $directory) {
            if (!is_dir($directory)) {
                throw new \InvalidArgumentException("Entity directory {$directory} does not exist.");
            }
        }
        $this->directories = $directories;
    }

    /**
     * The cached schema, compiled and written first when missing or stale.
     *
     * @return array<string, array<int, mixed>>
     */
    public function schema(DatabaseProviderInterface $databases): array
    {
        if ($this->cacheFile !== null && is_file($this->cacheFile) && !($this->refresh && $this->stale($this->cacheFile))) {
            return self::load($this->cacheFile);
        }
        $schema = $this->compile($databases);
        if ($this->cacheFile !== null) {
            $this->write($this->cacheFile, $schema);
        }
        return $schema;
    }

    /**
     * Compiles the entities, reading the current tables of the database.
     *
     * @param list<GeneratorInterface> $generators Run after the schema generators (e.g. migrations).
     * @return array<string, array<int, mixed>>
     */
    public function compile(DatabaseProviderInterface $databases, array $generators = []): array
    {
        $classes = (new Tokenizer(new TokenizerConfig([
            'directories' => $this->directories,
            'exclude' => [],
        ])))->classLocator();
        $reader = new AttributeReader();

        /** @var array<string, array<int, mixed>> */
        return (new Compiler())->compile(new Registry($databases), [
            new ResetTables(),
            new Embeddings(new TokenizerEmbeddingLocator($classes, $reader), $reader),
            new Entities(new TokenizerEntityLocator($classes, $reader), $reader),
            new TableInheritance($reader),
            new MergeColumns($reader),
            new GenerateRelations(),
            new GenerateModifiers(),
            new ValidateEntities(),
            new RenderTables(),
            new RenderRelations(),
            new RenderModifiers(),
            new ForeignKeys(),
            new MergeIndexes($reader),
            new GenerateTypecast(),
            ...$generators,
        ]);
    }

    public function clear(): void
    {
        if ($this->cacheFile !== null && is_file($this->cacheFile)) {
            unlink($this->cacheFile);
        }
    }

    private function stale(string $cacheFile): bool
    {
        $compiledAt = filemtime($cacheFile);
        foreach ($this->directories as $directory) {
            $files = new \RecursiveIteratorIterator(new \RecursiveDirectoryIterator($directory, \FilesystemIterator::SKIP_DOTS));
            foreach ($files as $file) {
                if ($file instanceof \SplFileInfo && $file->getExtension() === 'php' && $file->getMTime() > $compiledAt) {
                    return true;
                }
            }
        }
        return false;
    }

    /** @return array<string, array<int, mixed>> */
    private static function load(string $cacheFile): array
    {
        $schema = require $cacheFile;
        if (!is_array($schema)) {
            throw new \UnexpectedValueException("The Arcano schema cache {$cacheFile} does not return an array.");
        }
        /** @var array<string, array<int, mixed>> $schema */
        return $schema;
    }

    /** @param array<string, array<int, mixed>> $schema */
    private function write(string $cacheFile, array $schema): void
    {
        $directory = dirname($cacheFile);
        if (!is_dir($directory) && !mkdir($directory, 0775, true) && !is_dir($directory)) {
            throw new \RuntimeException("Unable to create {$directory}.");
        }
        $temporary = tempnam($directory, '.arcano-schema');
        if ($temporary === false) {
            throw new \RuntimeException("Unable to write to {$directory}.");
        }
        file_put_contents($temporary, "<?php\n\nreturn " . var_export($schema, true) . ";\n");
        chmod($temporary, 0644);
        if (!rename($temporary, $cacheFile)) {
            @unlink($temporary);
            throw new \RuntimeException("Unable to write {$cacheFile}.");
        }
        if (function_exists('opcache_invalidate')) {
            opcache_invalidate($cacheFile, true);
        }
    }
}
