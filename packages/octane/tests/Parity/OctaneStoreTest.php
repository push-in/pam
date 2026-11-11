<?php

declare(strict_types=1);

namespace Pam\Octane\Tests\Parity;

use Illuminate\Support\Carbon;
use Illuminate\Support\Str;
use Laravel\Octane\Cache\OctaneStore;
use Orchestra\Testbench\TestCase;
use Pam\Octane\Tables\PamTable;
use Pam\Octane\Tables\PamTables;

/**
 * laravel/octane's Swoole-only OctaneStoreTest (tests/OctaneStoreTest.php):
 * Octane's own `octane` cache store on the PAM shared-memory cache table,
 * which is what the store uses in PAM workers.
 */
final class OctaneStoreTest extends TestCase
{
    protected function setUp(): void
    {
        if (!PamTables::available()) {
            self::markTestSkipped('Requires the PAM runtime (run the suite with `pam test`).');
        }

        parent::setUp();
    }

    public function test_can_retrieve_items_from_store(): void
    {
        $table = $this->createSwooleTable();

        $table->set('foo', ['value' => serialize('bar'), 'expiration' => time() + 100]);

        $store = new OctaneStore($table);

        self::assertEquals('bar', $store->get('foo'));
    }

    public function test_missing_items_return_null(): void
    {
        $store = new OctaneStore($this->createSwooleTable());

        self::assertNull($store->get('foo'));
    }

    public function test_expired_items_return_null(): void
    {
        $table = $this->createSwooleTable();

        $store = new OctaneStore($table);

        $table->set('foo', ['value' => serialize('bar'), 'expiration' => time() - 100]);

        self::assertNull($store->get('foo'));
    }

    public function test_get_method_can_resolve_pending_interval(): void
    {
        $store = new OctaneStore($this->createSwooleTable());

        $store->interval('foo', fn () => 'bar', 1);

        self::assertEquals('bar', $store->get('foo'));
    }

    public function test_many_method_can_return_many_values(): void
    {
        $table = $this->createSwooleTable();

        $table->set('foo', ['value' => serialize('bar'), 'expiration' => time() + 100]);
        $table->set('bar', ['value' => serialize('baz'), 'expiration' => time() + 100]);

        $store = new OctaneStore($table);

        self::assertEquals(['foo' => 'bar', 'bar' => 'baz'], $store->many(['foo', 'bar']));
    }

    public function test_put_stores_value_in_table(): void
    {
        $store = new OctaneStore($this->createSwooleTable());

        $store->put('foo', 'bar', 5);

        self::assertEquals('bar', $store->get('foo'));
    }

    public function test_put_many_stores_value_in_table(): void
    {
        $store = new OctaneStore($this->createSwooleTable());

        $store->putMany(['foo' => 'bar', 'bar' => 'baz'], 5);

        self::assertEquals('bar', $store->get('foo'));
        self::assertEquals('baz', $store->get('bar'));
    }

    public function test_increment_and_decrement_operations(): void
    {
        $store = new OctaneStore($this->createSwooleTable());

        $store->increment('counter');
        self::assertEquals(1, $store->get('counter'));

        $store->increment('counter', 2);
        self::assertEquals(3, $store->get('counter'));

        $store->decrement('counter', 2);
        self::assertEquals(1, $store->get('counter'));
    }

    public function test_forever_stores_value_in_table(): void
    {
        $store = new OctaneStore($this->createSwooleTable());

        $store->forever('foo', 'bar');

        self::assertEquals('bar', $store->get('foo'));
    }

    public function test_intervals_can_be_refreshed(): void
    {
        $store = new OctaneStore($this->createSwooleTable());

        $store->interval('foo', fn () => Str::random(10), 1);

        self::assertIsString($first = $store->get('foo'));

        Carbon::setTestNow(Carbon::now()->addMinutes(1));

        $store->refreshIntervalCaches();

        self::assertIsString($second = $store->get('foo'));
        self::assertNotEquals($first, $second);

        Carbon::setTestNow();
    }

    public function test_touch_updates_expiration(): void
    {
        $this->requireTouch();

        $store = new OctaneStore($this->createSwooleTable());

        $store->put('foo', 'bar', 5);

        Carbon::setTestNow(Carbon::now()->addSeconds(3));

        $store->touch('foo', 10);

        Carbon::setTestNow(Carbon::now()->addSeconds(8));

        self::assertEquals('bar', $store->get('foo'));

        Carbon::setTestNow();
    }

    public function test_touch_returns_false_for_missing_items(): void
    {
        $this->requireTouch();

        $store = new OctaneStore($this->createSwooleTable());

        self::assertFalse($store->touch('foo', 10));
    }

    public function test_touch_returns_false_for_expired_items(): void
    {
        $this->requireTouch();

        $store = new OctaneStore($this->createSwooleTable());

        $store->put('foo', 'bar', 5);

        Carbon::setTestNow(Carbon::now()->addSeconds(10));

        self::assertFalse($store->touch('foo', 10));

        Carbon::setTestNow();
    }

    public function test_can_forget_cache_items(): void
    {
        $store = new OctaneStore($this->createSwooleTable());

        $store->put('foo', 'bar', 5);
        self::assertTrue($store->forget('foo'));

        self::assertNull($store->get('foo'));

        $store->put('foo', 'bar', 5);
        self::assertTrue($store->flush());

        self::assertNull($store->get('foo'));
    }

    public function test_intervals_are_not_flushed(): void
    {
        $store = new OctaneStore($this->createSwooleTable());

        $store->interval('foo', fn () => 'bar', 1);
        self::assertTrue($store->flush());

        self::assertEquals('bar', $store->get('foo'));
    }

    private function requireTouch(): void
    {
        if (!in_array('touch', get_class_methods(OctaneStore::class), true)) {
            self::markTestSkipped('Requires OctaneStore::touch method');
        }
    }

    private function createSwooleTable(): PamTable
    {
        $table = new PamTable(1000);
        $table->column('value', PamTable::TYPE_STRING, 10000);
        $table->column('expiration', PamTable::TYPE_INT);
        $table->create();

        return $table;
    }
}
