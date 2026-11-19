<?php

declare(strict_types=1);

namespace Nimbus\Tests\Admin;

use Nimbus\Admin\Admin;
use Nimbus\Admin\AdminAction;
use Nimbus\Admin\AdminController;
use Nimbus\Admin\AdminField;
use Nimbus\Admin\AdminMiddleware;
use Nimbus\Admin\AdminPanel;
use Nimbus\Admin\AdminResource;
use Nimbus\Admin\EntityMetadata;
use Nimbus\App;
use Nimbus\Auth\Authenticator;
use Nimbus\Auth\Principal;
use Nimbus\Auth\SessionAuthenticator;
use Nimbus\Database\ArcanoManager;
use Nimbus\Session\MemorySessionStore;
use Nimbus\Session\SessionServiceProvider;
use Nimbus\Testing\TestClient;
use Nimbus\Testing\TestResponse;
use Nimbus\Tests\Fixtures\Arcano\Author;
use Nimbus\Tests\Fixtures\Arcano\Post;
use Nimbus\Tests\Testing\ArcanoTestCase;
use Nimbus\Validation\Attributes\Min;
use PHPUnit\Framework\Attributes\CoversClass;

#[CoversClass(Admin::class)]
#[CoversClass(AdminPanel::class)]
#[CoversClass(AdminResource::class)]
#[CoversClass(AdminField::class)]
#[CoversClass(AdminAction::class)]
#[CoversClass(AdminController::class)]
#[CoversClass(AdminMiddleware::class)]
#[CoversClass(EntityMetadata::class)]
final class AdminPanelTest extends ArcanoTestCase
{
    public function testGuestsAreSentToTheLoginPageAndNonAdminsGet403(): void
    {
        $this->client()->get('/admin/posts?q=x')->assertRedirect('/login?redirect=%2Fadmin%2Fposts%3Fq%3Dx');
        $this->client()->postJson('/admin/posts', [])->assertStatus(419);

        $this->actingAs(new AdminUser('guest', admin: false));
        $this->client()->get('/admin')->assertForbidden()->assertSee('You are not allowed to do this.');
        $this->client()->getJson('/admin/posts')->assertForbidden()->assertJsonPath('code', 3);
    }

    public function testDashboardListsResourcesWithCounts(): void
    {
        $this->seedPosts();
        $this->actingAs(new AdminUser('root'));

        $this->client()->get('/admin')->assertOk()
            ->assertSee('<a href="/admin/authors">Authors</a>', false)
            ->assertSee('3 records')
            ->assertSee('2 records')
            ->assertHeader('cache-control', 'no-store, private')
            ->assertHeader('x-frame-options', 'DENY');
        $this->client()->get('/admin/nothing')->assertNotFound();
    }

    public function testListSearchesFiltersSortsAndPaginates(): void
    {
        $this->seedPosts();
        $this->actingAs(new AdminUser('root'));

        $page = $this->client()->get('/admin/posts')->assertOk()
            ->assertSee('Hello PAM')
            ->assertSee('Ada')
            ->assertSee('>Publish</option>', false)
            ->assertSee('name="f[published]"', false);
        self::assertLessThan(strpos($page->body(), 'Hello PAM'), strpos($page->body(), 'Draft notes'), 'Newest first by default.');

        $this->client()->get('/admin/posts?q=pam')->assertSee('Hello PAM')->assertDontSee('Draft notes');
        $this->client()->get('/admin/posts?f[published]=0')->assertSee('Draft notes')->assertDontSee('Hello PAM');
        $grace = $this->arcano()->select(Author::class)->where('name', 'Grace')->fetchOne();
        self::assertInstanceOf(Author::class, $grace);
        $this->client()->get('/admin/posts?f[author]=' . $grace->id)->assertSee('Release notes')->assertDontSee('Hello PAM');

        $sorted = $this->client()->get('/admin/posts?sort=title&dir=asc')->assertOk()->body();
        self::assertLessThan(strpos($sorted, 'Hello PAM'), strpos($sorted, 'Draft notes'));
        self::assertStringContainsString('aria-sort="ascending"', $sorted);

        $authors = $this->client()->get('/admin/authors')->assertOk()->assertSee('Page 1 of 2')->assertSee('rel="next"', false);
        self::assertStringNotContainsString('Ada', $authors->body(), 'Authors sort by name descending, one per page.');
        $this->client()->get('/admin/authors?page=2')->assertSee('Ada');
    }

    public function testCreateValidatesAndStores(): void
    {
        $this->seedPosts();
        $this->actingAs(new AdminUser('root'));
        [$browser, $token] = $this->browser('/admin/posts/create');

        $invalid = $browser->post('/admin/posts', ['_token' => $token, 'title' => '', 'author' => '999', 'published' => '1'])
            ->assertRedirect('/admin/posts/create');
        $browser = $browser->withCookiesFrom($invalid);
        $browser->get('/admin/posts/create')->assertOk()
            ->assertSee('The Title field is required.')
            ->assertSee('The Author field is invalid.')
            ->assertSee('aria-invalid="true"', false);

        $author = $this->arcano()->select(Author::class)->where('name', 'Ada')->fetchOne();
        self::assertInstanceOf(Author::class, $author);
        $browser->post('/admin/posts', ['_token' => $token, 'title' => 'Fresh', 'author' => (string) $author->id, 'published' => '0'])
            ->assertRedirect('/admin/posts');
        $this->assertDatabaseHas('posts', ['title' => 'Fresh', 'author_id' => $author->id, 'published' => false]);
        $browser->get('/admin/posts')->assertSee('Fresh created.');

        // An AdminResource Input-less rule: author names need at least 2 characters.
        $browser->post('/admin/authors', ['_token' => $token, 'name' => 'X'])->assertRedirect('/admin/authors/create');
        $this->assertDatabaseMissing('authors', ['name' => 'X']);
    }

    public function testCsrfTokenIsRequired(): void
    {
        $this->actingAs(new AdminUser('root'));
        [$browser] = $this->browser('/admin/authors/create');

        $browser->post('/admin/authors', ['name' => 'No token'])->assertStatus(419);
        $browser->post('/admin/authors', ['name' => 'Wrong', '_token' => str_repeat('a', 40)])->assertStatus(419);
        $this->assertDatabaseCount('authors', 0);
    }

    public function testEditUpdatesAndPoliciesProtectRecords(): void
    {
        $posts = $this->seedPosts();
        $this->actingAs(new AdminUser('root'));
        [$browser, $token] = $this->browser('/admin/posts/' . $posts['draft'] . '/edit');

        $browser->get('/admin/posts/' . $posts['draft'] . '/edit')->assertOk()
            ->assertSee('value="Draft notes"', false)
            ->assertSee('name="_method" value="PUT"', false);
        $browser->post('/admin/posts/' . $posts['draft'], ['_token' => $token, '_method' => 'PUT', 'title' => 'Edited', 'author' => (string) $posts['ada'], 'published' => '1'])
            ->assertRedirect('/admin/posts');
        $this->assertDatabaseHas('posts', ['id' => $posts['draft'], 'title' => 'Edited', 'published' => true]);

        // PostPolicy: only the owner of the "editor" role may change published posts.
        $this->actingAs(new AdminUser('editor'));
        [$editor, $editorToken] = $this->browser('/admin/posts');
        $editor->get('/admin/posts/' . $posts['hello'] . '/edit')->assertForbidden();
        $editor->post('/admin/posts/' . $posts['hello'], ['_token' => $editorToken, '_method' => 'PUT', 'title' => 'Nope'])->assertForbidden();
        $editor->post('/admin/posts/' . $posts['hello'], ['_token' => $editorToken, '_method' => 'DELETE'])->assertForbidden();
        $this->assertDatabaseHas('posts', ['id' => $posts['hello'], 'title' => 'Hello PAM']);
        $editor->get('/admin/posts/9999/edit')->assertNotFound();
    }

    public function testDeleteAndBulkActions(): void
    {
        $posts = $this->seedPosts();
        $this->actingAs(new AdminUser('root'));
        [$browser, $token] = $this->browser('/admin/posts');

        $browser->post('/admin/posts/' . $posts['draft'], ['_token' => $token, '_method' => 'DELETE'])->assertRedirect('/admin/posts');
        $this->assertDatabaseMissing('posts', ['id' => $posts['draft']]);

        $browser->post('/admin/posts/bulk', ['_token' => $token, 'action' => 'publish', 'ids' => [(string) $posts['release']]])
            ->assertRedirect('/admin/posts');
        $this->assertDatabaseHas('posts', ['id' => $posts['release'], 'published' => true]);
        $browser->get('/admin/posts')->assertSee('1 published.');

        $browser->post('/admin/posts/bulk', ['_token' => $token, 'action' => 'delete', 'ids' => [(string) $posts['hello'], (string) $posts['release']], '_back' => '?page=1'])
            ->assertRedirect('/admin/posts?page=1');
        $this->assertDatabaseCount('posts', 0);
        $browser->post('/admin/posts/bulk', ['_token' => $token, 'action' => 'explode', 'ids' => ['1']])->assertNotFound();
    }

    public function testAssetsAndDefaultFields(): void
    {
        $this->client()->get('/admin/_assets/admin.css')->assertOk()
            ->assertHeader('content-type', 'text/css; charset=utf-8')
            ->assertSee('prefers-color-scheme: dark');
        $css = (string) file_get_contents(dirname(__DIR__, 2) . '/resources/admin/admin.css');
        self::assertDoesNotMatchRegularExpression('/radial-gradient|conic-gradient/', $css);

        $metadata = EntityMetadata::of($this->arcano()->orm()->getSchema(), Post::class);
        self::assertSame('id', $metadata->primaryKey);
        self::assertSame(['id', 'title', 'published', 'author'], array_keys($metadata->fields));
        self::assertSame('id', $metadata->fields['id']->type);
        self::assertSame('text', $metadata->fields['title']->type);
        self::assertSame(255, $metadata->fields['title']->maxLength);
        self::assertTrue($metadata->fields['title']->required);
        self::assertSame('boolean', $metadata->fields['published']->type);
        self::assertFalse($metadata->fields['published']->required);
        self::assertSame('belongsTo', $metadata->fields['author']->type);
        self::assertSame(Author::class, $metadata->fields['author']->target);
        self::assertSame(['author' => 'author_id'], $metadata->innerKeys);
        self::assertSame(120, EntityMetadata::of($this->arcano()->orm()->getSchema(), Author::class)->fields['name']->maxLength);

        self::assertSame('blog-posts', (new class (Post::class) extends AdminResource {
            public function slug(): string
            {
                return 'blog-' . parent::slug();
            }
        })->slug());
        self::assertSame('Created at', AdminField::headline('createdAt'));
        self::assertSame(['draft' => 'Draft'], AdminField::select('status', ['draft' => 'Draft'])->options);
    }

    protected function createApplication(): App
    {
        $app = parent::createApplication();
        $app->provider(new SessionServiceProvider(new MemorySessionStore()));
        $container = $app->container();
        $container->instance(Authenticator::class, new SessionAuthenticator($container, static fn (string $id): ?Principal => null));
        $app->gate()
            ->define('viewAdmin', static fn (Principal $user): bool => $user instanceof AdminUser && $user->admin)
            ->policy(Post::class, PostPolicy::class);
        Admin::panel($app, '/admin', loginUrl: '/login')
            ->resource(AuthorAdmin::class)
            ->resource(new PostAdmin());
        return $app;
    }

    /** @return array{TestClient, string} A client with the session of the page and its CSRF token. */
    private function browser(string $page): array
    {
        $response = $this->client()->get($page)->assertOk();
        return [$this->client()->withCookiesFrom($response), self::token($response)];
    }

    private static function token(TestResponse $response): string
    {
        preg_match('/name="_token" value="([a-f0-9]{40})"/', $response->body(), $match);
        self::assertArrayHasKey(1, $match, 'The page has a CSRF field.');
        return $match[1] ?? '';
    }

    /** @return array<string, int> */
    private function seedPosts(): array
    {
        $arcano = $this->arcano();
        $manager = $arcano->entityManager();
        $ada = new Author();
        $ada->name = 'Ada';
        $grace = new Author();
        $grace->name = 'Grace';
        $posts = [];
        foreach ([['hello', 'Hello PAM', true, $ada], ['release', 'Release notes', false, $grace], ['draft', 'Draft notes', false, $ada]] as [$key, $title, $published, $author]) {
            $post = new Post();
            $post->title = $title;
            $post->published = $published;
            $post->author = $author;
            $manager->persist($post);
            $posts[$key] = $post;
        }
        $manager->run();
        return array_map(static fn (object $entity): int => (int) $entity->id, $posts + ['ada' => $ada, 'grace' => $grace]);
    }
}

final class AuthorAdmin extends AdminResource
{
    protected string $entity = Author::class;

    public function fields(array $fields): array
    {
        $fields['name']->rules(new Min(2));
        return $fields;
    }

    public function perPage(): int
    {
        return 1;
    }

    /** @return array{string, 'asc'|'desc'} */
    public function defaultSort(): array
    {
        return ['name', 'desc'];
    }
}

final class PostAdmin extends AdminResource
{
    public function __construct()
    {
        parent::__construct(Post::class);
    }

    public function actions(): array
    {
        return ['publish' => new AdminAction('Publish', static function (array $posts, ArcanoManager $arcano): string {
            $manager = $arcano->entityManager();
            foreach ($posts as $post) {
                if ($post instanceof Post) {
                    $post->published = true;
                    $manager->persist($post);
                }
            }
            $manager->run();
            return count($posts) . ' published.';
        })];
    }
}

final class PostPolicy
{
    public function viewAny(Principal $user): bool
    {
        return true;
    }

    public function create(Principal $user): bool
    {
        return true;
    }

    public function update(Principal $user, Post $post): bool
    {
        return $user->identifier() === 'root' || !$post->published;
    }

    public function delete(Principal $user, Post $post): bool
    {
        return $this->update($user, $post);
    }
}

final readonly class AdminUser implements Principal
{
    public function __construct(private string $id, public bool $admin = true)
    {
    }

    public function identifier(): string
    {
        return $this->id;
    }

    public function can(string $ability): bool
    {
        return false;
    }
}
