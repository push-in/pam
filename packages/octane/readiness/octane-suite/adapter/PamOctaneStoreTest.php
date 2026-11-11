<?php

namespace Laravel\Octane\Tests;

use Pam\Octane\Tables\PamTable;
use Pam\Octane\Tables\PamTables;

/**
 * Octane's OctaneStoreTest with Octane's own OctaneStore on a pam-octane PAM
 * table, which is what the `octane` cache driver uses in PAM workers.
 */
class PamOctaneStoreTest extends OctaneStoreTest
{
    protected function setUp(): void
    {
        if (!PamTables::available()) {
            $this->markTestSkipped('Requires the PAM runtime.');
        }

        TestCase::setUp();
    }

    protected function createSwooleTable()
    {
        $table = new PamTable(1000);
        $table->column('value', PamTable::TYPE_STRING, 10000);
        $table->column('expiration', PamTable::TYPE_INT);
        $table->create();

        return $table;
    }
}
