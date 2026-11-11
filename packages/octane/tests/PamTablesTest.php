<?php

declare(strict_types=1);

namespace Pam\Octane\Tests;

use Laravel\Octane\Exceptions\ValueTooLargeForColumnException;
use Pam\Octane\PamOctane;
use Pam\Octane\Tables\PamTable;
use Pam\Octane\Tables\PamTables;
use PHPUnit\Framework\TestCase;

/**
 * Exercises the real shared tables when the suite runs inside PAM
 * (`pam vendor/bin/phpunit`); on plain PHP only the fallbacks run.
 */
final class PamTablesTest extends TestCase
{
    private function requirePam(): void
    {
        if (!PamTables::available()) {
            self::markTestSkipped('Shared tables need the PAM runtime.');
        }
    }

    public function testTablesAreUnavailableOutsideThePamRuntime(): void
    {
        self::assertSame(function_exists('pam_native_table_open'), PamTables::available());
    }

    public function testBuildsOctaneConfiguredTablesWithSwooleSemantics(): void
    {
        $this->requirePam();

        $tables = PamTables::fromConfig([
            'example:1000' => ['name' => 'string:10', 'votes' => 'int', 'score' => 'float'],
        ]);
        $table = $tables['example'];

        self::assertInstanceOf(PamTable::class, $table);
        self::assertSame(1024, $table->getSize());
        self::assertTrue($table->set('taylor', ['name' => 'Taylor', 'votes' => 1, 'ignored' => 'x']));
        self::assertSame(['name' => 'Taylor', 'votes' => 1, 'score' => 0.0], $table->get('taylor'));
        self::assertSame(3, $table->incr('taylor', 'votes', 2));
        self::assertSame(1.5, $table->incr('taylor', 'score', 1.5));
        self::assertSame(2, $table->decr('taylor', 'votes'));
        self::assertSame('Taylor', $table->get('taylor', 'name'));
        self::assertTrue($table->exists('taylor'));
        self::assertSame(1, $table->count());

        $rows = iterator_to_array($table);
        self::assertSame(['taylor'], array_keys($rows));

        self::assertTrue($table->del('taylor'));
        self::assertFalse($table->get('taylor'));
        self::assertFalse($table->del('taylor'));
    }

    public function testRejectsValuesLargerThanTheirColumnLikeOctane(): void
    {
        $this->requirePam();

        $table = new PamTable(8);
        $table->column('name', PamTable::TYPE_STRING, 4);
        $table->create();

        $this->expectException(ValueTooLargeForColumnException::class);
        $table->set('k', ['name' => 'too long']);
    }

    public function testLongKeysAreStoredWithoutCollisions(): void
    {
        $this->requirePam();

        $table = new PamTable(8);
        $table->column('v', PamTable::TYPE_INT);
        $table->create();
        $prefix = str_repeat('k', 100);

        $table->set($prefix.'1', ['v' => 1]);
        $table->set($prefix.'2', ['v' => 2]);

        self::assertSame(1, $table->get($prefix.'1', 'v'));
        self::assertSame(2, $table->get($prefix.'2', 'v'));
        self::assertSame(2, $table->count());
    }

    public function testFullTablesRefuseNewRowsButUpdateExistingOnes(): void
    {
        $this->requirePam();

        $table = new PamTable(2);
        $table->column('v', PamTable::TYPE_INT);
        $table->create();

        self::assertTrue($table->set('a', ['v' => 1]));
        self::assertTrue($table->set('b', ['v' => 1]));
        self::assertFalse($table->set('c', ['v' => 1]));
        self::assertTrue($table->set('a', ['v' => 5]));
        self::assertSame(5, $table->get('a', 'v'));
    }

    public function testRowsAreSharedWithForkedProcesses(): void
    {
        $this->requirePam();
        if (!function_exists('pcntl_fork')) {
            self::markTestSkipped('pcntl is not available.');
        }

        $table = new PamTable(64);
        $table->column('hits', PamTable::TYPE_INT);
        $table->create();

        $children = [];
        for ($child = 0; $child < 4; $child++) {
            $pid = pcntl_fork();
            if ($pid === 0) {
                for ($i = 0; $i < 1000; $i++) {
                    $table->incr('total', 'hits');
                }
                exit(0);
            }
            $children[] = $pid;
        }
        foreach ($children as $pid) {
            pcntl_waitpid($pid, $status);
        }

        self::assertSame(4000, $table->get('total', 'hits'));
    }

    public function testUnconfiguredTablesThrowLikeOctane(): void
    {
        $this->expectExceptionMessage('Octane table [missing] has not been configured.');

        PamOctane::table('missing');
    }
}
