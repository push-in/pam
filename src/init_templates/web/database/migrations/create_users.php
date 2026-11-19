<?php

declare(strict_types=1);

namespace Migration;

use Arcano\Migrations\Migration;

class CreateUsersAndAuthTokens extends Migration
{
    protected const DATABASE = 'default';

    public function up(): void
    {
        $this->table('users')
            ->addColumn('id', 'primary', ['nullable' => false, 'defaultValue' => null])
            ->addColumn('name', 'string', ['nullable' => false, 'defaultValue' => null, 'size' => 120])
            ->addColumn('email', 'string', ['nullable' => false, 'defaultValue' => null, 'size' => 254])
            ->addColumn('password', 'string', ['nullable' => false, 'defaultValue' => null, 'size' => 255])
            ->addColumn('email_verified_at', 'datetime', ['nullable' => true, 'defaultValue' => null])
            ->addColumn('created_at', 'datetime', ['nullable' => false, 'defaultValue' => null])
            ->addIndex(['email'], ['unique' => true])
            ->setPrimaryKeys(['id'])
            ->create();

        // Single-use password reset tokens (Nimbus\Auth\OneTimeTokens); only SHA-256 hashes are stored.
        $this->table('auth_tokens')
            ->addColumn('id', 'string', ['nullable' => false, 'size' => 24])
            ->addColumn('purpose', 'string', ['nullable' => false, 'size' => 64])
            ->addColumn('subject', 'string', ['nullable' => false, 'size' => 191])
            ->addColumn('token_hash', 'string', ['nullable' => false, 'size' => 64])
            ->addColumn('expires_at', 'bigInteger', ['nullable' => false])
            ->addColumn('created_at', 'bigInteger', ['nullable' => false])
            ->addIndex(['purpose', 'subject'])
            ->addIndex(['expires_at'])
            ->setPrimaryKeys(['id'])
            ->create();
    }

    public function down(): void
    {
        $this->table('auth_tokens')->drop();
        $this->table('users')->drop();
    }
}
