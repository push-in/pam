<?php

declare(strict_types=1);

namespace Nimbus\Tests\Jobs;

use Arcano\ORM\ORM;
use Nimbus\Database\ArcanoMigrationManager;
use Nimbus\Jobs\DatabaseJobQueue;
use Nimbus\Jobs\FailedJobRepository;
use Nimbus\Jobs\JobCodec;
use Nimbus\Jobs\JobQueue;
use Nimbus\Jobs\QueueTablesMigration;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(DatabaseJobQueue::class)]
#[CoversClass(QueueTablesMigration::class)]
#[CoversClass(ArcanoMigrationManager::class)]
final class DatabaseJobQueueTest extends TestCase
{
    use PersistentQueueContract;

    private ?QueueDatabase $database = null;

    protected function setUp(): void
    {
        if (!class_exists(ORM::class)) {
            self::markTestSkipped('pushinbr/arcano is not installed.');
        }
        $this->database = QueueDatabase::create();
    }

    protected function tearDown(): void
    {
        $this->database?->drop();
        $this->database = null;
    }

    public function testTheMigrationCreatesTheQueueTables(): void
    {
        $db = $this->database()->databases->database();

        self::assertTrue($db->hasTable('jobs'));
        self::assertTrue($db->hasTable('failed_jobs'));
        self::assertTrue($db->hasTable('scheduler_locks'));
    }

    private function queue(string $name = 'default', ?JobCodec $codec = null): JobQueue&FailedJobRepository
    {
        return new DatabaseJobQueue($this->database()->databases, $codec ?? self::codec(), $name, clock: $this->clock());
    }

    private function database(): QueueDatabase
    {
        return $this->database ?? throw new \LogicException('The queue database is not ready.');
    }
}
