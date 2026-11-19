<?php

declare(strict_types=1);

namespace Nimbus\Auth;

use Arcano\Migrations\Migration;

/**
 * Creates `personal_access_tokens` (PersonalAccessTokens), `refresh_tokens` (RefreshTokens),
 * `auth_tokens` (OneTimeTokens: password reset, e-mail verification), `revoked_tokens`
 * (DatabaseTokenRevocationStore) and `oauth_states` (DatabaseOAuthStateStore). Publish it with
 * ArcanoMigrationManager::publish('create_auth_tables', AuthTablesMigration::class).
 * Token columns hold SHA-256 hashes; times are Unix seconds.
 */
final class AuthTablesMigration extends Migration
{
    public function up(): void
    {
        $this->table('personal_access_tokens')
            ->addColumn('id', 'string', ['nullable' => false, 'size' => 24])
            ->addColumn('subject', 'string', ['nullable' => false, 'size' => 191])
            ->addColumn('name', 'string', ['nullable' => false, 'size' => 191])
            ->addColumn('token_hash', 'string', ['nullable' => false, 'size' => 64])
            ->addColumn('abilities', 'text', ['nullable' => false])
            ->addColumn('last_used_at', 'bigInteger', ['nullable' => true, 'defaultValue' => null])
            ->addColumn('expires_at', 'bigInteger', ['nullable' => true, 'defaultValue' => null])
            ->addColumn('created_at', 'bigInteger', ['nullable' => false])
            ->addIndex(['subject', 'created_at'])
            ->addIndex(['expires_at'])
            ->setPrimaryKeys(['id'])
            ->create();

        $this->table('refresh_tokens')
            ->addColumn('id', 'string', ['nullable' => false, 'size' => 24])
            ->addColumn('family_id', 'string', ['nullable' => false, 'size' => 24])
            ->addColumn('subject', 'string', ['nullable' => false, 'size' => 191])
            ->addColumn('token_hash', 'string', ['nullable' => false, 'size' => 64])
            ->addColumn('abilities', 'text', ['nullable' => false])
            ->addColumn('expires_at', 'bigInteger', ['nullable' => false])
            ->addColumn('used_at', 'bigInteger', ['nullable' => true, 'defaultValue' => null])
            ->addColumn('revoked_at', 'bigInteger', ['nullable' => true, 'defaultValue' => null])
            ->addColumn('created_at', 'bigInteger', ['nullable' => false])
            ->addIndex(['family_id'])
            ->addIndex(['subject'])
            ->addIndex(['expires_at'])
            ->setPrimaryKeys(['id'])
            ->create();

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

        $this->table('revoked_tokens')
            ->addColumn('id', 'string', ['nullable' => false, 'size' => 191])
            ->addColumn('expires_at', 'bigInteger', ['nullable' => false])
            ->addIndex(['expires_at'])
            ->setPrimaryKeys(['id'])
            ->create();

        $this->table('oauth_states')
            ->addColumn('id', 'string', ['nullable' => false, 'size' => 64])
            ->addColumn('payload', 'text', ['nullable' => false])
            ->addColumn('expires_at', 'bigInteger', ['nullable' => false])
            ->addIndex(['expires_at'])
            ->setPrimaryKeys(['id'])
            ->create();
    }

    public function down(): void
    {
        $this->table('oauth_states')->drop();
        $this->table('revoked_tokens')->drop();
        $this->table('auth_tokens')->drop();
        $this->table('refresh_tokens')->drop();
        $this->table('personal_access_tokens')->drop();
    }
}
