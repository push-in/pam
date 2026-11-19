<?php

declare(strict_types=1);

namespace Nimbus\Tests\Auth;

use Nimbus\Auth\AuthTablesMigration;
use Nimbus\Database\ArcanoDatabaseProvider;
use Nimbus\Database\ArcanoMigrationManager;
use Nimbus\Tests\Database\ArcanoTestDatabase;

/** A migrated database with the auth tables, removed by drop(). */
final class AuthTestDatabase
{
    public readonly ArcanoDatabaseProvider $databases;

    private readonly ArcanoMigrationManager $migrations;

    private function __construct(private readonly string $directory)
    {
        mkdir($directory . '/migrations', 0o777, true);
        $this->databases = new ArcanoDatabaseProvider(ArcanoTestDatabase::config($directory . '/auth.sqlite'));
        $this->migrations = new ArcanoMigrationManager($this->databases, $directory . '/migrations');
    }

    public static function create(): self
    {
        $database = new self(sys_get_temp_dir() . '/nimbus-auth-' . bin2hex(random_bytes(6)));
        if ($database->migrations->publish('create_auth_tables', AuthTablesMigration::class) === null) {
            throw new \LogicException('The auth migration must publish.');
        }
        $database->migrations->migrate();
        return $database;
    }

    public function drop(): void
    {
        $this->migrations->rollback();
        $this->databases->releaseCurrent();
        $this->databases->disconnectIdle();
        foreach (glob($this->directory . '/migrations/*') ?: [] as $file) {
            unlink($file);
        }
        @rmdir($this->directory . '/migrations');
        @unlink($this->directory . '/auth.sqlite');
        @rmdir($this->directory);
    }
}
