<?php

declare(strict_types=1);

namespace Nimbus\Tests\Profiler;

use Nimbus\App;
use Nimbus\Auth\Authenticator;
use Nimbus\Auth\Principal;
use Nimbus\Http\Client\Factory;
use Nimbus\Http\Client\Http;
use Nimbus\Http\Client\HttpFake;
use Nimbus\Jobs\InvokableJobHandler;
use Nimbus\Jobs\JobOutcome;
use Nimbus\Jobs\QueueFake;
use Nimbus\Mail\MailFake;
use Nimbus\Mail\Mailer;
use Nimbus\Notifications\Mail\MailMessage;
use Nimbus\Profiler\DebugEntry;
use Nimbus\Profiler\DebugEntryType;
use Nimbus\Profiler\DebugPanel;
use Nimbus\Profiler\DebugPanelMiddleware;
use Nimbus\Profiler\DebugRecorder;
use Nimbus\Profiler\RecordingHttpTransport;
use Nimbus\Profiler\RecordingJobDispatcher;
use Nimbus\Profiler\RecordingJobHandler;
use Nimbus\Profiler\RecordingMailTransport;
use Nimbus\Testing\TestClient;
use Nimbus\Tests\Broadcasting\HeaderAuthenticator;
use Pam\Http\Response;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(DebugPanel::class)]
#[CoversClass(DebugPanelMiddleware::class)]
#[CoversClass(DebugRecorder::class)]
#[CoversClass(DebugEntry::class)]
#[CoversClass(DebugEntryType::class)]
#[CoversClass(RecordingJobDispatcher::class)]
#[CoversClass(RecordingJobHandler::class)]
#[CoversClass(RecordingMailTransport::class)]
#[CoversClass(RecordingHttpTransport::class)]
final class DebugPanelTest extends TestCase
{
    protected function tearDown(): void
    {
        Http::swap(null);
    }

    public function testItIsDisabledOutsideDevelopment(): void
    {
        self::assertFalse(DebugPanel::enabledByDefault([]));
        self::assertFalse(DebugPanel::enabledByDefault(['APP_ENV' => 'production']));
        self::assertFalse(DebugPanel::enabledByDefault(['APP_ENV' => 'local', 'APP_DEBUG' => 'false']));
        self::assertTrue(DebugPanel::enabledByDefault(['PAM_ENV' => 'development']));
        self::assertNull(DebugPanel::register(new App(discoverPackages: false), enabled: false));
    }

    public function testItRecordsRequestsExceptionsJobsMailsAndHttpCalls(): void
    {
        $app = new App(discoverPackages: false);
        $panel = DebugPanel::register($app, enabled: true);
        self::assertInstanceOf(DebugPanel::class, $panel);
        $app->container()->instance(Authenticator::class, new HeaderAuthenticator());
        $app->gate()->define('viewDebugPanel', static fn (Principal $user): bool => $user->identifier() === 'dev');
        $jobs = $panel->jobs(new QueueFake());
        $mail = new MailFake();
        $mailer = new Mailer($panel->mailTransport($mail), 'app@example.test');
        $http = new HttpFake(['*' => ['ok' => true]]);
        Http::swap(new Factory($http));
        $panel->recordHttpFacade();
        $app->post('/orders', static function (Response $response) use ($jobs, $mailer): Response {
            $jobs->dispatch(new \ArrayObject(), 2, 5);
            $mailer->to('ana@example.test')->send(MailMessage::make()->subject('Hi')->line('Body'));
            Http::get('https://api.example.test/sync?token=abc&x=1');
            return $response->json(['ok' => true], 201);
        });
        $app->get('/boom', static fn (): never => throw new \RuntimeException('Exploded'));
        $client = new TestClient($app->boot());

        $client->postJson('/orders', ['password' => 'hunter2', 'name' => 'x'], ['authorization' => 'Bearer s'])->assertStatus(201);
        $client->get('/boom')->assertStatus(500);

        $recorder = $panel->recorder();
        [$boom, $orders] = $recorder->entries(DebugEntryType::Request);
        self::assertSame('/orders', $orders->data['path']);
        self::assertSame(201, $orders->data['status']);
        self::assertSame(['password' => '********', 'name' => 'x'], $orders->data['body']);
        self::assertIsArray($orders->data['headers']);
        self::assertSame('********', $orders->data['headers']['authorization']);
        self::assertSame(\RuntimeException::class, $boom->data['exception']);
        [$exception] = $recorder->entries(DebugEntryType::Exception);
        self::assertSame('Exploded', $exception->data['message']);
        self::assertSame($boom->requestToken, $exception->requestToken);
        $related = $recorder->entries(null, $orders->requestToken);
        self::assertSame(['request', 'http', 'mail', 'job'], array_map(static fn (DebugEntry $entry): string => $entry->type->value, $related));
        [$call] = $recorder->entries(DebugEntryType::Http);
        self::assertSame('https://api.example.test/sync?token=********&x=1', $call->data['url']);
        self::assertSame(['ana@example.test'], $recorder->entries(DebugEntryType::Mail)[0]->data['to']);
        self::assertSame(['Hi'], array_map(static fn ($email) => $email->getSubject(), $mail->sent()));

        $client->getJson('/_debug/entries')->assertStatus(401);
        $client->getJson('/_debug/entries', ['x-user' => 'someone'])->assertStatus(403);
        $dev = ['x-user' => 'dev'];
        $list = $client->getJson('/_debug/entries?type=request', $dev)->assertOk();
        $list->assertJsonCount(2, 'data');
        $client->getJson('/_debug/entries/' . $orders->id, $dev)->assertOk()->assertJsonCount(3, 'data.related');
        $client->getJson('/_debug/entries/missing', $dev)->assertStatus(404);
        $page = $client->get('/_debug', $dev)->assertOk();
        self::assertStringContainsString('Nimbus debug panel', $page->body());
        self::assertCount(2, $recorder->entries(DebugEntryType::Request), 'Panel requests are not recorded.');
    }

    public function testTheJobHandlerRecordsRunsAndFailures(): void
    {
        $recorder = new DebugRecorder();
        $handler = new RecordingJobHandler(new InvokableJobHandler(), $recorder);

        self::assertSame(JobOutcome::Complete, $handler->handle(new class {
            public function handle(): void
            {
            }
        }));
        try {
            $handler->handle(new class {
                public function handle(): never
                {
                    throw new \DomainException('bad job');
                }
            });
        } catch (\DomainException) {
        }

        $jobs = $recorder->entries(DebugEntryType::Job);
        self::assertSame(['failed', 'processed'], array_map(static fn (DebugEntry $entry): mixed => $entry->data['event'], $jobs));
        self::assertCount(1, $recorder->entries(DebugEntryType::Exception));
        $recorder->clear();
        self::assertSame([], $recorder->entries());
    }

    public function testCapacityIsBounded(): void
    {
        $recorder = new DebugRecorder(3);
        for ($index = 0; $index < 5; ++$index) {
            $recorder->query('select ' . $index, 1.0);
        }
        self::assertSame(['select 4', 'select 3', 'select 2'], array_map(
            static fn (DebugEntry $entry): mixed => $entry->data['sql'],
            $recorder->entries(),
        ));
        self::assertNull($recorder->find('nope'));
        $this->expectException(\InvalidArgumentException::class);
        new DebugRecorder(0);
    }
}
