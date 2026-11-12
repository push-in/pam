<?php

declare(strict_types=1);

namespace Pam\Api\Tests\Database;

use Arcano\ORM\SchemaInterface;
use Arcano\Schema\Compiler;
use Pam\Api\Database\ArcanoDatabaseProvider;
use Pam\Api\Database\ArcanoManager;
use Pam\Api\Database\ArcanoMigrationManager;
use Pam\Api\Database\ArcanoSchemaCompiler;
use Pam\Api\Database\ArcanoServiceProvider;
use Pam\Api\Tests\Fixtures\Arcano\Author;
use Pam\Api\Tests\Fixtures\Arcano\Post;
use Pam\App;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(ArcanoSchemaCompiler::class)]
#[CoversClass(ArcanoMigrationManager::class)]
#[CoversClass(ArcanoServiceProvider::class)]
final class ArcanoSchemaTest extends TestCase
{
    private const ENTITIES = __DIR__ . '/../Fixtures/Arcano';

    private string $directory = '';

    protected function setUp(): void
    {
        if (!class_exists(Compiler::class)) {
            self::markTestSkipped('pushinbr/arcano is not installed.');
        }
        $this->directory = sys_get_temp_dir() . '/arcano-schema-' . bin2hex(random_bytes(6));
        mkdir($this->directory . '/migrations', 0775, true);
        $database = $this->databases()->database();
        foreach (['posts', 'authors', 'migrations'] as $table) {
            $database->execute('DROP TABLE IF EXISTS ' . $table);
        }
    }

    protected function tearDown(): void
    {
        if ($this->directory === '' || !is_dir($this->directory)) {
            return;
        }
        $files = new \RecursiveIteratorIterator(
            new \RecursiveDirectoryIterator($this->directory, \FilesystemIterator::SKIP_DOTS),
            \RecursiveIteratorIterator::CHILD_FIRST,
        );
        foreach ($files as $file) {
            if ($file instanceof \SplFileInfo) {
                $file->isDir() ? rmdir($file->getPathname()) : unlink($file->getPathname());
            }
        }
        rmdir($this->directory);
    }

    public function testItCompilesAttributeEntitiesIntoACachedSchema(): void
    {
        $cache = $this->directory . '/cache/schema.php';
        $databases = $this->databases();
        $schema = (new ArcanoSchemaCompiler(self::ENTITIES, $cache, refresh: false))->schema($databases->manager());

        self::assertSame(Author::class, $schema['author'][SchemaInterface::ENTITY]);
        self::assertSame('authors', $schema['author'][SchemaInterface::TABLE]);
        self::assertSame('posts', $schema['post'][SchemaInterface::TABLE]);
        $authorRelations = $schema['author'][SchemaInterface::RELATIONS];
        $postRelations = $schema['post'][SchemaInterface::RELATIONS];
        self::assertIsArray($authorRelations);
        self::assertIsArray($postRelations);
        self::assertArrayHasKey('posts', $authorRelations);
        self::assertArrayHasKey('author', $postRelations);
        self::assertFileExists($cache);
        self::assertSame($schema, require $cache);

        // Without refresh the cache is trusted as it is.
        file_put_contents($cache, "<?php\n\nreturn ['cached' => []];\n");
        self::assertSame(['cached' => []], (new ArcanoSchemaCompiler(self::ENTITIES, $cache, refresh: false))->schema($databases->manager()));

        // With refresh, entity files newer than the cache rebuild it.
        touch($cache, 1);
        clearstatcache();
        $rebuilt = (new ArcanoSchemaCompiler(self::ENTITIES, $cache))->schema($databases->manager());
        self::assertSame($schema, $rebuilt);
        $databases->releaseCurrent();
    }

    public function testMigrationsAreGeneratedFromEntitiesRunAndRolledBack(): void
    {
        $databases = $this->databases();
        $migrations = new ArcanoMigrationManager(
            $databases,
            $this->directory . '/migrations',
            entities: new ArcanoSchemaCompiler(self::ENTITIES),
        );

        $files = $migrations->generate();
        self::assertCount(1, $files);
        self::assertStringContainsString("table('authors')", (string) file_get_contents($files[0]));

        $executed = $migrations->migrate();
        self::assertCount(1, $executed);
        self::assertTrue($databases->database()->hasTable('authors'));
        self::assertTrue($databases->database()->hasTable('posts'));
        self::assertNotNull($migrations->status()[0]['executed_at']);
        self::assertSame([], $migrations->generate(), 'The database matches the entities.');
        self::assertSame([], $migrations->migrate());

        self::assertSame($executed, $migrations->rollback());
        self::assertFalse($databases->database()->hasTable('authors'));
        self::assertNull($migrations->status()[0]['executed_at']);
        $databases->releaseCurrent();
    }

    public function testGeneratingWithPendingMigrationsFails(): void
    {
        $migrations = new ArcanoMigrationManager(
            $this->databases(),
            $this->directory . '/migrations',
            entities: new ArcanoSchemaCompiler(self::ENTITIES),
        );
        $migrations->generate();

        $this->expectException(\LogicException::class);
        $this->expectExceptionMessage('Run the pending migration');
        $migrations->generate();
    }

    public function testTheServiceProviderServesEntitiesAndMigrations(): void
    {
        $app = new App(discoverPackages: false);
        $app->provider(new ArcanoServiceProvider(
            ArcanoTestDatabase::config($this->directory . '/app.sqlite'),
            new ArcanoSchemaCompiler(self::ENTITIES, $this->directory . '/cache/schema.php'),
            $this->directory . '/migrations',
        ));
        $app->boot();
        $migrations = $app->container()->get(ArcanoMigrationManager::class);
        $arcano = $app->container()->get(ArcanoManager::class);
        self::assertInstanceOf(ArcanoMigrationManager::class, $migrations);
        self::assertInstanceOf(ArcanoManager::class, $arcano);
        $migrations->generate();
        $migrations->migrate();

        $author = new Author();
        $author->name = 'Ada';
        foreach (['First', 'Second'] as $title) {
            $post = new Post();
            $post->title = $title;
            $author->posts[] = $post;
        }
        $arcano->entityManager()->persist($author)->run();
        $arcano->releaseCurrentRequest();
        self::assertNotNull($author->id);

        $loaded = $arcano->select(Author::class)->load('posts')->wherePK($author->id)->fetchOne();
        self::assertInstanceOf(Author::class, $loaded);
        self::assertNotSame($author, $loaded);
        self::assertSame(['First', 'Second'], array_map(static fn (Post $post): string => $post->title, $loaded->posts));
        $post = $arcano->repository(Post::class)->findOne(['title' => 'Second']);
        self::assertInstanceOf(Post::class, $post);
        self::assertSame($loaded, $post->author);
        $arcano->releaseCurrentRequest();
    }

    private function databases(): ArcanoDatabaseProvider
    {
        return new ArcanoDatabaseProvider(ArcanoTestDatabase::config($this->directory . '/app.sqlite'));
    }
}
