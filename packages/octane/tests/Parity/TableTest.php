<?php

declare(strict_types=1);

namespace Pam\Octane\Tests\Parity;

use Laravel\Octane\Exceptions\ValueTooLargeForColumnException;
use Pam\Octane\Tables\PamTable;
use Pam\Octane\Tables\PamTables;
use PHPUnit\Framework\Attributes\DataProvider;
use PHPUnit\Framework\TestCase;

/**
 * laravel/octane's Swoole-only TableTest (tests/TableTest.php), ported to the
 * PAM shared-memory table. The three tests that build tables through
 * Octane's Swoole scripts build them through PamTables, which reads the same
 * `octane.tables` and `octane.cache` configuration.
 */
final class TableTest extends TestCase
{
    protected function setUp(): void
    {
        if (!PamTables::available()) {
            self::markTestSkipped('Requires the PAM runtime (run the suite with `pam test`).');
        }
    }

    public function test_it_gets_used_while_creating_an_octane_store(): void
    {
        $table = PamTables::cache(['rows' => 1000, 'bytes' => 10000]);

        self::assertInstanceOf(PamTable::class, $table);
        self::assertGreaterThanOrEqual(1000, $table->getSize());
    }

    public function test_it_gets_used_while_creating_an_table(): void
    {
        $tables = PamTables::fromConfig([
            'example:1000' => [
                'name' => 'string:1000',
                'votes' => 'int',
            ],
        ]);

        self::assertCount(1, $tables);
        self::assertInstanceOf(PamTable::class, $tables['example']);
    }

    public function test_it_gets_used_while_creating_an_timer_table(): void
    {
        // Swoole needs a timer table to enforce octane.max_execution_time;
        // PAM enforces it natively (a request past it is answered 504).
        $table = PamTables::cache(['rows' => 250, 'bytes' => 16]);

        self::assertInstanceOf(PamTable::class, $table);
    }

    public function test_can_use_all_available_rows(): void
    {
        $table = $this->createSwooleTable();

        for ($i = 0; $i < 1000; $i++) {
            $table->set((string) $i, ['string' => 'foo']);
        }

        self::assertSame(1000, $table->count());
    }

    #[DataProvider('validStringValues')]
    public function test_set_of_string_columns(string $value): void
    {
        $table = $this->createSwooleTable();

        $table->set('key', ['string' => $value]);

        $row = $table->get('key');
        self::assertIsArray($row);
        self::assertSame($value, $row['string']);
    }

    #[DataProvider('tooLargeStringValues')]
    public function test_ensures_string_sizes(string $value): void
    {
        $table = $this->createSwooleTable();

        $this->expectException(ValueTooLargeForColumnException::class);
        $this->expectExceptionMessage(sprintf(
            'Value [%s...] is too large for [string] column.',
            substr($value, 0, 20),
        ));

        $table->set('key', ['string' => $value]);
    }

    public function test_ensures_string_sizes_only_on_declared_columns(): void
    {
        $table = $this->createSwooleTable();

        $table->set('key', ['non-existing-column' => 'foo']);

        $row = $table->get('key');
        self::assertIsArray($row);
        self::assertArrayNotHasKey('non-existing-column', $row);
    }

    public function test_ensures_string_sizes_only_on_string_types(): void
    {
        $table = $this->createSwooleTable();

        $value = 1000000000000000000;

        $table->set('key', ['int' => $value]);

        $row = $table->get('key');
        self::assertIsArray($row);
        self::assertSame($value, $row['int']);
    }

    /** @return list<array{string}> */
    public static function validStringValues(): array
    {
        return [
            [str_repeat('a', 10)],
            ['taylortayl'],
            ['手 田日'],
            ['! p8VrB[]{'],
            ['N Rs*Gz2@h'],
            ['O:29:"Illu'],
            ['          '],
        ];
    }

    /** @return list<array{string}> */
    public static function tooLargeStringValues(): array
    {
        return [
            [str_repeat('a', 11)],
            ['taylortaylo'],
            ['手 田日尸'],
            ['! p8VrB[]{@'],
            ['N Rs*Gz2@hS'],
            ['O:29:"Illum'],
            ['           '],
        ];
    }

    private function createSwooleTable(): PamTable
    {
        $table = new PamTable(1000);
        $table->column('string', PamTable::TYPE_STRING, 10);
        $table->column('int', PamTable::TYPE_INT);
        $table->create();

        return $table;
    }
}
