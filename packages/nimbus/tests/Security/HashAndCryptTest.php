<?php

declare(strict_types=1);

namespace Nimbus\Tests\Security;

use Nimbus\Security\Crypt;
use Nimbus\Security\DecryptException;
use Nimbus\Security\Hash;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(Hash::class)]
#[CoversClass(Crypt::class)]
final class HashAndCryptTest extends TestCase
{
    public function testPasswordsHashAndVerify(): void
    {
        $hash = Hash::make('secret');
        self::assertTrue(Hash::check('secret', $hash));
        self::assertFalse(Hash::check('other', $hash));
        self::assertFalse(Hash::needsRehash($hash));
        self::assertTrue(Hash::needsRehash(password_hash('secret', PASSWORD_BCRYPT, ['cost' => 4])));
    }

    public function testEncryptedValuesRoundTripAndRejectTampering(): void
    {
        $key = Crypt::generateKey();
        $decoded = base64_decode(substr($key, 7), true);
        self::assertIsString($decoded);
        $crypt = new Crypt($decoded);

        $payload = $crypt->encrypt('card 4242');
        self::assertStringStartsWith('v1.', $payload);
        self::assertNotSame($payload, $crypt->encrypt('card 4242'));
        self::assertSame('card 4242', $crypt->decrypt($payload));

        $tampered = substr($payload, 0, -2) . (str_ends_with($payload, 'A') ? 'B' : 'A') . substr($payload, -1);
        $this->expectException(DecryptException::class);
        $crypt->decrypt($tampered);
    }
}
