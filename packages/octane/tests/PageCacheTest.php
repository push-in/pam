<?php

declare(strict_types=1);

namespace Pam\Octane\Tests;

use Illuminate\Contracts\Config\Repository;
use Illuminate\Database\Connection;
use Illuminate\Database\DatabaseManager;
use Illuminate\Routing\Router;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Http\Request;
use Illuminate\Http\Response;
use Illuminate\Routing\Route;
use Orchestra\Testbench\TestCase;
use Pam\Octane\Cache\PageCache;
use Pam\Octane\Cache\PurgesPageCache;
use Pam\Octane\Facades\Pam;
use Pam\Octane\Http\Middleware\CacheResponse;
use Pam\Octane\PamOctaneServiceProvider;

final class PageCacheTest extends TestCase
{
    private string $log;

    protected function setUp(): void
    {
        $_SERVER['PHP_SELF'] = 'artisan';
        $this->log = sys_get_temp_dir().'/pam-octane-page-cache-'.getmypid().'/nested/log';
        @unlink($this->log);

        parent::setUp();
    }

    protected function tearDown(): void
    {
        @unlink($this->log);
        @rmdir(dirname($this->log));
        @rmdir(dirname($this->log, 2));
        parent::tearDown();
    }

    /** @return list<class-string> */
    protected function getPackageProviders($app): array
    {
        return [PamOctaneServiceProvider::class];
    }

    protected function defineEnvironment($app): void
    {
        $config = $app->make('config');
        self::assertInstanceOf(Repository::class, $config);
        $config->set('pam-octane.server.responseCacheInvalidationLog', $this->log);
        $config->set('database.default', 'testing');
        $config->set('database.connections.testing', [
            'driver' => 'sqlite',
            'database' => ':memory:',
        ]);
    }

    private function pageCache(): PageCache
    {
        $cache = $this->app?->make(PageCache::class);
        self::assertInstanceOf(PageCache::class, $cache);

        return $cache;
    }

    private function connection(): Connection
    {
        $database = $this->app?->make('db');
        self::assertInstanceOf(DatabaseManager::class, $database);
        $connection = $database->connection();
        self::assertInstanceOf(Connection::class, $connection);

        return $connection;
    }

    public function testFacadePublishesTagUrlAndAllPurgesToTheInvalidationLog(): void
    {
        self::assertNull(Pam::purge('products', ['product:42', 'products']));
        self::assertNull(Pam::purgeUrl('/products/*', 'https://example.com/a?b=1'));
        self::assertNull(Pam::purgeAll());
        self::assertSame(0, Pam::purge());
        self::assertSame(
            "products\nproduct:42\n!url /products/*\n!url https://example.com/a?b=1\n*\n",
            file_get_contents($this->log),
        );
        self::assertSame(0600, fileperms($this->log) & 0777);
    }

    public function testInvalidTagsAndUrlsAreRejected(): void
    {
        $cache = $this->pageCache();
        foreach ([['bad tag'], [str_repeat('a', 129)], ["a\nb"], ['']] as $tags) {
            try {
                $cache->purge($tags);
                self::fail('accepted '.json_encode($tags));
            } catch (\InvalidArgumentException) {
            }
        }
        foreach (['relative', "/a\n*", 'ftp://x/y', '', '/a b', "/caf\u{e9}"] as $url) {
            try {
                $cache->purgeUrl($url);
                self::fail("accepted {$url}");
            } catch (\InvalidArgumentException) {
            }
        }
        self::assertFileDoesNotExist($this->log);
    }

    public function testTheLogIsRotatedWhenLarge(): void
    {
        $cache = new PageCache($this->log);
        for ($index = 0; $index < 9_000; $index++) {
            $cache->purge('tag-'.$index.'-'.str_repeat('x', 100));
        }
        self::assertLessThan(1024 * 1024, filesize($this->log));
    }

    public function testWithoutALogOutsideAWorkerThePurgeFails(): void
    {
        $this->expectException(\RuntimeException::class);
        (new PageCache(null))->purge('products');
    }

    public function testMiddlewareMarksCacheableResponsesAndResolvesRouteTags(): void
    {
        $product = new PageCacheProduct();
        $product->id = 42;
        $request = Request::create('/products/42');
        $route = new Route('GET', '/products/{product}', []);
        $route->bind($request);
        $route->setParameter('product', $product);
        $request->setRouteResolver(static fn (): Route => $route);

        $middleware = new CacheResponse();
        $response = $middleware->handle($request, static fn (): Response => new Response('ok'), '600', '60', 'products', 'product:{product}', 'bad tag');
        self::assertInstanceOf(Response::class, $response);
        self::assertSame('max-age=0, public, s-maxage=600, stale-while-revalidate=60', $response->headers->get('Cache-Control'));
        self::assertSame('products,product:42', $response->headers->get('Pam-Cache-Tags'));

        self::assertSame(
            CacheResponse::class.':600,60,products,product:{product}',
            CacheResponse::for(ttl: 600, staleWhileRevalidate: 60, tags: ['products', 'product:{product}']),
        );
    }

    public function testMiddlewareLeavesPersonalOrUncacheableResponsesAlone(): void
    {
        $middleware = new CacheResponse();
        $cases = [
            'cookie' => [Request::create('/'), static fn (): Response => (new Response('x'))->withCookie(cookie('session', 'abc'))],
            'raw set-cookie' => [Request::create('/'), static function (): Response {
                $response = new Response('x');
                $response->headers->set('Set-Cookie', 'a=b');

                return $response;
            }],
            'post' => [Request::create('/', 'POST'), static fn (): Response => new Response('x')],
            'server error' => [Request::create('/'), static fn (): Response => new Response('x', 500)],
            'redirect' => [Request::create('/'), static fn (): Response => new Response('x', 302)],
            'private' => [Request::create('/'), static fn (): Response => new Response('x', 200, ['Cache-Control' => 'private, max-age=60'])],
            'no-store' => [Request::create('/'), static fn (): Response => new Response('x', 200, ['Cache-Control' => 'no-store'])],
            'explicit no-cache' => [Request::create('/'), static fn (): Response => new Response('x', 200, ['Cache-Control' => 'no-cache, private'])],
        ];
        foreach ($cases as $name => [$request, $next]) {
            $response = $middleware->handle($request, $next, '600');
            self::assertInstanceOf(Response::class, $response);
            self::assertStringNotContainsString('s-maxage', (string) $response->headers->get('Cache-Control'), $name);
            self::assertFalse($response->headers->has('Pam-Cache-Tags'), $name);
        }
        $disabled = $middleware->handle(Request::create('/'), static fn (): Response => new Response('x'), '0');
        self::assertInstanceOf(Response::class, $disabled);
        self::assertStringNotContainsString('s-maxage', (string) $disabled->headers->get('Cache-Control'));
    }

    public function testTheMiddlewareAliasIsRegistered(): void
    {
        $router = $this->app?->make('router');
        self::assertInstanceOf(Router::class, $router);
        self::assertSame(CacheResponse::class, $router->getMiddleware()['pam.cache'] ?? null);
    }

    public function testModelChangesPurgeTheirTagsAfterCommit(): void
    {
        $connection = $this->connection();
        $schema = $connection->getSchemaBuilder();
        $schema->create('page_cache_products', static function ($table): void {
            $table->id();
            $table->string('name');
        });

        $product = PageCacheProduct::query()->create(['name' => 'a']);
        self::assertSame("page_cache_products\npage_cache_products:1\n", file_get_contents($this->log));

        $connection->transaction(function () use ($product): void {
            $product->update(['name' => 'b']);
            self::assertSame("page_cache_products\npage_cache_products:1\n", file_get_contents($this->log), 'not before commit');
        });
        self::assertSame(2, substr_count((string) file_get_contents($this->log), "page_cache_products:1\n"));

        try {
            $connection->transaction(static function () use ($product): void {
                $product->update(['name' => 'c']);
                throw new \RuntimeException('rollback');
            });
        } catch (\RuntimeException) {
        }
        self::assertSame(2, substr_count((string) file_get_contents($this->log), "page_cache_products:1\n"), 'rolled back changes purge nothing');

        $product->delete();
        self::assertSame(3, substr_count((string) file_get_contents($this->log), "page_cache_products:1\n"));
    }
}

/** @property int $id */
final class PageCacheProduct extends Model
{
    use PurgesPageCache;

    public $timestamps = false;

    protected $table = 'page_cache_products';

    protected $guarded = [];
}
