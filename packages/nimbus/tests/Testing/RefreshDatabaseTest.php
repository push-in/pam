<?php

declare(strict_types=1);

namespace Nimbus\Tests\Testing;

use Nimbus\App;
use Nimbus\Database\ArcanoDatabaseProvider;
use Nimbus\Database\ArcanoManager;
use Nimbus\Testing\RefreshDatabase;
use Nimbus\Testing\TestCase;
use Nimbus\Tests\Fixtures\Arcano\Author;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\Attributes\Depends;
use Pam\Http\Request;
use Pam\Http\Response;

#[CoversClass(TestCase::class)]
#[CoversClass(RefreshDatabase::class)]
#[CoversClass(ArcanoDatabaseProvider::class)]
final class RefreshDatabaseTest extends ArcanoTestCase
{
    public function testRequestsShareTheTestTransaction(): void
    {
        $this->postJson('/authors', ['name' => 'Ada'])
            ->assertCreated()
            ->assertJsonPath('data.name', 'Ada');
        $this->postJson('/authors', ['name' => 'Grace'])->assertCreated();

        $this->getJson('/authors')
            ->assertOk()
            ->assertJsonCount(2, 'data')
            ->assertJsonPath('data.0.name', 'Ada');
        $this->assertDatabaseHas('authors', ['name' => 'Ada']);
        $this->assertDatabaseHas(Author::class, ['name' => 'Grace']);
        $this->assertDatabaseMissing('authors', ['name' => 'Linus']);
        $this->assertDatabaseCount('authors', 2);
    }

    #[Depends('testRequestsShareTheTestTransaction')]
    public function testThePreviousTestWasRolledBack(): void
    {
        $this->assertDatabaseCount('authors', 0);
        $this->getJson('/authors')->assertOk()->assertExactJson(['data' => []]);
    }

    public function testTransactionsNestInTheTestTransaction(): void
    {
        $this->arcano()->transaction(function (): void {
            $author = new Author();
            $author->name = 'Barbara';
            $this->arcano()->entityManager()->persist($author)->run();
        });
        $this->assertDatabaseCount(Author::class, 1);

        // A request that leaves its transaction open only loses its own writes.
        $this->postJson('/leaky')->assertNoContent();
        $this->assertDatabaseCount(Author::class, 1);
        $this->postJson('/authors', ['name' => 'Edsger'])->assertCreated();
        $this->assertDatabaseCount(Author::class, 2);
    }

    protected function createApplication(): App
    {
        $app = parent::createApplication();
        $app->post('/leaky', static function (Request $request, Response $response, ArcanoManager $arcano): Response {
            $arcano->database()->begin();
            $arcano->database()->execute("INSERT INTO authors (name) VALUES ('Left open')");
            return $response->status(204)->send(null);
        });
        return $app;
    }
}
