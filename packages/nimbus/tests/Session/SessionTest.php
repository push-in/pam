<?php

declare(strict_types=1);

namespace Nimbus\Tests\Session;

use Nimbus\Session\ErrorBag;
use Nimbus\Session\FileSessionStore;
use Nimbus\Session\MemorySessionStore;
use Nimbus\Session\Session;
use Nimbus\Session\SessionPayload;
use Nimbus\Session\SessionStore;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(Session::class)]
#[CoversClass(ErrorBag::class)]
#[CoversClass(MemorySessionStore::class)]
#[CoversClass(FileSessionStore::class)]
#[CoversClass(SessionPayload::class)]
final class SessionTest extends TestCase
{
    public function testFlashDataLivesForTheNextRequestOnly(): void
    {
        $session = new Session(Session::newId());
        $session->flash('status', 'saved');
        $session->now('instant', 'only now');
        self::assertSame('only now', $session->get('instant'));
        $session->ageFlashData();
        $afterFirst = $session->all();

        $next = new Session($session->id(), $session->all(), true);
        self::assertSame('saved', $next->get('status'));
        self::assertNull($next->get('instant'));
        $next->ageFlashData();

        $third = new Session($session->id(), $next->all(), true);
        self::assertNull($third->get('status'));

        $kept = new Session($session->id(), $afterFirst, true);
        $kept->reflash();
        $kept->ageFlashData();
        self::assertSame('saved', (new Session($session->id(), $kept->all(), true))->get('status'));
    }

    public function testOldInputAndErrors(): void
    {
        $session = new Session(Session::newId());
        $session->flashInput(['email' => 'a@b.c', 'password' => 'secret', '_token' => 't'])
            ->flashErrors(['email' => ['Taken.', 'Invalid.'], 'name' => 'Required.']);

        self::assertSame('a@b.c', $session->old('email'));
        self::assertNull($session->old('password'));
        self::assertSame('x', $session->old('missing', 'x'));
        $errors = $session->errors();
        self::assertSame('Taken.', $errors->first('email'));
        self::assertSame('Required.', $errors->first('name'));
        self::assertSame(['Taken.', 'Invalid.', 'Required.'], $errors->all());
        self::assertCount(3, $errors);
        self::assertTrue($errors->any());
        self::assertSame('Taken.', $errors->first());
        self::assertSame([], (new ErrorBag())->get('x'));
    }

    public function testRegenerationAndInvalidation(): void
    {
        $id = Session::newId();
        $session = new Session($id, ['user' => 1], true);
        $token = $session->token();
        self::assertSame($token, $session->token());
        self::assertSame(40, strlen($token));

        $session->regenerate();
        self::assertNotSame($id, $session->id());
        self::assertSame($id, $session->previousId());
        self::assertSame(1, $session->get('user'));

        $session->invalidate();
        self::assertNull($session->get('user'));
        self::assertNotSame($token, $session->token());
        self::assertSame($id, $session->previousId());

        self::assertSame(1, $session->put(['a' => 1])->pull('a'));
        self::assertFalse($session->has('a'));

        $this->expectException(\InvalidArgumentException::class);
        new Session('../../etc/passwd');
    }

    public function testCurrentSessionIsPerFiber(): void
    {
        $outer = new Session(Session::newId());
        Session::enter($outer);
        $inner = new Session(Session::newId());
        $seen = [];
        $fiber = new \Fiber(static function () use ($inner, &$seen): void {
            $seen[] = Session::current();
            Session::enter($inner);
            \Fiber::suspend();
            $seen[] = Session::current();
            Session::enter(null);
        });
        $fiber->start();
        self::assertSame($outer, Session::current());
        $fiber->resume();
        Session::enter(null);

        self::assertSame([null, $inner], $seen);
        self::assertNull(Session::current());
    }

    public function testMemoryStoreExpiresSessions(): void
    {
        $now = 1_000;
        $store = new MemorySessionStore(static function () use (&$now): int {
            return $now;
        });
        $this->assertStoreContract($store);
        $id = Session::newId();
        $store->write($id, ['a' => 1], 10);
        $now += 10;
        self::assertNull($store->read($id));
        self::assertSame(1, $store->gc());
    }

    public function testFileStore(): void
    {
        $directory = sys_get_temp_dir() . '/nimbus-sessions-' . bin2hex(random_bytes(4));
        $now = 1_000;
        $store = new FileSessionStore($directory, static function () use (&$now): int {
            return $now;
        });
        try {
            $this->assertStoreContract($store);
            $id = Session::newId();
            $store->write($id, ['a' => 1], 10);
            self::assertSame(0o600, fileperms($directory . '/' . $id) & 0o777);
            self::assertNull($store->read('../' . $id));
            $now += 11;
            self::assertSame(1, $store->gc());
            self::assertFileDoesNotExist($directory . '/' . $id);
        } finally {
            array_map('unlink', glob($directory . '/*') ?: []);
            @rmdir($directory);
        }
    }

    private function assertStoreContract(SessionStore $store): void
    {
        $id = Session::newId();
        self::assertNull($store->read($id));
        $store->write($id, ['user' => 7, 'list' => [1.5, 'x'], 'nested' => ['ok' => true]], 60);
        self::assertSame(['user' => 7, 'list' => [1.5, 'x'], 'nested' => ['ok' => true]], $store->read($id));
        $store->write($id, ['user' => 8], 60);
        self::assertSame(['user' => 8], $store->read($id));
        $store->destroy($id);
        self::assertNull($store->read($id));
    }
}
