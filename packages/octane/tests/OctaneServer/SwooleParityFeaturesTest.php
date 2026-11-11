<?php

declare(strict_types=1);

namespace Pam\Octane\Tests\OctaneServer;

use Illuminate\Support\Facades\Cache;
use Laravel\Octane\Cache\OctaneStore;
use Laravel\Octane\Facades\Octane;
use Pam\Octane\OctaneManager;
use Pam\Octane\PamServer;
use Pam\Octane\Tables\PamTable;
use Pam\Octane\Tables\PamTables;

/**
 * The Swoole-only Octane APIs inside a PAM worker, through stock Octane:
 * Octane::table(), the shared `octane` cache store and Octane::concurrently().
 * (Not process-isolated: PAM 2.2.15 cannot run PHPUnit 12's isolated
 * children. `Swoole\Table` stays an alias of PamTable afterwards, which only
 * matters to code that runs on PAM.)
 */
final class SwooleParityFeaturesTest extends OctaneServerTestCase
{
    protected function setUp(): void
    {
        if (!PamTables::available()) {
            self::markTestSkipped('Requires the PAM runtime (run the suite with `pam test`).');
        }

        // What PamServer does before it boots the worker application.
        (new \ReflectionProperty(PamServer::class, 'serving'))->setValue(null, true);
        // Read while the providers register, before defineEnvironment().
        putenv('PAM_OCTANE_SHARED_CACHE=1');
        $_ENV['PAM_OCTANE_SHARED_CACHE'] = $_SERVER['PAM_OCTANE_SHARED_CACHE'] = '1';

        parent::setUp();
    }

    protected function tearDown(): void
    {
        (new \ReflectionProperty(PamServer::class, 'serving'))->setValue(null, false);
        putenv('PAM_OCTANE_SHARED_CACHE');
        unset($_ENV['PAM_OCTANE_SHARED_CACHE'], $_SERVER['PAM_OCTANE_SHARED_CACHE']);

        parent::tearDown();
    }

    protected function defineEnvironment($app): void
    {
        parent::defineEnvironment($app);

        config(['octane.cache' => ['rows' => 100, 'bytes' => 1000]]);
    }

    public function test_octane_table_returns_the_shared_pam_table(): void
    {
        self::assertInstanceOf(OctaneManager::class, $this->application()->make('octane'));

        $table = Octane::table('example');

        self::assertInstanceOf(PamTable::class, $table);
        self::assertInstanceOf(\Swoole\Table::class, $table);
        $table->set('taylor', ['name' => 'Taylor', 'votes' => 3]);
        self::assertSame(3, Octane::table('example')->get('taylor', 'votes'));
    }

    public function test_unconfigured_tables_throw_like_octane(): void
    {
        $this->expectExceptionMessage('Swoole table [missing] has not been configured.');

        Octane::table('missing');
    }

    public function test_the_octane_cache_store_lives_in_shared_memory(): void
    {
        $store = Cache::store('octane')->getStore();

        self::assertInstanceOf(OctaneStore::class, $store);
        self::assertTrue(Cache::store('octane')->put('foo', 'bar', 60));
        self::assertSame('bar', Cache::store('octane')->get('foo'));
        self::assertSame(2, Cache::store('octane')->increment('counter', 2));
    }

    public function test_concurrently_runs_without_task_workers(): void
    {
        self::assertSame(['a' => 2, 'b' => 'pam'], Octane::concurrently([
            'a' => fn () => 1 + 1,
            'b' => fn () => 'pam',
        ]));
    }

    public function test_octane_routes_survive_the_manager(): void
    {
        Octane::route('GET', '/pam', fn () => new \Illuminate\Http\Response('pam'));

        self::assertTrue(Octane::hasRouteFor('GET', '/pam'));
    }
}
