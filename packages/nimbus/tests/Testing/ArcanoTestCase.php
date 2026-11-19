<?php

declare(strict_types=1);

namespace Nimbus\Tests\Testing;

use Nimbus\App;
use Nimbus\Database\ArcanoDatabaseProvider;
use Nimbus\Database\ArcanoManager;
use Nimbus\Database\ArcanoSchemaCompiler;
use Nimbus\Database\ArcanoServiceProvider;
use Nimbus\Testing\RefreshDatabase;
use Nimbus\Testing\TestCase;
use Nimbus\Tests\Database\ArcanoTestDatabase;
use Nimbus\Tests\Fixtures\Arcano\Author;
use Nimbus\Tests\Fixtures\Arcano\Post;
use Pam\Http\Request;
use Pam\Http\Response;

/**
 * An application over the Author and Post fixtures, migrated and rolled back per test.
 * The SQLite file outlives the tests of a class, so rollbacks are observable.
 */
abstract class ArcanoTestCase extends TestCase
{
    use RefreshDatabase;

    private static ?string $path = null;

    public static function setUpBeforeClass(): void
    {
        self::$path = tempnam(sys_get_temp_dir(), 'nimbus-testing') ?: null;
        if (ArcanoTestDatabase::driver() !== 'sqlite' && class_exists(\Arcano\ORM\ORM::class)) {
            // A server database is shared with other suites that create the same tables.
            $databases = new ArcanoDatabaseProvider(ArcanoTestDatabase::config(''));
            foreach (['posts', 'authors', 'migrations'] as $table) {
                $databases->database()->execute('DROP TABLE IF EXISTS ' . $table);
            }
            $databases->releaseCurrent();
            $databases->disconnectIdle();
            self::forgetMigrations();
        }
    }

    public static function tearDownAfterClass(): void
    {
        if (self::$path !== null) {
            @unlink(self::$path);
            self::$path = null;
        }
    }

    protected function setUp(): void
    {
        if (!class_exists(\Arcano\ORM\ORM::class)) {
            self::markTestSkipped('pushinbr/arcano is not installed.');
        }
        parent::setUp();
    }

    protected function createApplication(): App
    {
        $app = new App(discoverPackages: false);
        $app->provider(new ArcanoServiceProvider(
            ArcanoTestDatabase::config(self::$path ?? ':memory:'),
            new ArcanoSchemaCompiler(dirname(__DIR__) . '/Fixtures/Arcano'),
            migrations: dirname(__DIR__) . '/Fixtures/migrations',
        ));
        $app->get('/authors', static fn (Request $request, Response $response, ArcanoManager $arcano): Response => $response->json([
            'data' => array_map(
                static fn (object $author): array => ['name' => $author instanceof Author ? $author->name : null],
                $arcano->select(Author::class)->orderBy('id')->fetchAll(),
            ),
        ]));
        $app->post('/authors', static function (Request $request, Response $response, ArcanoManager $arcano): Response {
            $input = $request->json();
            $author = new Author();
            $author->name = is_array($input) && is_string($input['name'] ?? null) ? $input['name'] : '';
            $arcano->entityManager()->persist($author)->run();
            return $response->json(['data' => ['id' => $author->id, 'name' => $author->name]], 201);
        });
        $app->get('/posts', static fn (Request $request, Response $response, ArcanoManager $arcano): Response => $response->json([
            'data' => array_map(
                static fn (object $post): array => $post instanceof Post
                    ? ['title' => $post->title, 'author' => $post->author?->name]
                    : [],
                $arcano->select(Post::class)->load('author')->orderBy('id')->fetchAll(),
            ),
        ]));
        return $app;
    }
}
