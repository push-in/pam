<?php

namespace Laravel\Octane\Tests;

use Pam\Octane\Tables\PamTable;
use Pam\Octane\Tables\PamTables;

/**
 * Octane's TableTest against pam-octane's PAM table (released Octane only
 * builds tables for Swoole). The three tests that require Octane's Swoole
 * table scripts check pam-octane's equivalent instead.
 */
class PamTableTest extends TableTest
{
    protected function setUp(): void
    {
        if (!PamTables::available()) {
            $this->markTestSkipped('Requires the PAM runtime.');
        }

        TestCase::setUp();
    }

    public function test_it_gets_used_while_creating_an_octane_store()
    {
        $table = PamTables::cache(['rows' => 1000, 'bytes' => 10000]);

        $this->assertInstanceOf(PamTable::class, $table);
        $this->assertSame(1024, $table->getSize());
    }

    public function test_it_gets_used_while_creating_an_table()
    {
        $tables = PamTables::fromConfig(['example:1000' => ['name' => 'string:1000', 'votes' => 'int']]);

        $this->assertCount(1, $tables);
        $this->assertInstanceOf(PamTable::class, $tables['example']);
        $tables['example']->set('taylor', ['name' => 'Taylor', 'votes' => 3]);
        $this->assertSame(['name' => 'Taylor', 'votes' => 3], $tables['example']->get('taylor'));
    }

    public function test_it_gets_used_while_creating_an_timer_table()
    {
        // PAM enforces Octane's max_execution_time natively; no timer table.
        $this->assertTrue(PamTables::available());
    }

    protected function createSwooleTable()
    {
        return tap(new PamTable(1000), function ($table) {
            $table->column('string', PamTable::TYPE_STRING, 10);
            $table->column('int', PamTable::TYPE_INT);

            $table->create();
        });
    }
}
