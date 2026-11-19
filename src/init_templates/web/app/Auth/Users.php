<?php

declare(strict_types=1);

namespace App\Auth;

use App\Models\User;
use Nimbus\Auth\AuthUser;
use Nimbus\Auth\AuthUserProvider;
use Nimbus\Database\ArcanoManager;
use Nimbus\Security\Hash;

/** The users table, as seen by the login, registration and password reset screens. */
final readonly class Users implements AuthUserProvider
{
    public function __construct(private ArcanoManager $arcano)
    {
    }

    public function find(string $id): ?User
    {
        if (!ctype_digit($id)) {
            return null;
        }
        $user = $this->arcano->select(User::class)->wherePK((int) $id)->fetchOne();
        return $user instanceof User ? $user : null;
    }

    public function findById(string $identifier): ?AuthUser
    {
        return $this->find($identifier);
    }

    public function findByEmail(string $email): ?User
    {
        $user = $this->arcano->select(User::class)->where('email', self::normalize($email))->fetchOne();
        return $user instanceof User ? $user : null;
    }

    /** Returns the user whose password matches, or null; the hash is checked even for unknown e-mails. */
    public function attempt(string $email, #[\SensitiveParameter] string $password): ?User
    {
        static $unknown = null;
        $user = $this->findByEmail($email);
        // Unknown e-mails cost the same hash check, so timing does not reveal which accounts exist.
        $unknown ??= Hash::make(bin2hex(random_bytes(16)));
        $valid = Hash::check($password, $user?->password ?? $unknown);
        if ($valid && $user !== null && Hash::needsRehash($user->password)) {
            $this->updatePassword($user, Hash::make($password));
        }
        return $valid ? $user : null;
    }

    public function create(string $name, string $email, #[\SensitiveParameter] string $password): User
    {
        $user = new User();
        $user->name = $name;
        $user->email = self::normalize($email);
        $user->password = Hash::make($password);
        $user->createdAt = new \DateTimeImmutable();
        $this->arcano->entityManager()->persist($user)->run();
        return $user;
    }

    public function updatePassword(AuthUser $user, string $passwordHash): void
    {
        if (!$user instanceof User) {
            return;
        }
        $user->password = $passwordHash;
        $this->arcano->entityManager()->persist($user)->run();
    }

    public function markEmailAsVerified(AuthUser $user): void
    {
        if (!$user instanceof User) {
            return;
        }
        $user->emailVerifiedAt = new \DateTimeImmutable();
        $this->arcano->entityManager()->persist($user)->run();
    }

    public static function normalize(string $email): string
    {
        return strtolower(trim($email));
    }
}
